# Progress: Backup ZIP + 10-Locale i18n

**Branch:** `feat/backup-zip-i18n`  
**Spec:** `docs/superpowers/specs/2026-08-07-backup-zip-i18n-design.md`  
**Plan:** `docs/superpowers/plans/2026-08-07-backup-zip-i18n.md`

| # | Feature | Status | Unit tests | E2E test agent | Commit |
|---|---------|--------|------------|----------------|--------|
| 1 | App data ZIP export/import (replace config JSON) | 🔄 in progress | pending | pending | — |
| 2 | 10 locales (en, zh-CN, zh-TW, es, fr, de, ja, pt-BR, ru, ko) | ✅ done | ✅ pass | pending | — |
| 3 | Delete GH v0.0.7 + trigger rebuild | ⏳ pending | n/a | n/a | — |

## Feature 1 notes

_(fill after implementation)_

## Feature 2 notes

- Extended `SupportedLocale` + `src/locales/index.ts` with 10 locales and `SUPPORTED_LOCALES` constant.
- Added locale files: `zh-TW`, `es`, `fr`, `de`, `ja`, `pt-BR`, `ru`, `ko` (non-zh copied from `en`; `zh-TW` adapted from `zh-CN`).
- Settings language dropdown lists all 10 locales.
- `scripts/generate-menu-labels.mjs` generates menu labels for all 10 (en fallback for missing keys).
- Vitest: `src/locales/locales.test.ts` — key parity, load, fallback.

## Feature 3 notes

_(fill after release ops)_

## E2E / bug log

_(test agents append here)_
