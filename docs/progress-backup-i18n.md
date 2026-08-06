# Progress: Backup ZIP + 10-Locale i18n

**Branch:** `feat/backup-zip-i18n`  
**Spec:** `docs/superpowers/specs/2026-08-07-backup-zip-i18n-design.md`  
**Plan:** `docs/superpowers/plans/2026-08-07-backup-zip-i18n.md`

| # | Feature | Status | Unit tests | E2E test agent | Commit |
|---|---------|--------|------------|----------------|--------|
| 1 | App data ZIP export/import (replace config JSON) | ✅ merged | 4/4 pass | ✅ pass (unit + static) | `feat/zip-backup` |
| 2 | 10 locales (en, zh-CN, zh-TW, es, fr, de, ja, pt-BR, ru, ko) | ✅ merged | ✅ pass | ✅ pass | `feat/i18n-10` |
| 3 | First-run language follows system (else `en`) | ✅ merged | ✅ Rust + Vitest | ✅ pass | `feat/sys-locale` |
| 4 | Trigger GitHub release package | ✅ triggered | n/a | n/a | [run 31126595004](https://github.com/flyxl/datazen/actions/runs/31126595004) `v0.0.8` |

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

### Feature 1 — App data ZIP export/import

**Agent:** test-only (2026-08-07)  
**Branch:** `feat/backup-zip-i18n`  
**Spec:** `docs/superpowers/specs/2026-08-07-backup-zip-i18n-design.md`

#### Test cases (designed)

| ID | Case | Steps | Expected |
|----|------|-------|----------|
| F1-E2E-001 | Export ZIP excludes logs | 1. Seed app data dir with `connections.json`, `settings.json`, `logs/app.log`, `scratch.tmp`, `.import_staging/partial`. 2. Export via menu **File → Export App Data** (or `menu:export-config`). 3. Save as `datazen-backup-YYYYMMDD.zip`. 4. List ZIP entries. | ZIP contains `connections.json`, `settings.json`; **no** `logs/*`, `*.tmp`, `.import_staging/*`. Default save name matches `datazen-backup-YYYYMMDD.zip`. |
| F1-E2E-002 | Export success feedback | Complete F1-E2E-001 export. | Success message shown (`appData.exportSuccess`); no password dialog. |
| F1-E2E-003 | Import overwrite warning | 1. **File → Import App Data**. 2. Pick valid backup ZIP. | Native `ask()` dialog with title `appData.importConfirmTitle`, message `appData.importConfirmMessage`, kind `warning`. |
| F1-E2E-004 | Import cancel skips apply | On F1-E2E-003 dialog, click **Cancel**. | No `import_app_data` invoke; app data unchanged; no restart. |
| F1-E2E-005 | Import confirm applies + restarts | 1. Note current app data. 2. Import valid ZIP; confirm overwrite. | Data dir replaced from ZIP (minus excluded paths); existing `logs/` preserved; app restarts via `restart_app`. |
| F1-E2E-006 | Path traversal rejected | Import ZIP containing entry `../outside.txt` (or `foo/../../bar`). | Import fails with traversal/invalid-input error; target data dir unchanged. |
| F1-E2E-007 | Preserve logs on import | 1. Target data dir has `logs/existing.log`. 2. Import ZIP that includes `connections.json` but no logs (export excludes logs). | After import, `logs/existing.log` content unchanged; zip-sourced `logs/app.log` not written. |
| F1-E2E-008 | Old JSON flow removed | Open main window; trigger import/export. | No `ImportConfigDialog`, no password/Argon2 prompts. Menu labels: EN “Export/Import App Data”, zh-CN “导出/导入应用数据”. |
| F1-E2E-009 | Menu wiring | Click native menu export/import items. | Emits `menu:export-config` / `menu:import-config`; `MainWindow` handlers open save/open dialogs and invoke backup commands. |

#### Execution results

| ID | Result | Method | Evidence |
|----|--------|--------|----------|
| F1-E2E-001 | **PASS** | Unit | `cargo test -p datazen app_data_archive::tests::round_trip_excludes_logs_and_preserves_existing_logs` — ZIP listing asserts no `logs/`, `.tmp`, `.import_staging`. |
| F1-E2E-002 | **PASS** (partial) | Static | `MainWindow.tsx` `handleExportConfig`: `save()` → `exportAppData` → `setErrorMessage(t('appData.exportSuccess'))`. Default path `datazen-backup-${date}.zip`. GUI dialog not exercised. |
| F1-E2E-003 | **PASS** | Static | `MainWindow.tsx` L413–416: `ask(t('appData.importConfirmMessage'), { title: …, kind: 'warning' })` before import. i18n keys present in `en.ts` / `zh-CN.ts`. |
| F1-E2E-004 | **PASS** | Static | `if (!confirmed) return;` before `importAppData` / `restartApp`. |
| F1-E2E-005 | **PASS** (partial) | Unit + static | Round-trip unit test covers data swap + log preserve. UI chain: `importAppData` then `restartApp()` (`backup.ts` → `restart_app` → `app.restart()` in `config.rs`). Full GUI restart not run. |
| F1-E2E-006 | **PASS** | Unit | `rejects_path_traversal_in_zip_entries` + `rejects_malicious_zip_on_import` (4/4 tests green). |
| F1-E2E-007 | **PASS** | Unit | Same round-trip test: `logs/existing.log` == `"keep me"`, no `logs/app.log`. |
| F1-E2E-008 | **PASS** | Static | `ImportConfigDialog` absent from `src/`; menu keys relabeled in locales. |
| F1-E2E-009 | **PASS** | Static | `lib.rs` menu ids `export-config`/`import-config` emit events; `MainWindow` listens and calls handlers; `lib.rs` registers `export_app_data`, `import_app_data`, `restart_app`. |

**Commands run**

```bash
cargo test -p datazen app_data_archive   # 4 passed, 0 failed
```

**Summary:** 9/9 PASS (4 fully via unit tests; 5 via static code verification — native file dialogs and app restart not automated in this run).

**Bugs:** None recorded.

### Feature 2 — 10 UI locales

**Agent:** test-only (2026-08-07)  
**Branch:** `feat/backup-zip-i18n`  
**Spec:** `e2e/specs/i18n-10-locales.ts`

#### Test cases (designed)

| ID | Case | Steps | Expected |
|----|------|-------|----------|
| I18N10-001 | Settings shows 10 languages | Open `?window=settings`, open language Select dropdown. | 10 options: 简体中文, 繁體中文, English, Español, Français, Deutsch, 日本語, Português (Brasil), Русский, 한국어. |
| I18N10-002 | `LANGUAGE_OPTIONS` ↔ `SUPPORTED_LOCALES` | Static compare `SettingsWindow.tsx` vs `src/locales/index.ts`. | Same 10 locale codes (order may differ). |
| I18N10-003 | Switching language updates UI | `save_settings({ language })` + refresh on main window for `en`, `zh-CN`, `zh-TW`, `ja`, `de`. | `main.searchPlaceholder` reflects locale (`Find` vs `查找`). |
| I18N10-004 | All locales key parity | Vitest `locales.test.ts`; E2E smoke-render each locale on main window. | Every locale dict matches zh-CN keys; UI renders without error. |
| I18N10-005 | Unsupported falls back to `en` | `save_settings({ language: 'xx-XX' })`, refresh main window. | UI shows English placeholder; persisted value stays `xx-XX`. |
| I18N10-006 | Menu rebuild all locales | `rebuild_menu({ language })` for each of 10 codes. | No error. |

#### Execution results

| ID | Result | Method | Evidence |
|----|--------|--------|----------|
| I18N10-001 | **PASS** | E2E | WDIO spec — dropdown lists 10 labels |
| I18N10-002 | **PASS** | Static | Both arrays length 10; sets equal: `de, en, es, fr, ja, ko, pt-BR, ru, zh-CN, zh-TW` |
| I18N10-003 | **PASS** | E2E | Placeholder updates for 5 sampled locales |
| I18N10-004 | **PASS** | Vitest + E2E smoke | `npx vitest run src/locales/locales.test.ts` 5/5; E2E renders all 10 |
| I18N10-005 | **PASS** | E2E | `xx-XX` → English UI, stored language unchanged |
| I18N10-006 | **PASS** | E2E | `rebuild_menu` OK for all 10 |

**Commands run**

```bash
npx vitest run src/locales/locales.test.ts                          # 5 passed, 0 failed
pnpm tauri build --debug --features webdriver                       # required for E2E WebDriver port 4445
pnpm e2e:skip-build -- --spec e2e/specs/i18n-10-locales.ts          # 6 passed, 0 failed
```

**Summary:** 6/6 PASS. `LANGUAGE_OPTIONS` and `SUPPORTED_LOCALES` aligned (10 codes, display order differs).

**Bugs:** None recorded.

**E2E harness notes (not product bugs):** Initial spec draft needed navigation fix after settings window (`openMainWindow` wait on `[data-conn-item]`) and WDIO-compatible `expect` syntax; dynamic `import()` in `browser.execute` is not WebDriver-serializable — key parity delegated to Vitest.

### Feature 3 — First-run language follows system (else `en`)

**Agent:** test-only (2026-08-07)  
**Branch:** `feat/backup-zip-i18n`  
**Host OS locale:** macOS `AppleLocale=zh_CN`

#### Test cases (designed)

| ID | Case | Steps | Expected |
|----|------|-------|----------|
| F3-E2E-001 | No `settings.json` → OS-mapped language | 1. Empty temp data dir (no `settings.json`). 2. `Store::init_with_path`. 3. `get_settings().language`. | Language equals `default_ui_language()` (`zh-CN` on host with `zh_CN`). |
| F3-E2E-002 | Unsupported OS locale → `en` | Call `resolve_ui_language` with `it-IT`, `nl-NL`, `xx`. | All map to `en`. |
| F3-E2E-003 | Existing `settings.json` not overwritten | 1. Write `settings.json` with `"language":"fr"`. 2. Init store. 3. Read settings. | Language remains `fr`. |
| F3-E2E-004 | Store `default_for_first_run` wiring | Code review `AppSettings::default_for_first_run` + `load_all`. | Uses `default_ui_language()` only when file missing. |
| F3-E2E-005 | Frontend preload before paint | Code review `main.tsx` + `settingsStore`. | Tauri path loads backend settings before `ReactDOM.createRoot`; default `en` in store; dev catch uses `resolveUiLanguage(navigator.language)`. |

#### Execution results

| ID | Result | Method | Evidence |
|----|--------|--------|----------|
| F3-E2E-001 | **PASS** | Store harness (temp, not committed) | `get_settings().language == default_ui_language()` → `zh-CN` |
| F3-E2E-002 | **PASS** | Rust unit + store harness | `resolve_ui_language("it-IT"|"nl-NL"|"xx")` → `en`; `default_for_first_run().language` ∈ supported set |
| F3-E2E-003 | **PASS** | Store harness | Pre-seeded `language:"fr"` preserved after init |
| F3-E2E-004 | **PASS** | Static | `store/mod.rs` L56–59, L297–299 |
| F3-E2E-005 | **PASS** | Static | `main.tsx` L20–28; `settingsStore.ts` L7–9, L94–104 |

**Commands run**

```bash
cargo test -p datazen i18n_locale                                    # 8 passed, 0 failed
npx vitest run src/lib/__tests__/resolveUiLanguage.test.ts           # 6 passed, 0 failed
cargo test --lib feature3_e2e  # ephemeral harness in store/mod.rs     # 3 passed, 0 failed (reverted)
```

**Summary:** 5/5 PASS (3 via ephemeral store harness; 2 via unit/static). No GUI first-launch run (would need isolated app data dir).

**Bugs:** None recorded.

**Optional observations (not filed):**

- OBS-001: `get_system_ui_language` IPC registered but unused on frontend; first-run uses `get_settings` (no functional impact).
- OBS-002: `settingsStore` `DEFAULT_SETTINGS.limitSelectResults: true` vs Rust `false` — pre-existing; dev-only fallback path.
- OBS-003: Dev catch uses `navigator.language` vs Rust `sys_locale` — may diverge outside Tauri; production path consistent.
