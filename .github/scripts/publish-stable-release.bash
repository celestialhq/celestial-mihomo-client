#!/usr/bin/env bash
set -euo pipefail

: "${TAG_NAME:?TAG_NAME is required}"
: "${VERSION:?VERSION is required}"
: "${RELEASE_NAME:?RELEASE_NAME is required}"
: "${IS_PRERELEASE:?IS_PRERELEASE is required}"
: "${PUBLIC_REPO:?PUBLIC_REPO is required}"
: "${PUBLIC_RELEASE_TOKEN:?PUBLIC_RELEASE_TOKEN is required}"

ASSETS_DIR="${ASSETS_DIR:-release-assets}"
RELEASE_BODY_PATH="${RELEASE_BODY_PATH:-release.txt}"

ensure_draft_release() {
  if GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh release view "$TAG_NAME" \
    --repo "$PUBLIC_REPO" >/dev/null 2>&1; then
    return
  fi

  local prerelease_flag=()
  if [[ "$IS_PRERELEASE" == "true" ]]; then
    prerelease_flag=(--prerelease)
  fi

  GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh release create "$TAG_NAME" \
    --repo "$PUBLIC_REPO" \
    --target main \
    --title "$RELEASE_NAME" \
    --notes-file "$RELEASE_BODY_PATH" \
    --draft \
    "${prerelease_flag[@]}"
}

clean_release_assets() {
  local names
  names="$(GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh release view "$TAG_NAME" \
    --repo "$PUBLIC_REPO" --json assets -q '.assets[].name' 2>/dev/null || true)"

  while IFS= read -r name; do
    [[ -z "$name" ]] && continue
    [[ "$name" == *"$VERSION"* ]] && continue
    GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh release delete-asset \
      "$TAG_NAME" "$name" --repo "$PUBLIC_REPO" --yes || true
  done <<< "$names"
}

upload_release_assets() {
  shopt -s nullglob
  local files=("$ASSETS_DIR"/*)
  shopt -u nullglob
  ((${#files[@]} > 0)) || {
    echo "No release assets found in $ASSETS_DIR"
    exit 1
  }

  GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh release upload \
    "$TAG_NAME" "${files[@]}" --repo "$PUBLIC_REPO" --clobber
}

publish_stable_updater() {
  GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh api \
    -H "Accept: application/vnd.github+json" \
    "repos/$PUBLIC_REPO/releases/tags/$TAG_NAME" > release-assets.json

  VERSION="$VERSION" UPDATE_VERSION="$TAG_NAME" \
    NOTES_FILE="$RELEASE_BODY_PATH" \
    node .github/scripts/generate-tauri-latest-json.mjs \
    release-assets.json update.json

  cp update.json update-proxy.json
  sha256sum update.json > update.json.sha256
  sha256sum update-proxy.json > update-proxy.json.sha256

  if ! GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh release view updater \
    --repo "$PUBLIC_REPO" >/dev/null 2>&1; then
    GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh release create updater \
      --repo "$PUBLIC_REPO" \
      --target main \
      --title "Auto-update Stable Channel" \
      --notes "Stable updater metadata for Celestial."
  fi

  GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh release upload updater \
    update.json update.json.sha256 \
    update-proxy.json update-proxy.json.sha256 \
    --repo "$PUBLIC_REPO" --clobber
}

finalize_release() {
  GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh release edit "$TAG_NAME" \
    --repo "$PUBLIC_REPO" \
    --title "$RELEASE_NAME" \
    --notes-file "$RELEASE_BODY_PATH"

  local release_id
  release_id="$(GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh api \
    "repos/$PUBLIC_REPO/releases/tags/$TAG_NAME" --jq '.id')"

  if [[ "$IS_PRERELEASE" == "true" ]]; then
    GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh api \
      --method PATCH \
      "repos/$PUBLIC_REPO/releases/$release_id" \
      -F draft=false \
      -F prerelease=true \
      -f make_latest=false >/dev/null
  else
    GH_TOKEN="$PUBLIC_RELEASE_TOKEN" gh api \
      --method PATCH \
      "repos/$PUBLIC_REPO/releases/$release_id" \
      -F draft=false \
      -F prerelease=false \
      -f make_latest=true >/dev/null
  fi
}

ensure_draft_release
clean_release_assets
upload_release_assets

if [[ "$IS_PRERELEASE" != "true" ]]; then
  publish_stable_updater
fi

finalize_release
