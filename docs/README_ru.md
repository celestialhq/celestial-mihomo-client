<h1 align="center">
  ☁️
  <br />
  Celestial
</h1>

<h3 align="center">
  Настольный клиент Mihomo / Clash Meta на Tauri, React и Rust.
</h3>

<p align="center">
  <a href="../README.md">English</a> · <strong>Русский</strong>
</p>

> Celestial - это форк [clash-verge-rev](https://github.com/clash-verge-rev/clash-verge-rev). Проект сохраняет основу Clash Verge Rev и адаптирует ее под клиент Celestial.

## Что это

Celestial - кроссплатформенный GUI для [Mihomo](https://github.com/MetaCubeX/mihomo), также известного как Clash Meta. Приложение управляет удаленными и локальными профилями, группами прокси, правилами, соединениями, логами, системным прокси, TUN-режимом, резервными копиями и интеграцией с рабочим столом.

Кодовая база идет от Clash Verge Rev, но название продукта, заголовок окна и дополнительные функции управления подпиской оформлены под Celestial.

## Возможности

- Встроенное ядро Mihomo с поддержкой стабильного и alpha sidecar-бинарников.
- Управление профилями: удаленные подписки и локальные YAML-файлы.
- Расширение профилей через merge-конфигурации и пользовательские скрипты.
- Страницы прокси-групп, узлов, правил, соединений и логов.
- Системный прокси, proxy guard, редактирование PAC, TUN-режим, DNS, порты и настройки controller.
- Визуальные редакторы прокси, групп, правил, туннелей и Web UI.
- Настраиваемый интерфейс: светлая, темная и системная тема, цвета, шрифт, CSS injection, сворачиваемая навигация и ручная сортировка меню.
- Главная панель с карточками профиля, текущего прокси, сетевого режима, трафика, IP, Mihomo и системной информации.
- Интеграция с треем, горячие клавиши, облегченный режим, проверка обновлений и платформенные настройки окна.
- Резервное копирование, восстановление, история бэкапов, авто-бэкап и синхронизация через WebDAV.
- Управление подпиской Celestial для подходящих remote-профилей `socelestial.com`: трафик, статус, срок действия и привязанные устройства.

## Платформы

Upstream-проект поддерживает Windows, macOS и Linux. Этот форк использует тот же стек Tauri 2 и хранит платформенные ресурсы сборки в `src-tauri`.

## Разработка

Сначала установите обычные [зависимости Tauri](https://tauri.app/start/prerequisites/) для вашей ОС. Затем выполните:

```shell
pnpm i
pnpm run prebuild
pnpm dev
```

Полезные команды:

```shell
pnpm typecheck
pnpm lint
pnpm run web:build
pnpm build
```

## Структура проекта

- `src/` - React-приложение, страницы, компоненты, сервисы, хуки, локали и ассеты.
- `src-tauri/` - Tauri-приложение, Rust backend, платформенные конфиги, инсталляторы, sidecar-ресурсы и capabilities.
- `crates/` - общие Rust-крейты, используемые desktop-приложением.
- `scripts/` - скрипты сборки, релиза, updater, portable-версий и i18n.
- `docs/` - переводы README, превью и историческая документация.

## Upstream

Celestial наследует большую часть функциональности от Clash Verge Rev и связанных проектов:

- [clash-verge-rev/clash-verge-rev](https://github.com/clash-verge-rev/clash-verge-rev) - upstream, от которого сделан этот форк.
- [zzzgydi/clash-verge](https://github.com/zzzgydi/clash-verge) - оригинальный desktop GUI Clash Verge.
- [MetaCubeX/mihomo](https://github.com/MetaCubeX/mihomo) - rule-based proxy core.
- [tauri-apps/tauri](https://github.com/tauri-apps/tauri) - фреймворк для desktop-приложений.
- [vitejs/vite](https://github.com/vitejs/vite) - frontend tooling.

## Лицензия

Проект распространяется по лицензии GPL-3.0-only. Подробнее см. [LICENSE](../LICENSE).
