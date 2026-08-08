#!/usr/bin/env bash
set -euo pipefail

# Publishes the release to this repository.
#
# This used to mirror every step to a second, public repository with a separate
# personal access token, back when the source repository was private. The
# repository is public now and both names resolved to it, so each step ran twice
# against the same release -- the second time with a token still scoped to the
# retired mirror, which answered 403 and failed the job after the first pass had
# already published everything.

: "${TAG_NAME:?TAG_NAME is required}"
: "${VERSION:?VERSION is required}"
: "${RELEASE_NAME:?RELEASE_NAME is required}"
: "${REPO:?REPO is required}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}"

export GH_TOKEN="$GITHUB_TOKEN"

ASSETS_DIR="${ASSETS_DIR:-release-assets}"
RELEASE_BODY_PATH="${RELEASE_BODY_PATH:-release.txt}"

ensure_release() {
  if gh release view "$TAG_NAME" --repo "$REPO" >/dev/null 2>&1; then
    gh release edit "$TAG_NAME" \
      --repo "$REPO" \
      --title "$RELEASE_NAME" \
      --notes-file "$RELEASE_BODY_PATH" \
      --prerelease
  else
    gh release create "$TAG_NAME" \
      --repo "$REPO" \
      --title "$RELEASE_NAME" \
      --notes-file "$RELEASE_BODY_PATH" \
      --prerelease
  fi
}

clean_stale_assets() {
  local names
  names="$(gh release view "$TAG_NAME" \
    --repo "$REPO" --json assets -q '.assets[].name' 2>/dev/null || true)"

  while IFS= read -r name; do
    [[ -z "$name" ]] && continue
    [[ "$name" == *"$VERSION"* ]] && continue
    gh release delete-asset "$TAG_NAME" "$name" \
      --repo "$REPO" --yes || true
  done <<< "$names"
}

upload_directory() {
  shopt -s nullglob
  local files=("$ASSETS_DIR"/*)
  shopt -u nullglob
  ((${#files[@]} > 0)) || {
    echo "No release assets found in $ASSETS_DIR"
    exit 1
  }
  gh release upload "$TAG_NAME" "${files[@]}" \
    --repo "$REPO" --clobber
}

upload_file() {
  local file="$1"
  gh release upload "$TAG_NAME" "$file" \
    --repo "$REPO" --clobber
}

ensure_release
clean_stale_assets
upload_directory

gh api \
  -H "Accept: application/vnd.github+json" \
  "repos/$REPO/releases/tags/$TAG_NAME" > release-assets.json

set +e
VERSION="$VERSION" BUILD_COMMIT="${BUILD_COMMIT:-}" \
  NOTES_FILE="$RELEASE_BODY_PATH" \
  node .github/scripts/generate-tauri-latest-json.mjs \
  release-assets.json latest.json
latest_status=$?
set -e

if [[ "$latest_status" -eq 0 ]]; then
  sha256sum latest.json > latest.json.sha256
  upload_file latest.json
  upload_file latest.json.sha256
elif [[ "$latest_status" -ne 2 ]]; then
  exit "$latest_status"
fi

# Apply the final body after every upload so the release page changes atomically
# from a reader's perspective.
ensure_release
