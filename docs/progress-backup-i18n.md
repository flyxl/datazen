# Progress: Backup ZIP + 10-Locale i18n

**Branch:** `feat/backup-zip-i18n`  
**Spec:** `docs/superpowers/specs/2026-08-07-backup-zip-i18n-design.md`  
**Plan:** `docs/superpowers/plans/2026-08-07-backup-zip-i18n.md`

| # | Feature | Status | Unit tests | E2E test agent | Commit |
|---|---------|--------|------------|----------------|--------|
| 1 | App data ZIP export/import (replace config JSON) | ✅ done | 4/4 pass | pending | feat: replace config JSON backup with full app-data ZIP |
| 2 | 10 locales (en, zh-CN, zh-TW, es, fr, de, ja, pt-BR, ru, ko) | ⏳ pending | pending | pending | — |
| 3 | Delete GH v0.0.7 + trigger rebuild | ⏳ pending | n/a | n/a | — |

## Feature 1 notes

- Added `zip` crate and `src-tauri/src/app_data_archive.rs` (exclude `logs/` + temp patterns, path-traversal validation, preserve logs on import).
- Tauri commands: `export_app_data`, `import_app_data`, `restart_app` in `commands/config.rs`.
- Frontend: `src/commands/backup.ts`; `MainWindow` ZIP save/open dialogs with overwrite `ask()` + restart on import.
- Removed `ImportConfigDialog.tsx` and password-based JSON export/import UI flow.
- i18n: updated `en.ts` / `zh-CN.ts` menu/action/appData keys; regenerated `menu-labels.json`.
- Unit tests: `cargo test -p datazen app_data_archive` — 4 passed.

## Feature 2 notes

_(fill after implementation)_

## Feature 3 notes

_(fill after release ops)_

## E2E / bug log

_(test agents append here)_
