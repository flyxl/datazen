# Progress: Backup ZIP + 10-Locale i18n

**Branch:** `feat/backup-zip-i18n`  
**Spec:** `docs/superpowers/specs/2026-08-07-backup-zip-i18n-design.md`  
**Plan:** `docs/superpowers/plans/2026-08-07-backup-zip-i18n.md`

| # | Feature | Status | Unit tests | E2E test agent | Commit |
|---|---------|--------|------------|----------------|--------|
| 1 | App data ZIP export/import (replace config JSON) | ✅ merged | 4/4 pass | pending | `feat/zip-backup` |
| 2 | 10 locales (en, zh-CN, zh-TW, es, fr, de, ja, pt-BR, ru, ko) | ✅ merged | ✅ pass | pending | `feat/i18n-10` |
| 3 | First-run language follows system (else `en`) | ✅ merged | ✅ Rust + Vitest | pending | `feat/sys-locale` |
| 4 | Trigger GitHub release package | ⏳ pending | n/a | n/a | — |

## Feature 1 notes

- Added `zip` crate and `src-tauri/src/app_data_archive.rs` (exclude `logs/` + temp patterns, path-traversal validation, preserve logs on import).
- Tauri commands: `export_app_data`, `import_app_data`, `restart_app` in `commands/config.rs`.
- Frontend: `src/commands/backup.ts`; `MainWindow` ZIP save/open dialogs with overwrite `ask()` + restart on import.
- Removed `ImportConfigDialog.tsx` and password-based JSON export/import UI flow.
- i18n: updated `en.ts` / `zh-CN.ts` menu/action/appData keys; regenerated `menu-labels.json`.
- Unit tests: `cargo test -p datazen app_data_archive` — 4 passed.

## Feature 2 notes

- Extended `SupportedLocale` + `src/locales/index.ts` with 10 locales and `SUPPORTED_LOCALES` constant.
- Added locale files: `zh-TW`, `es`, `fr`, `de`, `ja`, `pt-BR`, `ru`, `ko` (non-zh copied from `en`; `zh-TW` adapted from `zh-CN`).
- Settings language dropdown lists all 10 locales.
- `scripts/generate-menu-labels.mjs` generates menu labels for all 10 (en fallback for missing keys).
- Vitest: `src/locales/locales.test.ts` — key parity, load, fallback.

## Feature 3 notes

- Rust `resolve_ui_language` in `src-tauri/src/i18n_locale.rs` maps OS locale → supported UI code; unknown → `en`.
- Store uses `AppSettings::default_for_first_run()` when `settings.json` is absent; existing settings are never overwritten.
- Frontend default language is `en`; `main.tsx` preloads settings via Tauri before first paint.
- TS mirror `src/lib/resolveUiLanguage.ts` for dev fallback when backend unavailable.

## E2E / bug log

_(test agents append here)_
