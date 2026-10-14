# ChatGPT Usage Tray

Небольшое приложение для системного трея, которое показывает остаток лимитов подписки **Codex**: 5-часовое и недельное окна, время сброса и состояние обновления.

Основное окружение — **Arch Linux, KDE Plasma 6, Wayland**. Приложение написано на Rust и Tauri 2; интерфейс — Vanilla TypeScript и CSS, без React, Vue и тяжёлых UI-библиотек.

> Неофициальный проект, не связанный с OpenAI. Используется недокументированный endpoint статистики, который может измениться. Эти лимиты относятся к Codex и не обязательно отражают все возможности ChatGPT.

## Возможности

- Динамическая иконка трея с оставшимся процентом 5-часового лимита.
- Нативное меню: оба лимита, время сброса, статус, открытие статистики, обновление и выход.
- Тёмное окно статистики с крупными процентами, цветными карточками и плавными прогресс-барами.
- Цветовые состояния: более 50% — зелёный/фиолетовый, 21–50% — янтарный, 0–20% — красный; отсутствие данных — серый. Состояния дополнены текстом.
- Автоматическое обновление каждые 5 минут после успешного запроса и ручное обновление.
- Последовательные запросы с таймаутом и паузами при ошибках сети, HTTP 401/403/429/5xx.
- Сохранение последних успешных данных при ошибке; отображение даты и возраста статистики.
- Версионированный кеш между запусками с атомарной записью и проверкой формата.
- Запуск со скрытым окном; закрытие окна скрывает его, а не завершает приложение.
- Защита от повторного запуска на Linux.
- Запуск через `systemd --user`, без работающего Vite-сервера и терминала.

## Авторизация и безопасность

Нужна существующая авторизация Codex CLI. Приложение читает access token из:

- `CODEX_HOME/auth.json`, если задан `CODEX_HOME`;
- иначе — `~/.codex/auth.json`.

Приложение **не изменяет файл авторизации**, не выполняет logout и не обновляет OAuth-токены автоматически. Токен используется только Rust backend для запроса `https://chatgpt.com/backend-api/wham/usage`, не передаётся во frontend и не сохраняется в кеше.

При HTTP 401 проверьте авторизацию через обычный Codex CLI. При 401/403 автоматическая пауза составляет 15 минут. При 429 учитывается `Retry-After`; временные ошибки используют задержки 30 → 60 → 120 → 240 → 480 → 900 секунд. Ручное обновление не обходит паузу после ошибки.

Кеш `usage-cache.json` находится в пользовательском каталоге данных Tauri — обычно `~/.local/share/dev.pastop.chatgpt-tray/` на Linux. Он содержит только окна лимитов, время получения и версию формата. Данные из кеша явно отмечены до первого успешного обновления.

## Требования

- Rust/Cargo. Проверенная версия Rust — **1.97.1**. Код блокировки требует минимум 1.89, совместимость всего дерева зависимостей с этой минимальной версией отдельно не проверялась.
- Node.js и npm. Версия проекта указана в [`.node-version`](.node-version): **24.19.0**.
- Linux-зависимости сборки Tauri 2: WebKitGTK 4.1, GTK 3, AppIndicator, инструменты C/C++ и `pkg-config`.
- Codex CLI с существующей авторизацией.

Для Arch Linux зависимости сборки можно установить так:

```sh
sudo pacman -S --needed base-devel pkgconf webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg
```

Для других дистрибутивов см. [системные требования Tauri](https://v2.tauri.app/start/prerequisites/).

## Разработка

Из корня репозитория:

```sh
cd desktop
npm ci
npm run tauri dev
```

Окно при запуске скрыто — выберите **«Открыть статистику»** в трее. Если приложение уже запущено через службу, сначала остановите её: защита от повторного запуска не позволит открыть второй экземпляр.

```sh
systemctl --user stop chatgpt-tray.service
```

Для консольной диагностики из корня репозитория:

```sh
cargo run
```

И desktop-приложение, и CLI выполняют реальный запрос статистики с существующей авторизацией.

## Release-сборка

Из каталога `desktop`:

```sh
npm run tauri -- build --no-bundle
```

Если все Cargo-зависимости уже доступны локально, можно добавить `-- --offline`.

Бинарник при стандартной конфигурации Cargo:

```text
desktop/src-tauri/target/release/chatgpt-tray
```

Запуск из корня репозитория:

```sh
./desktop/src-tauri/target/release/chatgpt-tray
```

Frontend встроен в release-бинарник: Vite-сервер и Node.js для его работы не нужны. Системные runtime-библиотеки WebKitGTK/GTK должны оставаться установленными. `--no-bundle` собирает приложение без пакета установки.

## Автозапуск через systemd

Ниже пример для расположения репозитория `/opt/usr/chatgpt_tray`. **Замените `ExecStart` на свой абсолютный путь**, если проект находится в другом каталоге.

Создайте `~/.config/systemd/user/chatgpt-tray.service`:

```ini
[Unit]
Description=ChatGPT Codex Usage Tray
PartOf=graphical-session.target
After=graphical-session.target
StartLimitIntervalSec=120
StartLimitBurst=3

[Service]
Type=simple
ExecStart=/opt/usr/chatgpt_tray/desktop/src-tauri/target/release/chatgpt-tray
Restart=on-failure
RestartSec=10
TimeoutStopSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=graphical-session.target
```

Перед включением завершите экземпляр, запущенный вручную, через **«Выход»** в трее. Затем:

```sh
systemctl --user daemon-reload
systemctl --user enable --now chatgpt-tray.service
systemctl --user status chatgpt-tray.service
journalctl --user -u chatgpt-tray.service -f
```

`Restart=on-failure` перезапускает приложение после аварии, но не после штатного «Выход». `PartOf` останавливает службу при завершении графической сессии. Не включайте службу как системную/root и не используйте `sudo systemctl` для неё.

KDE должен передавать `WAYLAND_DISPLAY`, `DISPLAY`, `XDG_RUNTIME_DIR` и `DBUS_SESSION_BUS_ADDRESS` пользовательскому systemd. Проверка:

```sh
systemctl --user is-active graphical-session.target
systemctl --user show-environment
```

Не прописывайте фиксированные номера дисплеев или токены в unit. Если используется нестандартный `CODEX_HOME`, передайте службе только путь к нему. Удаление `target` или перенос репозитория потребует обновить путь запуска службы.

## Иконка окна в KDE / Wayland

Включён GTK app ID. Однако в проверенном окружении KWin определяет окно как `chatgpt-tray`. Поэтому предусмотрены два desktop-файла:

- [`desktop/dev.pastop.chatgpt-tray.desktop`](desktop/dev.pastop.chatgpt-tray.desktop) — основной ярлык;
- [`desktop/chatgpt-tray.desktop`](desktop/chatgpt-tray.desktop) — совместимость с фактическим идентификатором KWin; скрыт из меню через `NoDisplay=true`.

Перед установкой поправьте `Exec` в обоих файлах, если проект расположен не в `/opt/usr/chatgpt_tray`. Команды ниже выполняются из корня репозитория; не перезаписывайте собственные существующие ярлыки без проверки:

```sh
mkdir -p ~/.local/share/applications
mkdir -p ~/.local/share/icons/hicolor/256x256/apps
mkdir -p ~/.local/share/icons/hicolor/scalable/apps
cp -i desktop/dev.pastop.chatgpt-tray.desktop ~/.local/share/applications/
cp -i desktop/chatgpt-tray.desktop ~/.local/share/applications/
cp -i desktop/src-tauri/icons/128x128@2x.png ~/.local/share/icons/hicolor/256x256/apps/dev.pastop.chatgpt-tray.png
cp -i desktop/src-tauri/icons/app-icon.svg ~/.local/share/icons/hicolor/scalable/apps/dev.pastop.chatgpt-tray.svg
kbuildsycoca6 --noincremental
systemctl --user restart chatgpt-tray.service
```

Для ручного запуска вместо последней команды завершите и заново запустите приложение. Динамическая иконка трея с процентом независима от иконки окна.

Если значок заголовка неверный, откройте окно статистики, выполните команду и выберите его курсором:

```sh
qdbus6 org.kde.KWin /KWin org.kde.KWin.queryWindowInfo
```

Поля `desktopFile` и `resourceClass` помогут проверить сопоставление с установленным ярлыком.

## Проверки

Из корня репозитория:

```sh
cargo test --offline
cargo check --offline
```

Из `desktop/src-tauri`:

```sh
cargo test --offline --lib
cargo check --offline
cargo clippy --offline --no-deps --all-targets
```

Из `desktop`:

```sh
node --test src/limit-state.test.mjs src/time-format.test.mjs
npx tsc --noEmit
npm run build
```

`--offline` требует заранее загруженных зависимостей. Тесты не читают OAuth-файл и не обращаются к Codex API; HTTP-сценарии проверяются локальным mock-сервером. Покрыты цветовые границы, время сброса и возраст данных, HTTP-ошибки, retry, формат кеша и блокировка экземпляра.

Release-сборка и запуск службы проверены; исправление иконки KDE подтверждено вручную. Вход после перезагрузки и работа в других окружениях требуют отдельной проверки. В Clippy остаются два предупреждения `implicit_saturating_sub` в `tray_icon.rs`.

## Структура

```text
src/                         Общие Rust API, авторизация, модели и CLI
desktop/src/                 Vanilla TypeScript, CSS и frontend-тесты
desktop/src-tauri/src/
  lib.rs                     Настройка и запуск Tauri
  state.rs                   Состояние и payload событий
  commands.rs                get_status / request_refresh
  tray.rs                    Нативное меню и отображение трея
  tray_icon.rs               Генерация иконки с процентом
  worker.rs                  Запросы, retry и обновление статистики
  cache.rs                   Безопасный кеш
  instance.rs                Защита от повторного запуска на Linux
.docs/                       Задание и заметки о release
```

## Перед публикацией

Не добавляйте в Git `auth.json`, OAuth-токены, ответы с личными данными или локальные логи. Проверьте диагностические скрипты, в частности `curl_req.sh`, перед публикацией. `target`, `node_modules` и `dist` должны быть исключены; lock-файлы Cargo и npm стоит сохранить для воспроизводимой сборки.

## Разработка с помощью AI

**Проект доработан с помощью AI-ассистента OpenAI (ChatGPT), работающего в редакторе Zed.** С его помощью выполнены визуальное обновление, обработка ошибок и повторы запросов, безопасное кеширование, рефакторинг Tauri-модулей, защита от повторного запуска, подготовка release/автозапуска, исправление интеграции иконок KDE/Wayland и эта документация.

Постановка задач, решения о принятии изменений и ручная проверка приложения выполнялись владельцем проекта. Упоминание AI-помощи не означает поддержку или официальное одобрение проекта со стороны OpenAI.
