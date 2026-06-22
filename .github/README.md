# Build and release workflows

This directory contains the active development autobuild and stable release
pipelines.

The previous GitHub configuration is archived in `.old-github` and is not
loaded by GitHub Actions.

Required secrets:

- `PUBLIC_RELEASE_TOKEN` — token with release write access to
  `pius-pp/celestial-mihomo-client-public`.
- `TAURI_PRIVATE_KEY` and `TAURI_KEY_PASSWORD` — required for stable releases
  and their updater metadata; optional for development builds.
- `TELEGRAM_BOT_TOKEN` — optional.

The macOS jobs intentionally do not use Apple signing or notarization secrets.
WebView2 fixed-runtime builds are intentionally not produced.
