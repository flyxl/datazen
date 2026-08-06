# Design: App Data ZIP Backup + 10-Locale i18n

**Date:** 2026-08-07  
**Branch:** `feat/backup-zip-i18n`  
**Status:** Approved

## Goals

1. Replace connection-config JSON import/export with **full application data directory ZIP** backup/restore.
2. Expand i18n to the **10 most common locales**.
3. After both features ship and pass tests: delete GitHub `v0.0.7` release/tag and trigger rebuild.

## Decisions (approved)

| Topic | Choice |
|-------|--------|
| Import/export UX | **Replace** old encrypted connections JSON flow |
| ZIP contents | Almost all app data dir; **exclude `logs/`** (and obvious temp if present) |
| Locales | `en`, `zh-CN`, `zh-TW`, `es`, `fr`, `de`, `ja`, `pt-BR`, `ru`, `ko` |
| After import | Confirm overwrite → apply → **force app restart** |

## Feature 1 — ZIP backup

### Behavior

- **Export:** Recursively zip `store.data_dir()` preserving relative paths. Skip `logs/` and names matching temp patterns (e.g. `.tmp`, `*.tmp`). Save via system dialog as `datazen-backup-YYYYMMDD.zip`.
- **Import:** Pick zip → warn that this **overwrites all app data** → on confirm: extract safely (path-traversal check), replace data dir contents (preserve `logs/` if present on disk), then `app.restart()`.
- Menu/actions: keep ids `export-config` / `import-config` for menu wiring; **relabel** to “导出/导入应用数据” (and EN equivalents).
- Remove password dialogs, `ImportConfigDialog` conflict UI, and Argon2 re-encrypt path used only for old export.

### Backend

- New helpers in `src-tauri/src/commands/config.rs` (or `backup.rs`): `export_app_data(path)`, `import_app_data(path)`.
- Dependency: `zip` crate.
- Security: reject zip entries with `..` or absolute paths; extract to staging dir then swap.

### Frontend

- `MainWindow` / menu listeners: save/open dialog → invoke new commands → toast → restart on import success.
- Delete/stop using `ImportConfigDialog` for this flow; simplify export (no password).

### Tests

- Unit: exclude `logs/`; reject traversal; round-trip fixture dir → zip → extract matches (minus exclusions).

## Feature 2 — 10 locales

### Behavior

- Extend `SupportedLocale` and `locales/index.ts` registration.
- New locale files: copy `en.ts` as baseline for non-en/zh-CN (acceptable quality for v1); `zh-TW` can adapt from `zh-CN`.
- Settings language dropdown lists all 10.
- `scripts/generate-menu-labels.mjs` includes all locales that have keys (fallback to `en` for missing menu keys if needed).

### Tests

- Unit: all 10 locales load; key-set equals `zh-CN` keys; unsupported code falls back safely.

## Feature 3 — Release

- `gh release delete v0.0.7 --yes` (and delete tag if present).
- Re-trigger release workflow / republish same version per project convention (`bump-version` / `workflow_dispatch`).

## Process

1. Progress file: `docs/progress-backup-i18n.md` updated per feature.
2. Per feature: implement + unit tests → **fresh test agent** (E2E cases + results only) → fix agent if fail → commit when green.
3. Do not mix unrelated WIP into feature commits when avoidable.
