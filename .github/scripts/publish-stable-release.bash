#!/usr/bin/env bash
set -euo pipefail

: "${TAG_NAME:?TAG_NAME is required}"
: "${VERSION:?VERSION is required}"
: "${RELEASE_NAME:?RELEASE_NAME is required}"
: "${IS_PRERELEASE:?IS_PRERELEASE is required}"
: "${PRIVATE_REPO:?PRIVATE_REPO is required}"
: "${PUBLIC_REPO:?PUBLIC_REPO is required}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}"
: "${PUBLIC_RELEASE_TOKEN:?PUBLIC_RELEASE_TOKEN is required}"

ASSETS_DIR="${ASSETS_DIR:-release-assets}"
RELEASE_BODY_PATH="${RELEASE_BODY_PATH:-release.txt}"

ensure_draft_release() {
  local repo="$1"
  local token="$2"

  if GH_TOKEN="$token" gh release view "$TAG_NAME" \
    --repo "$repo" >/dev/null 2>&1; then
    return
  fi

  local prerelease_flag=()
  if [[ "$IS_PRERELEASE" == "true" ]]; then
    prerelease_flag=(--prerelease)
  fi

  GH_TOKEN="$token" gh release create "$TAG_NAME" \
    --repo "$repo" \
    --target main \
    --title "$RELEASE_NAME" \
    --notes-file "$RELEASE_BODY_PATH" \
    --draft \
    "${prerelease_flag[@]}"
}

clean_release_assets() {
  local repo="$1"
  local token="$2"
  local names
  names="$(GH_TOKEN="$token" gh release view "$TAG_NAME" \
    --repo "$repo" --json assets -q '.assets[].name' 2>/dev/null || true)"

  while IFS= read -r name; do
    [[ -z "$name" ]] && continue
    [[ "$name" == *"$VERSION"* ]] && continue
    GH_TOKEN="$token" gh release delete-asset \
      "$TAG_NAME" "$name" --repo "$repo" --yes || true
  done <<< "$names"
}

upload_release_assets() {
  local repo="$1"
  local token="$2"
  shopt -s nullglob
  local files=("$ASSETS_DIR"/*)
  shopt -u nullglob
  ((${#files[@]} > 0)) || {
    echo "No release assets found in $ASSETS_DIR"
    exit 1
  }

  GH_TOKEN="$token" gh release upload \
    "$TAG_NAME" "${files[@]}" --repo "$repo" --clobber
}

finalize_release() {
  local repo="$1"
  local token="$2"

  GH_TOKEN="$token" gh release edit "$TAG_NAME" \
    --repo "$repo" \
    --title "$RELEASE_NAME" \
    --notes-file "$RELEASE_BODY_PATH"

  local release_id
  release_id="$(GH_TOKEN="$token" gh api \
    "repos/$repo/releases/tags/$TAG_NAME" --jq '.id')"

  if [[ "$IS_PRERELEASE" == "true" ]]; then
    GH_TOKEN="$token" gh api \
      --method PATCH \
      "repos/$repo/releases/$release_id" \
      -F draft=false \
      -F prerelease=true \
      -f make_latest=false >/dev/null
  else
    GH_TOKEN="$token" gh api \
      --method PATCH \
      "repos/$repo/releases/$release_id" \
      -F draft=false \
      -F prerelease=false \
      -f make_latest=true >/dev/null
  fi
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

ensure_draft_release "$PRIVATE_REPO" "$GITHUB_TOKEN"
ensure_draft_release "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"
clean_release_assets "$PRIVATE_REPO" "$GITHUB_TOKEN"
clean_release_assets "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"
upload_release_assets "$PRIVATE_REPO" "$GITHUB_TOKEN"
upload_release_assets "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"

finalize_release "$PRIVATE_REPO" "$GITHUB_TOKEN"
finalize_release "$PUBLIC_REPO" "$PUBLIC_RELEASE_TOKEN"

# Must run after finalize_release: until the public release is un-drafted,
# GitHub hasn't created the real "$TAG_NAME" git tag yet, so
# `releases/tags/$TAG_NAME` 404s.
if [[ "$IS_PRERELEASE" != "true" ]]; then
  publish_stable_updater
fi
