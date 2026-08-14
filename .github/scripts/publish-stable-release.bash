#!/usr/bin/env bash
set -euo pipefail

# Publishes the stable release to this repository.
#
# This used to mirror every step to a second, public repository with a separate
# personal access token, back when the source repository was private. The
# repository is public now and both names resolved to it, so each step ran twice
# against the same release -- the second time with a token still scoped to the
# retired mirror.

: "${TAG_NAME:?TAG_NAME is required}"
: "${VERSION:?VERSION is required}"
: "${RELEASE_NAME:?RELEASE_NAME is required}"
: "${IS_PRERELEASE:?IS_PRERELEASE is required}"
: "${REPO:?REPO is required}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}"

export GH_TOKEN="$GITHUB_TOKEN"

ASSETS_DIR="${ASSETS_DIR:-release-assets}"
RELEASE_BODY_PATH="${RELEASE_BODY_PATH:-release.txt}"

# GitHub's "get a release by tag name" REST endpoint only returns published
# releases — it 404s for drafts, because a draft's tag isn't a real git ref
# yet. `gh api repos/.../releases/tags/$TAG_NAME` hits that endpoint
# directly, so any script code needing a draft's release id must list
# releases and filter by tag_name instead, which works for drafts too.
release_id_for_tag() {
  gh api "repos/$REPO/releases" --paginate \
    --jq "[.[] | select(.tag_name == \"$TAG_NAME\")][0].id"
}

ensure_draft_release() {
  if gh release view "$TAG_NAME" --repo "$REPO" >/dev/null 2>&1; then
    return
  fi

  local prerelease_flag=()
  if [[ "$IS_PRERELEASE" == "true" ]]; then
    prerelease_flag=(--prerelease)
  fi

  gh release create "$TAG_NAME" \
    --repo "$REPO" \
    --target main \
    --title "$RELEASE_NAME" \
    --notes-file "$RELEASE_BODY_PATH" \
    --draft \
    "${prerelease_flag[@]}"
}

clean_release_assets() {
  local names
  names="$(gh release view "$TAG_NAME" \
    --repo "$REPO" --json assets -q '.assets[].name' 2>/dev/null || true)"

  while IFS= read -r name; do
    [[ -z "$name" ]] && continue
    [[ "$name" == *"$VERSION"* ]] && continue
    gh release delete-asset \
      "$TAG_NAME" "$name" --repo "$REPO" --yes || true
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

  gh release upload \
    "$TAG_NAME" "${files[@]}" --repo "$REPO" --clobber
}

finalize_release() {
  gh release edit "$TAG_NAME" \
    --repo "$REPO" \
    --title "$RELEASE_NAME" \
    --notes-file "$RELEASE_BODY_PATH"

  local release_id
  release_id="$(release_id_for_tag)"

  if [[ "$IS_PRERELEASE" == "true" ]]; then
    gh api \
      --method PATCH \
      "repos/$REPO/releases/$release_id" \
      -F draft=false \
      -F prerelease=true \
      -f make_latest=false >/dev/null
  else
    gh api \
      --method PATCH \
      "repos/$REPO/releases/$release_id" \
      -F draft=false \
      -F prerelease=false \
      -f make_latest=true >/dev/null
  fi
}

publish_stable_updater() {
  local release_id
  release_id="$(release_id_for_tag)"
  gh api \
    -H "Accept: application/vnd.github+json" \
    "repos/$REPO/releases/$release_id" > release-assets.json

  VERSION="$VERSION" UPDATE_VERSION="$TAG_NAME" \
    NOTES_FILE="$RELEASE_BODY_PATH" \
    node .github/scripts/generate-tauri-latest-json.mjs \
    release-assets.json update.json

  cp update.json update-proxy.json
  sha256sum update.json > update.json.sha256
  sha256sum update-proxy.json > update-proxy.json.sha256

  if ! gh release view updater --repo "$REPO" >/dev/null 2>&1; then
    gh release create updater \
      --repo "$REPO" \
      --target main \
      --title "Auto-update Stable Channel" \
      --notes "Stable updater metadata for Celestial." \
      --latest=false
  fi

  gh release upload updater \
    update.json update.json.sha256 \
    update-proxy.json update-proxy.json.sha256 \
    --repo "$REPO" --clobber

  # This release holds nothing but `update.json`, yet it is published after the real one
  # and GitHub hands "latest" to whichever was published last. That left
  # /releases/latest — and every `gh release download` and install script that follows
  # it — pointing at a release with no installers in it. Pin it back to the version tag.
  gh api \
    --method PATCH \
    "repos/$REPO/releases/$(gh api "repos/$REPO/releases/tags/updater" --jq .id)" \
    -F draft=false \
    -F prerelease=false \
    -f make_latest=false >/dev/null

  if [[ "$IS_PRERELEASE" != "true" ]]; then
    gh api \
      --method PATCH \
      "repos/$REPO/releases/$(release_id_for_tag)" \
      -f make_latest=true >/dev/null
  fi
}

# A second copy of each downloadable artefact under a name with the version taken out.
#
# GitHub resolves /releases/latest/download/<name> against an exact asset name, and ours
# carry the version, so no link to "the current build" could exist. With these,
# .../releases/latest/download/Celestial_arm64-setup.exe always lands on the newest
# release.
#
# Two things make this safe. The updater ignores them: generate-tauri-latest-json.mjs
# skips every asset whose name does not contain the version. And they must be uploaded
# last, because clean_release_assets deletes precisely the assets that lack the version —
# which is how a stale alias from an earlier run gets cleared before this rewrites it.
stable_alias_for() {
  local name="$1"
  case "$name" in
    # Signatures and checksums belong to the versioned files; the updater payloads
    # (.app.tar.gz) and the Play bundle (.aab) are not things anyone downloads by hand.
    *.sig | *.sha256 | *.app.tar.gz | *.aab) return 1 ;;
    # rpm names are name-version-release.arch.rpm, so Celestial-3.0.0-1.x86_64.rpm has a
    # release number after the version too. Drop both, leaving Celestial-x86_64.rpm.
    Celestial-"$VERSION"-*)
      local rest="${name#Celestial-"$VERSION"-}"
      printf 'Celestial-%s' "${rest#*.}"
      ;;
    Celestial_"$VERSION"_*) printf 'Celestial_%s' "${name#Celestial_"$VERSION"_}" ;;
    *) return 1 ;;
  esac
}

upload_stable_aliases() {
  local alias_dir="${ASSETS_DIR}-stable-names"
  rm -rf "$alias_dir"
  mkdir -p "$alias_dir"

  local file name alias
  shopt -s nullglob
  for file in "$ASSETS_DIR"/*; do
    [[ -f "$file" ]] || continue
    name="$(basename "$file")"
    alias="$(stable_alias_for "$name")" || continue
    cp "$file" "$alias_dir/$alias"
    # Regenerate rather than copy the checksum: the original names the versioned file
    # inside it, so `sha256sum -c` on the alias would fail on a name mismatch.
    (cd "$alias_dir" && sha256sum "$alias" > "$alias.sha256")
  done
  local aliases=("$alias_dir"/*)
  shopt -u nullglob

  ((${#aliases[@]} > 0)) || return 0
  gh release upload "$TAG_NAME" "${aliases[@]}" --repo "$REPO" --clobber
}

ensure_draft_release
clean_release_assets
upload_release_assets
finalize_release

# Must run after finalize_release: until the release is un-drafted, GitHub
# hasn't created the real "$TAG_NAME" git tag yet, so `releases/tags/$TAG_NAME`
# 404s.
if [[ "$IS_PRERELEASE" != "true" ]]; then
  publish_stable_updater
  # Only the stable channel gets them: a prerelease is never "latest", so a stable
  # link pointing into one would be wrong.
  upload_stable_aliases
fi
