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
