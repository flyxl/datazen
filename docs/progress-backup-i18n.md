# Progress: Backup ZIP + 10-Locale i18n

**Branch:** `feat/backup-zip-i18n`  
**Spec:** `docs/superpowers/specs/2026-08-07-backup-zip-i18n-design.md`  
**Plan:** `docs/superpowers/plans/2026-08-07-backup-zip-i18n.md`

| # | Feature | Status | Unit tests | E2E test agent | Commit |
|---|---------|--------|------------|----------------|--------|
| 1 | App data ZIP export/import (replace config JSON) | 🔄 in progress | pending | pending | — |
| 2 | 10 locales (en, zh-CN, zh-TW, es, fr, de, ja, pt-BR, ru, ko) | ⏳ pending | pending | pending | — |
| 3 | First-run language follows system (else `en`) | ✅ done | ✅ Rust + Vitest | pending | `feat/sys-locale` |

## Feature 1 notes

_(fill after implementation)_

## Feature 2 notes

_(fill after implementation)_

## Feature 3 notes

- Rust `resolve_ui_language` in `src-tauri/src/i18n_locale.rs` maps OS locale → supported UI code; unknown → `en`.
- Store uses `AppSettings::default_for_first_run()` when `settings.json` is absent; existing settings are never overwritten.
- Frontend default language is `en`; `main.tsx` preloads settings via Tauri before first paint.
- TS mirror `src/lib/resolveUiLanguage.ts` for dev fallback when backend unavailable.

## E2E / bug log

_(test agents append here)_
