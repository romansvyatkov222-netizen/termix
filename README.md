# Termix

SSH-терминал и SFTP в одном окне. Десктопное приложение для Windows 10/11 x64:
удалённый файловый менеджер, SSH-терминал (xterm.js), очередь передач,
просмотр/правка файлов во внешнем редакторе, мониторинг VDS (CPU/RAM/диски).

Стек: **Tauri 2 + Rust (russh) + Svelte 5 + TypeScript + Vite 5**.

## Требования для сборки

- **Windows 10/11 x64**
- **Node.js 20+** (проверено на 24.x), npm
- **Rust stable** (через [rustup](https://rustup.rs)), target `x86_64-pc-windows-msvc`
- Build Tools: MSVC (Visual Studio Build Tools, workload «Desktop development
  with C++»), **WebView2** (ставится с Windows 10/11 либо с установщиком),
  **NSIS 3+** (только для сборки установщика, в PATH: `makensis`)

## Сборка с нуля

```powershell
npm ci            # зависимости фронта (строго по package-lock.json)
npm run check     # svelte-check: 0 errors
npx tsc --noEmit  # TypeScript без эмита
npm run build     # фронт -> dist/
```

```powershell
cd src-tauri
cargo fmt --check # форматирование Rust
cargo clippy --all-targets  # линтер (предупреждение про russh — внешнее)
cargo test sysinfo          # юнит-тесты парсинга статистики VDS
cargo build                 # debug exe (иконка вшивается из icons/termix.ico)
```

## Установщик (только NSIS)

```powershell
npm run tauri build   # release + dist/ вшивается в exe + Termix_*_x64-setup.exe
```

Готовый установщик: `src-tauri/target/release/bundle/nsis/Termix_*_x64-setup.exe`.
Устанавливается и работает **без исходников**: фронт (`dist/`, собранный
`npm run build` через `beforeBuildCommand`) вшивается в exe как ресурсы
Tauri, бэкенд — тот же exe. Отдельный exe без установщика не публикуем —
артефакт релиза только setup.exe.

CI (`.github/workflows/release.yml`) собирает установщик на
`windows-latest` и прикладывает его к GitHub Release. Ручной запуск:
Actions → «Release installer» → Run workflow (или пуш тега `v*`).

## Структура

- `src/` — фронт (Svelte): `components/` (FilesPanel, TerminalPanel,
  StatsPanel, Workspace, …), `lib/` (api, stores, i18n, format)
- `src-tauri/src/` — бэкенд (Rust): `ssh`/`sftp`/`terminal`/`transfers`/
  `edit`/`sysinfo`, `models`, `errors`, `state`, `storage` (DPAPI)
- `src-tauri/icons/termix.ico` — иконка exe/установщика (7 размеров 16–256)
- `public/termix-brand.svg` — лого в шапке (только значок) и favicon
- `docs/` — `path-autocomplete.md`, `search.md` (поведение файлового менеджера)

## Локализация

RU/EN, паритет ключей обязателен: `src/lib/lang/ru.ts` + `en.ts`.
