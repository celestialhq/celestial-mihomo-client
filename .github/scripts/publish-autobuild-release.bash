#!/usr/bin/env bash
set -euo pipefail

: "${TAG_NAME:?TAG_NAME is required}"
: "${VERSION:?VERSION is required}"
: "${RELEASE_NAME:?RELEASE_NAME is required}"
: "${PRIVATE_REPO:?PRIVATE_REPO is required}"
: "${PUBLIC_REPO:?PUBLIC_REPO is required}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}"
: "${PUBLIC_RELEASE_TOKEN:?PUBLIC_RELEASE_TOKEN is required}"

ASSETS_DIR="${ASSETS_DIR:-release-assets}"
RELEASE_BODY_PATH="${RELEASE_BODY_PATH:-release.txt}"

ensure_release() {
  local repo="$1"
  local token="$2"

  if GH_TOKEN="$token" gh release view "$TAG_NAME" --repo "$repo" >/dev/null 2>&1; then
    GH_TOKEN="$token" gh release edit "$TAG_NAME" \
      --repo "$repo" \
      --title "$RELEASE_NAME" \
      --notes-file "$RELEASE_BODY_PATH" \
      --prerelease
  else
    GH_TOKEN="$token" gh release create "$TAG_NAME" \
      --repo "$repo" \
      --title "$RELEASE_NAME" \
      --notes-file "$RELEASE_BODY_PATH" \
      --prerelease
  fi
}

clean_stale_assets() {
  local repo="$1"
  local token="$2"
  local names
  names="$(GH_TOKEN="$token" gh release view "$TAG_NAME" \
    --repo "$repo" --json assets -q '.assets[].name' 2>/dev/null || true)"

  while IFS= read -r name; do
    [[ -z "$name" ]] && continue
    [[ "$name" == *"$VERSION"* ]] && continue
    GH_TOKEN="$token" gh release delete-asset "$TAG_NAME" "$name" \
      --repo "$repo" --yes || true
  done <<< "$names"
}

upload_directory() {
  local repo="$1"
  local token="$2"
  shopt -s nullglob
  local files=("$ASSETS_DIR"/*)
  shopt -u nullglob
  ((${#files[@]} > 0)) || {
    echo "No release assets found in $ASSETS_DIR"
    exit 1
  }
  GH_TOKEN="$token" gh release upload "$TAG_NAME" "${files[@]}" \
    --repo "$repo" --clobber
}

upload_file() {
  local repo="$1"
  local token="$2"
  local file="$3"
  GH_TOKEN="$token" gh release upload "$TAG_NAME" "$file" \
    --repo "$repo" --clobber
}

ensure_release "$PRIVATE_REPO" "$GITHUB_TOKEN"
ensure_release "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"
clean_stale_assets "$PRIVATE_REPO" "$GITHUB_TOKEN"
clean_stale_assets "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"
upload_directory "$PRIVATE_REPO" "$GITHUB_TOKEN"
upload_directory "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"

GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh api \
  -H "Accept: application/vnd.github+json" \
  "repos/$PUBLIC_REPO/releases/tags/$TAG_NAME" > release-assets.json

set +e
VERSION="$VERSION" NOTES_FILE="$RELEASE_BODY_PATH" \
  node .github/scripts/generate-tauri-latest-json.mjs \
  release-assets.json latest.json
latest_status=$?
set -e

if [[ "$latest_status" -eq 0 ]]; then
  sha256sum latest.json > latest.json.sha256
  upload_file "$PRIVATE_REPO" "$GITHUB_TOKEN" latest.json
  upload_file "$PRIVATE_REPO" "$GITHUB_TOKEN" latest.json.sha256
  upload_file "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN" latest.json
  upload_file "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN" latest.json.sha256
elif [[ "$latest_status" -ne 2 ]]; then
  exit "$latest_status"
fi

# Apply the final body after every upload so the release page changes atomically
# from a reader's perspective.
ensure_release "$PRIVATE_REPO" "$GITHUB_TOKEN"
ensure_release "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"
