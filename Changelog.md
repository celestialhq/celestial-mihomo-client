## v1.1.1

### Исправлено

- Исправлена логика проверки Celestial Service: проверка состояния больше не запускает автоматический `uninstall/install`.
- Устранены повторные цепочки переустановки сервиса после установки, перезапуска приложения, завершения задачи или рестарта ПК.
- Установка сервиса теперь выполняет контролируемый reinstall только когда обнаружен устаревший или несовместимый сервис.
- Сервис с живым IPC, но несовпадающей версией больше не считается доступным.
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
