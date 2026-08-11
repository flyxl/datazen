# Multi-Track Platform Hardening — Implementation Plan

> **For agentic workers:** Use subagent-driven or parallel Wave 0 tasks. Spec: `docs/superpowers/specs/2026-08-11-multi-track-platform-hardening-design.md`.  
> **Do not commit** unless the user explicitly asks. Do not commit private signing keys.

**Goal:** Ship updater GitHub Releases fix, history SQLite + purge UI, sync pairing policy, all-window flash fix + lazy load, plugin locale ownership — per approved mega-spec.

**Architecture:** Five workstreams; Wave 0 parallel (W1/W3/W4/W5), Wave 1 W2, Wave 2 locale cleanup.

**Tech Stack:** Tauri v2 updater, GitHub Actions, rusqlite/sqlx-sqlite, React lazy, resolve-drivers locale merge.

## Global Constraints

- Spec pairing matrix and all-window ready-gate are mandatory
- Host locales must not retain `redis.*` / `mongo.*` after W5
- `document.*` → `mongo.*`
- No private key in git
- Preserve PG↔MySQL IR sync

---

### Task W1: Updater keys + CI `latest.json`

**Files:** `src-tauri/tauri.conf.json`, `.github/workflows/release.yml`, `docs/updater.md`, scripts as needed

- [ ] Generate signing keypair; `gh secret set TAURI_SIGNING_PRIVATE_KEY` (+ password if any)
- [ ] Replace placeholder `plugins.updater.pubkey`
- [ ] Add CI job to build/upload `latest.json` for Basic release assets
- [ ] Document verification curl in `docs/updater.md`

### Task W3: Sync pairing

**Files:** `src-tauri/src/sync/` (new pairing helper), `src-tauri/src/commands/sync/`, `src/windows/data-sync/DataSyncWindow.tsx`, locales `sync.*`

- [ ] Implement `sync_pairing` → Direct | Ir | Unsupported
- [ ] Backend enforce; UI disable unsupported targets + hint
- [ ] Tests for matrix; keep IR PG↔MySQL

### Task W4: All-window flash + lazy load

**Files:** `index.html`, `src/main.tsx`, `src/lib/splash.ts` (or new ready gate), `src-tauri/src/commands/window.rs`, `src/App.tsx`, heavy window components

- [ ] Remove early main `show` from HTML; unified frontend ready → show + hideSplash
- [ ] Sub-windows: stop show-on-page-load-only; same ready gate + Rust timeout fallback
- [ ] Inner React.lazy for heavy panels; defer updater check post-show

### Task W5: Plugin locales

**Files:** `packages/drivers/redis/locales/*`, `packages/drivers/mongodb/locales/*`, `scripts/resolve-drivers.mjs`, `src/locales/*`, plugin UI imports

- [ ] Extract redis/mongo locale files; rename document→mongo
- [ ] Merge in resolve-drivers; stub/restore
- [ ] Host TranslationKey drops plugin keys

### Task W2: History SQLite

**Files:** new `src-tauri/src/store/history_db.rs` (or similar), migrate from JSON, Settings clear UI, IPC

- [ ] `{appData}/history.sqlite` schema + migrate
- [ ] Wire query/workflow history read/write
- [ ] Settings purge: 7/30/90 + custom + clear all

### Verification

- [ ] Targeted cargo/vitest per track
- [ ] Manual: all windows no flash; sync gray-out; locales with/without redis
