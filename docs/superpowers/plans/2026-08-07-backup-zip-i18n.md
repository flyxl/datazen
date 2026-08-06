# App Data ZIP + 10-Locale i18n Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (or executing-plans). Steps use checkbox (`- [ ]`) syntax.

**Goal:** Replace config JSON import/export with full app-data ZIP; add 10 locales; then delete GH v0.0.7 and rebuild.

**Architecture:** Rust `zip` pack/unpack of `Store::data_dir()` with exclusions + path checks; frontend dialogs/confirm/restart. Locales remain TypeScript dicts keyed by `TranslationKey` from `zh-CN.ts`.

**Tech stack:** Tauri v2, Rust `zip`, React, existing `t()` / settings language.

**Progress ledger:** `docs/progress-backup-i18n.md`

---

## File map

| Area | Files |
|------|--------|
| ZIP core | `src-tauri/src/commands/config.rs` (or new `backup.rs`), `src-tauri/Cargo.toml` |
| Commands register | `src-tauri/src/lib.rs`, `commands/mod.rs` |
| Frontend API | `src/commands/connection.ts` or new `src/commands/backup.ts` |
| UI | `src/windows/main/MainWindow.tsx`, remove/retire `ImportConfigDialog.tsx` usage |
| i18n | `src/locales/*`, `index.ts`, `SettingsWindow.tsx`, `generate-menu-labels.mjs` |
| Tests | `src-tauri/src/commands/config.rs` `#[cfg(test)]` or `src-tauri/tests/backup_tests.rs`; Vitest for locale registry |

---

### Task 1: App data ZIP export/import

**Goal:** Working export/import replacing old connection JSON flow.

- [ ] Add `zip` dependency to `src-tauri/Cargo.toml`
- [ ] Implement pure functions: `should_exclude`, `zip_dir`, `unzip_to` with traversal guards
- [ ] Write Rust unit tests (exclude logs, traversal reject, round-trip)
- [ ] Commands `export_app_data` / `import_app_data`; register; remove or stop exposing old export/import commands from UI
- [ ] Frontend: dialogs, confirm overwrite, restart; update locale strings for menu/actions
- [ ] Update progress file
- [ ] Dispatch **fresh test agent** for E2E cases/results (no fixes)
- [ ] If fail → coding agent fixes → retest
- [ ] Commit when green

### Task 2: Ten locales

**Goal:** Settings can select any of the 10 languages; UI strings resolve.

- [ ] Add locale files + register in `index.ts`
- [ ] Settings dropdown + menu-labels script
- [ ] Vitest: key parity / supported list
- [ ] Progress + test agent + fix loop + commit

### Task 3: Release ops

- [ ] Delete GitHub release/tag `v0.0.7`
- [ ] Trigger rebuild (workflow_dispatch or documented release script)
- [ ] Record outcome in progress file + commit note if needed
