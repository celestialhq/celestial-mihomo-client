#!/usr/bin/env bash
set -euo pipefail

: "${TAG_NAME:?TAG_NAME is required}"
: "${VERSION:?VERSION is required}"
: "${RELEASE_NAME:?RELEASE_NAME is required}"
: "${PRIVATE_REPO:?PRIVATE_REPO is required}"
: "${PUBLIC_REPO:?PUBLIC_REPO is required}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}"
: "${PUBLIC_RELEASE_TOKEN:?PUBLIC_RELEASE_TOKEN is required}"

ASSETS_DIR="${ASSETS_DIR:-./release-assets}"
RELEASE_BODY_PATH="${RELEASE_BODY_PATH:-release.txt}"
LATEST_JSON_PATH="${LATEST_JSON_PATH:-latest.json}"
RELEASE_ASSETS_JSON="${RELEASE_ASSETS_JSON:-release-assets.json}"

ensure_release() {
  local repo="$1"
  local token="$2"

  if GH_TOKEN="$token" gh release view "$TAG_NAME" --repo "$repo" >/dev/null 2>&1; then
    GH_TOKEN="$token" gh release edit "$TAG_NAME" \
      --repo "$repo" \
      --title "$RELEASE_NAME" \
      --notes-file "$RELEASE_BODY_PATH"
  else
    GH_TOKEN="$token" gh release create "$TAG_NAME" \
      --repo "$repo" \
      --title "$RELEASE_NAME" \
      --notes-file "$RELEASE_BODY_PATH" \
      --prerelease
  fi
}

upload_assets() {
  local repo="$1"
  local token="$2"

  shopt -s nullglob
  local files=("$ASSETS_DIR"/*)
  shopt -u nullglob

  if [ "${#files[@]}" -eq 0 ]; then
    echo "No release assets found in $ASSETS_DIR"
    exit 1
  fi

  GH_TOKEN="$token" gh release upload "$TAG_NAME" "${files[@]}" \
    --repo "$repo" \
    --clobber
}

clean_stale_assets() {
  local repo="$1"
  local token="$2"
  local assets

  assets="$(GH_TOKEN="$token" gh release view "$TAG_NAME" --repo "$repo" --json assets -q '.assets[].name' || true)"
  if [ -z "$assets" ]; then
    return
  fi

  while IFS= read -r asset; do
    if [ -z "$asset" ]; then
      continue
    fi

    if [[ "$asset" == *"$VERSION"* ]] || [[ "$asset" == "latest.json" ]]; then
      continue
    fi

    echo "Deleting stale asset from $repo: $asset"
    GH_TOKEN="$token" gh release delete-asset "$TAG_NAME" "$asset" \
      --repo "$repo" \
      -y || true
  done <<< "$assets"
}

upload_latest_json() {
  local repo="$1"
  local token="$2"

  GH_TOKEN="$token" gh release upload "$TAG_NAME" "$LATEST_JSON_PATH" \
    --repo "$repo" \
    --clobber
}

ensure_release "$PRIVATE_REPO" "$GITHUB_TOKEN"
ensure_release "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"

upload_assets "$PRIVATE_REPO" "$GITHUB_TOKEN"
upload_assets "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"

clean_stale_assets "$PRIVATE_REPO" "$GITHUB_TOKEN"
clean_stale_assets "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"

GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh api \
  -H "Accept: application/vnd.github+json" \
  "repos/$PUBLIC_REPO/releases/tags/$TAG_NAME" \
  > "$RELEASE_ASSETS_JSON"

VERSION="$VERSION" NOTES_FILE="$RELEASE_BODY_PATH" \
  node scripts-workflow/generate-tauri-latest-json.mjs "$RELEASE_ASSETS_JSON" "$LATEST_JSON_PATH"

upload_latest_json "$PRIVATE_REPO" "$GITHUB_TOKEN"
upload_latest_json "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"
