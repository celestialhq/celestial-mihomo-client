<h1 align="center">
  ☁️
  <br />
  Celestial
</h1>

<h3 align="center">
  A desktop Mihomo / Clash Meta client built with Tauri, React, and Rust.
</h3>

<p align="center">
  <strong>English</strong> · <a href="./README_ru.md">Русский</a>
</p>

> Celestial is a fork of [clash-verge-rev](https://github.com/clash-verge-rev/clash-verge-rev). It keeps the Clash Verge Rev foundation and adapts it for the Celestial client experience.

## What It Is

Celestial is a cross-platform GUI for [Mihomo](https://github.com/MetaCubeX/mihomo), also known as Clash Meta. It manages remote and local profiles, proxy groups, rules, connections, logs, system proxy settings, TUN mode, backups, and desktop integrations from one polished interface.

The app is based on the Clash Verge Rev codebase, but the packaged product name, window title, and subscription-management additions are branded for Celestial.

## Features

- Embedded Mihomo core with support for stable and alpha sidecar binaries.
- Profile management for remote subscriptions and local YAML files.
- Profile extension tools, including merge configuration and script-based customization.
- Proxy group, node, rule, connection, and log pages.
- System proxy, proxy guard, PAC editing, TUN mode, DNS, port, and controller settings.
- Visual editors for proxies, groups, rules, tunnels, and Web UI entries.
- Customizable UI with light, dark, system theme mode, color settings, font settings, CSS injection, collapsible navigation, and reorderable menu items.
- Home dashboard with profile, current proxy, network mode, traffic, IP, Mihomo, and system information cards.
- Tray integration, hotkeys, lightweight mode, update checks, and platform-specific desktop behavior.
- Configuration backup, restore, backup history, automatic backup settings, and WebDAV sync.
- Celestial subscription management for eligible `socelestial.com` remote profiles, including traffic, status, expiration, and bound-device controls.

## Platforms

The upstream project supports Windows, macOS, and Linux. This fork uses the same Tauri 2 desktop stack and keeps platform-specific packaging resources under `src-tauri`.

## Development

Install the normal [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your operating system first. Then run:

```shell
pnpm i
pnpm run prebuild
pnpm dev
```

Useful commands:

```shell
pnpm typecheck
pnpm lint
pnpm run web:build
pnpm build
```

## Project Layout

- `src/` - React application, pages, components, services, hooks, locales, and assets.
- `src-tauri/` - Tauri application, Rust backend, platform configuration, installers, sidecar resources, and capabilities.
- `crates/` - Shared Rust crates used by the desktop app.
- `scripts/` - Build, release, updater, portable, and i18n helper scripts.
- `docs/` - README translations, previews, and historical documentation.

## Upstream

Celestial inherits a large amount of functionality from Clash Verge Rev and its lineage:

- [clash-verge-rev/clash-verge-rev](https://github.com/clash-verge-rev/clash-verge-rev) - the upstream project this repository forks.
- [zzzgydi/clash-verge](https://github.com/zzzgydi/clash-verge) - the original Clash Verge desktop GUI.
- [MetaCubeX/mihomo](https://github.com/MetaCubeX/mihomo) - the rule-based proxy core.
- [tauri-apps/tauri](https://github.com/tauri-apps/tauri) - the desktop app framework.
- [vitejs/vite](https://github.com/vitejs/vite) - frontend tooling.

## License

This project is distributed under the GPL-3.0-only license. See [LICENSE](../LICENSE) for details.
