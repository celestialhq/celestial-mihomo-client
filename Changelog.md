## v4.1.0

### New

- **The relay works on Android.** The xray core is linked into the app the same way mihomo already is, and reads the same generated `xray.json` the desktop build writes — so the TLS fingerprint and the REALITY / Vision / XHTTP implementations come from xray on a phone exactly as they do on a desktop. The v4.0.0 note that called the relay desktop-only no longer holds: there is no second process to spawn on Android, which was a fact about how the core was shipped rather than about the platform.
- **Shipped for arm64-v8a.** Every Android phone of the last several years is arm64; the core builds cleanly for the other ABIs but adds about 33 MB each to an already large universal APK. A device it is not shipped for loses the relay, not the app — the nodes are dialled natively, which is what mobile did before this existed, and the settings say so rather than showing a control that does nothing.
- **Hysteria2 nodes go through the relay now.** They were held back on the grounds that xray's implementation was unproven; the core we ship carries them, and on a real subscription that was two nodes in eleven dialled natively for no reason the user could see. Nodes using salamander obfuscation stay native — xray has no counterpart for it, and relaying one with its obfuscation quietly gone is worse than not relaying it.

### Changed

- **The per-node relay/native switch is gone.** Whether a node is relayed was never a preference: it is relayed when xray can carry it and an outbound exists for it, and it stays native when one of those is false — conclusions that asking cannot change, because asking does not give xray a protocol it lacks. Offering the choice also put the one escape hatch from a pinned-on mode in the hands of whoever clicked it, when going native is meant to be the client's answer to something being wrong. The node list and the reason each node is where it is both stay: losing the control does not mean losing the explanation.
- **The embedded mihomo core moves to v1.19.30**, which is what desktop downloads. Android compiles the core from a pinned submodule while desktop resolves `releases/latest` on every build, so the two only agreed when someone moved the pin by hand — and nobody had since June, leaving two months of upstream fixes on one platform and not the other. A weekly job now reads the same version the prebuild script reads and opens a pull request when they differ, rather than leaving it to be noticed.

### Fixed

- **Turning on global mode killed the connection.** mihomo decides that mode before it reaches the rule engine, so the rule that kept the relay's own traffic out of the tunnel was never consulted: xray dialled its node, the tunnel caught it, and global mode sent it straight back to xray. Every connection became a loop. The relay's traffic now leaves through a dedicated mihomo listener that is answered before the mode is looked at, so it holds in every mode — and the core's own DNS goes the same way, which is the half that deadlocked rather than merely circling. Anyone who has tried global mode since 4.0.0 was hitting this.
- A node whose certificate mihomo was told not to check was verified strictly through the relay. `skip-cert-verify` was being dropped for every protocol, so a node with a self-signed certificate worked natively and failed the moment it was relayed.
- **The Android build carried a TLS vulnerability the desktop builds did not.** The embedded core is compiled from a pinned submodule, and the pin predated the fix — CVE-2026-56862 in the crypto/tls fork mihomo vendors, addressed upstream in v1.19.30. Desktop was never affected: it downloads whatever `releases/latest` offers at build time, so it picked the fix up on its own. That asymmetry is the whole reason the pin is now followed rather than remembered.
- An xray core that fails to start on Android now releases what it managed to bring up. The core was built and partly started before the failure, nothing held a reference to it afterwards, and its inbound ports stayed taken — so the retry that follows a failed start could find its own ports occupied.
- The set of Android ABIs the core ships for is decided in one place and the build enforces it. It had been written down three times and only one of them built anything, so adding an ABI would have shipped 31 MB of core that nothing linked against while the client still reported the relay unsupported — and nothing would have failed to say so.

## v4.0.1

### Fixed

- **The relay's local socks5 inbounds were open to anything on the machine.** Each relayed node is reached through an inbound on `127.0.0.1`, and those were generated without authentication — but loopback is not a boundary between programs. Any process on a desktop, and on Android any installed application, could dial one and reach the internet through a chosen exit node, past every rule mihomo was about to apply and at the subscriber's expense. The inbounds now require a credential that is generated fresh each time the core starts and known only to the client's own configuration. Anyone running 4.0.0 with the relay on should update.

## v4.0.0

### New

- **Xray relay.** Traffic now leaves through a bundled xray core instead of mihomo dialling the nodes itself. mihomo stays the routing frontend — TUN, rules, groups, process rules are unchanged — and each relayable node is replaced by a socks5 stand-in pointing at a local xray inbound. The point is that the TLS fingerprint and the REALITY / Vision / XHTTP implementations always come from xray, so the client does not trip over `minClientVer` on a freshly configured server or announce a home-made ClientHello. **The mode is pinned on from this release** and the switch is locked; that has never meant every node is relayed, nor that a failure leaves you without a connection.
- **Nodes that xray cannot carry keep working.** Eligibility is three things: a protocol xray speaks, a name not on the exclusion list, and an outbound that actually exists. The third decides — a supported protocol still stays native when no outbound could be produced for it. Hysteria2, TUIC, SSH and the rest simply go on being dialled by mihomo, and the settings dialog lists every node with the reason it is where it is.
- **The subscription's own xray config is used where it exists.** Profiles are fetched a second time from the same URL with `celestial/xray/<ver>`, and outbounds that come back are used verbatim rather than converted, so nothing can be lost in translation. A panel that ignores the User-Agent, serves the same bytes twice, or serves no template at all falls back to converting the mihomo side — a subscription is free to be a mixture, and usually is.
- **Per-node control.** Any node can be pinned to the relay or to native by hand, stored with the profile so it survives a subscription refresh. An override outranks the name-exclusion list but cannot make a node relayable that has no outbound.
- **Diagnostic export.** The generated `xray.json` and `config.yaml` can be exported with credentials replaced. The masking parameters themselves are deliberately not hidden — an export that cannot be used to check that the obfuscation survived conversion would defeat the point.

### Improved

- The xray core is validated before either process starts, and the chain comes up from the far end: xray first, a TCP connect on every relayed port, then mihomo. Stopping goes the other way. mihomo pointed at stand-ins nothing is listening on is not a degraded client, it is a client with no working proxies.
- A relay that will not come up never costs the connection. The first failure is treated as the port race the search cannot close on its own and answered by regenerating with fresh ports; a second gives up for the session and rebuilds the configuration natively, with mihomo left running throughout.
- Nodes the panel inlines into a `proxy-providers` payload are relayed too. Groups commonly source their nodes from a provider rather than from `proxies`, and a relay confined to `proxies` would generate, start, and carry nothing while appearing to work.

### Changed

- Windows portable archives now carry the xray core alongside mihomo, and the packagers refuse to build an archive with a core missing rather than shipping one quietly incomplete.
- The relay is desktop-only. On Android the core runs in-process through cgo and there is no second process to spawn, so nothing is planned there and the control is hidden rather than shown inert.

### Fixed

- A subscription refresh no longer drops connections when nothing changed. The port search asks the operating system what is free, and the process holding those ports is the running xray — so every regeneration moved unchanged nodes onto new ports, and the chain was replaced to serve a plan that differed in nothing else. Long enough to be thrown out of an online game by a timer nobody set.
- `flow` is dropped on the transports XTLS cannot apply to. Panels attach `xtls-rprx-vision` to every VLESS node regardless of transport; mihomo ignores it where it is inapplicable and xray executes it, so the connection dies on a server that never enabled flow. `xray -test -config` accepts the combination silently, which is the one place validation is no help.
- XHTTP masking options are converted mechanically rather than against a list of known names, so an option added to xray later is carried rather than turning into a refused node. The session fields are written under both spellings the core has used, since the release and pre-release channels disagree about them and xray ignores the one it does not know.

## v3.0.0

### New

- **Android support.** The mihomo core is embedded and runs in-process through a cgo bridge, TUN is provided by `VpnService` with a real file-descriptor handoff, and CI builds and signs a universal APK for every release. Proxies, rules, connections and logs all work against the embedded core; desktop-only settings (autostart, lite mode, hotkeys) are hidden rather than shown as broken.
- **Run State store.** Service health, running mode, pending privileged action and the privileged-operation lock now have a single owner instead of being re-derived in several places. The frontend receives a snapshot on every transition, so it no longer polls to notice that the core stopped or the service came back.
- **Owner-authenticated service IPC.** The privileged helper now authenticates the owner of each request, keeps one runtime generation per owner, and the client detects and recovers from losing ownership of a running core.
- **Node choices survive a core start.** The node picked in each proxy group is recorded in the profile and put back when a core starts, not only when a profile is switched. Previously this relied entirely on mihomo's own `cache.db`, which does not hold when `store-selected` is absent from a merge template or when the core starts in a directory nothing has run in.
- Starting the core falls back to a free mixed port instead of refusing to start when the configured one is taken.

### Improved

- **A subscription update no longer replaces the core.** In service mode the core refuses to reload a configuration from outside its own directory, so every change fell through to a full restart — which tears down TUN and takes the device's network with it. The service now stages the configuration into the generation the core is already running in, and the core reloads in place. An automatic subscription refresh no longer drops connections.
- Configuration validation reports a structured outcome rather than a bare boolean, so "valid", "invalid", "skipped" and "busy" are no longer indistinguishable to the caller or the frontend.
- Core start, stop and restart are serialized, and on Windows the sidecar's lifetime is tied to the app through a Job Object so a crash cannot leave an orphaned core holding the proxy ports.
- The privileged installer and uninstaller run under tighter constraints, and a panic during app setup degrades startup instead of aborting it.
- Dependency and toolchain updates: Renovate batches, GitHub Actions v7, Go 1.26.5, and the reported npm advisories cleared.

### Changed

- Moved to the `celestialhq` organization. Application data is migrated from the previous identifier on first start.
- Reconciling the profile's recorded node choices against the running core moved from the frontend into the backend; the frontend copy is gone, because two reconcilers writing the same record would race.
- The vendored `tauri-plugin-mihomo` fork was dropped, `kode-bridge` now comes through the `celestialhq` fork, and the bundled privileged service is pinned to the version the client is built against instead of whatever was published last.

### Fixed

- Toggling TUN off and straight back on left the setting saved as enabled while the core ran without it: the interface said the tunnel was up and no traffic went through it. Each configuration file has one draft slot, and two overlapping changes were committing each other's values.
- In service mode the client talked to the sidecar's socket rather than the endpoint the service actually opened, so traffic flowed while logs, connections, rules, proxy switching and delay tests were all silently dead.
- Deleting a profile unlinked its files before writing the index that referenced them, and validated the result only afterwards. Both halves are now ordered so that nothing irreversible happens before the configuration without that profile is known to work.
- The profile editors could destroy the file they had opened, and a chain file that did not exist yet was treated as broken rather than empty.
- The subscription `x-hwid` is now built from characters Remnawave 3.0 accepts. A single character outside its set made the panel ignore the header entirely, so affected devices never registered at all.
- Upgrading from 2.x could leave every subscription behind under the old identifier: the migration treated "the directory contains anything" as "the user already moved across", and a window-geometry file written by an earlier start satisfied that.
- A helper too old to state which protocol it speaks is reported as needing reinstallation instead of failing as a JSON parse error, and installing over an incompatible helper reinstalls it.
- macOS and Linux AppImage builds had no entry in the updater manifest at all, so neither platform could ever update itself.
- A stalled signature download could hold the publish job — and the whole concurrency group — indefinitely; releases are now finalized before the updater manifest is published, and draft release IDs are resolved by listing rather than by tag.
- A pending configuration draft is rebased onto data committed while it was open, instead of silently rolling that data back.

## v2.1.0

### New

- Applied the new Celestial Design across the app: reworked layout, navigation, proxy/rule/connection lists, home cards, and settings, plus new Montserrat/JetBrains Mono fonts.
- Added a bottom navigation bar (`bottom-nav.tsx`) for narrow windows.

### Changed

- Removed the standalone theme viewer settings page; theme controls now live inline in the redesigned settings.

## v2.0.0

### New

- Bumped the application, Tauri configuration, and Cargo package version to `2.0.0`.
- Added the new dev, RC, and stable release/autobuild pipeline with a generated release page, OS download table, asset `.sha256` files, and publishing into `celestialhq/celestial-mihomo-client`.
- Renamed release-candidate tags from `testv*.*.*` to `rc-v*.*.*`.
- Moved Celestial Service IPC to `celestialhq/celestial-service-ipc`; prebuild now supports the service release archives and their new asset layout.

### Improved

- macOS DMG builds remain unsigned, but workflows now pass `TAURI_PRIVATE_KEY` / `TAURI_KEY_PASSWORD` to Tauri updater signing as `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Applied Renovate updates for `react-router` 8, `js-yaml` 5, `commander` 15, `lint-staged` 17, `@eslint-react/eslint-plugin` 5, and `rust_iso3166` 0.2.
- Applied security lockfile updates for `tauri` 2.11.1, `rustls-webpki` 0.103.13, `rkyv` 0.8.16, `tar` 0.4.46, and `dompurify` 3.4.x via `pnpm.overrides`.

### Changed

- Removed the current remote/native notification subsystem completely: `notify.json` polling, system push notifications, the Tauri notification plugin, and native hotkey notifications.
- Kept internal frontend events and in-app toast feedback (`showNotice`) because they are still used for normal UI feedback.
- Adapted ESLint configuration for `@eslint-react/eslint-plugin` 5 without forcing a UI refactor of existing components.

### Fixed

- Fixed the macOS build error `A public key has been found, but no private key` in dev/release workflows.
- Fixed production build compatibility with `js-yaml` 5 by switching imports to namespace imports.
- Verified stability after major dependency updates: TypeScript typecheck, ESLint, Vite production build, `cargo check`, and `cargo clippy` pass.

## v1.4.1

### Новое

- Запросы удалённых подписок теперь отправляют заголовки `x-hwid`, `x-device-os`, `x-ver-os` и `x-device-model`.
- HWID формируется как версионированный SHA-256-отпечаток системных и аппаратных идентификаторов. Исходные идентификаторы не отправляются на сервер.
- Добавлена обработка ответных заголовков `x-hwid-active`, `x-hwid-not-supported` и `x-hwid-max-devices-reached`.

### Улучшено

- Перенесены актуальные исправления upstream для обновления правил, виртуального списка прокси, прокрутки логов и совместимости с macOS 12.
- Восстанавливается нормальный размер окна, если сохранённое состояние содержит слишком маленькие размеры.
- Улучшена устойчивость импорта некорректных `ss://`-ссылок с параметрами `v2ray-plugin`.

### Изменено

- Удалена страница «Управление подпиской» и связанный с ней API.
- Версия приложения, Tauri-конфигурации и Cargo-пакета обновлена до `1.4.1`.

## v1.4.0

### Новое

- Версия приложения, Tauri-конфигурации и Cargo-пакета обновлена до `1.4.0`.
- Добавлен просмотр release notes с поддержкой GitHub alert-блоков и более читаемой версткой.
- Добавлена подсказка-предупреждение для цепочек прокси.

### Улучшено

- Monaco editor теперь загружается лениво, что снижает начальную нагрузку на фронтенд.
- Стабилизирован sticky scroll в группах прокси.
- Улучшена кликабельная область отображения скорости в macOS tray.
- Диалоги backup/restore/delete приведены к in-app modal поведению.
- Улучшена совместимость Linux/Wayland, включая legacy Wayland renderer workaround и показ окна после загрузки страницы.

### Исправлено

- Исправлен разбор hotkey-клавиши `OS`: теперь она корректно мапится в `CMD`.
- Остановлен system proxy guard, когда системный прокси отключен.
- Замаскированы subscription URL в backend logs.
- Улучшена логика exit cleanup, чтобы не выполнять лишнюю очистку в lightweight/window-close сценариях.
- Сохранены fork-specific зависимости `tauri-plugin-mihomo` при переносе upstream fixes.

## v1.3.0

### Новое

- Обновлен основной дизайн приложения: темная небесная база дополнена мягкими фиолетовыми акцентами из страницы прокси.
- Иконка приложения и логотип Celestial заменены на новый облачный знак во всех основных app, web, installer и tray-ассетах.
- Версия приложения, Tauri-конфигурации и Cargo-пакета обновлена до `1.3.0`.
- Mihomo core обновлен до `v1.19.24`, Mihomo Alpha обновлен до `alpha-98aa7e6`.

### Улучшено

- Убран простой режим, приложение теперь всегда открывается в полноценном интерфейсе.
- Индикаторы системного прокси теперь используют реальное состояние ОС и проверяют, что прокси указывает именно на Celestial.
- Переключатель системного прокси использует оптимистичное обновление без визуального отката во время применения настроек.

### Исправлено

- Исправлено самовыключение системного прокси на Windows: PAC очищается до включения manual proxy, чтобы `sysproxy-rs` не сбрасывал только что включенный системный прокси.
- Исправлены состояния системного прокси в карточке `System / TUN`, tray-иконке, tray-меню и tooltip.

## v1.2.0

### Новое

- Добавлен простой режим клиента с тремя вкладками: главная, профили и настройки.
- На главную простого режима вынесены большая кнопка подключения, выбор прокси и переключение режимов `Правила / Глобал / Директ`.
- Добавлен блок `Группы` с карточками прокси и текущей подписки.
- В настройках простого режима оставлены системный прокси, TUN и ручная проверка обновлений.

### Улучшено

- Простой режим получил отдельный компактный небесный дизайн без лишней шапки, статуса и расширенных элементов.
- Навбар в простом режиме принудительно свернут  не разворачивается.
- Таймер подключения теперь обновляется каждую секунду и сбрасывается при повторном подключении.
- Исправлен pre-commit для SCSS: `lint-staged` больше не отправляет `.scss` файлы в Biome.
- Версия приложения, Tauri-конфигурации и Cargo-пакета обновлена до `1.2.0`.

## v1.1.1

### Исправлено

- Исправлена логика проверки Celestial Service: проверка состояния больше не запускает автоматический `uninstall/install`.
- Устранены повторные цепочки переустановки сервиса после установки, перезапуска приложения, завершения задачи или рестарта ПК.
- Установка сервиса теперь выполняет контролируемый reinstall только когда обнаружен устаревший или несовместимый сервис.
- Проверка версии сервиса больше не блокирует установку и не запускает повторную переустановку при рабочем IPC.
- Добавлена защита от повторного запуска установки сервиса быстрыми кликами по карточке режима работы.
- Версия приложения, Tauri-конфигурации и Cargo-пакета обновлена до `1.1.1`.

## v1.1.0

### Новое

- Добавлен центр уведомлений Celestial с загрузкой сообщений из удаленного `notify.json`.
- Добавлен системный push для новых уведомлений со статусом `urgent`.
- Если `notify.json` временно недоступен или пустой, центр уведомлений показывает состояние загрузки и продолжает проверку.

### Формат notify.json

```json
{
  "schemaVersion": 1,
  "generatedAt": "2026-04-19T12:00:00Z",
  "notifications": [
    {
      "id": "v1.1.0-release",
      "status": "info",
      "title": "Celestial v1.1.0",
      "body": "Release notes or important message.",
      "createdAt": "2026-04-19T12:00:00Z",
      "updatedAt": "2026-04-19T12:00:00Z",
      "expiresAt": null,
      "link": "https://example.com",
      "locale": {
        "ru": {
          "title": "Celestial v1.1.0",
          "body": "Описание уведомления."
        },
        "en": {
          "title": "Celestial v1.1.0",
          "body": "Notification body."
        }
      }
    }
  ]
}
```

Поддерживаемые статусы: `info`, `success`, `warning`, `urgent`.

### Исправлено

- Версия приложения, Tauri-конфигурации и Cargo-пакета обновлена до `1.1.0`.
- Windows installer/uninstaller теперь работает только с бинарниками Celestial: `celestial.exe`, `celestial-service.exe`, `celestial-mihomo.exe` и `celestial-mihomo-alpha.exe`.
- Убрана очистка ярлыков, registry-ключей, автозапуска и процессов Clash Verge из установщика Celestial.
- Изменен singleton-порт Celestial, чтобы запущенный или недавно закрытый Clash Verge не мешал запуску Celestial.
## v1.0.0

> **Initial Celestial release.** This is the first public build under the Celestial name, forked from [clash-verge-rev](https://github.com/clash-verge-rev/clash-verge-rev) and adapted for the Celestial client experience.

- **Mihomo (Meta) core upgraded to v1.19.23**

### 🐞 Bug Fixes

- Fixed system proxy not fully closing in PAC mode after disabling
- Fixed potential freeze when toggling proxy on macOS
- Fixed auto-update timer not refreshing immediately after interval change
- Fixed TUN disable not taking effect immediately on Linux

### ✨ New Features

- macOS tray now shows live upload/download speed
- Hotkey actions now display a notification with the result

### 🚀 Improvements

- Improved system proxy read performance on macOS
- Rebranded all UI references from Clash Verge to Celestial
- Cleaned up build pipeline: English-only release notes, UTC timestamps, removed external referral links
