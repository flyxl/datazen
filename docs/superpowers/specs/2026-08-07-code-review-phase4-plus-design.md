# Design: Code Review Phase 4+ (C2/C6/P3–P5 + Backup/Share UX)

**Date:** 2026-08-07  
**Branch:** `fix/code-review-phase-1-3`  
**Status:** Pending user review  
**Progress ledger:** [`docs/progress-code-review-fix.md`](../../progress-code-review-fix.md)  
**Prior plan:** [`docs/code-review-2026-08-07-full.md`](../../code-review-2026-08-07-full.md)

## Goals

Ship remaining correctness/UX/security items on the existing review branch:

1. **S1+** — After app-data ZIP export, prompt user to **separately back up `.key`** (optional save dialog).
2. **Connection share UX** — Menu entries for **export/import connections** with a user-chosen decryption password.
3. **C2** — Full real translations for all 10 locales; remove Beta marking.
4. **P3** — Move local encryption master key to OS Keychain/credential store; connection export still uses password (independent of Keychain).
5. **C6** — Hard rename: MCP/params that mean config ID use `config_id` only (no `connection_id` alias).
6. **P4** — SQL/NL payloads: `info` → `debug` / redact.
7. **P5** — Splash waits for bootstrap; user-visible hardcoded errors → i18n.

## Approved decisions

| Topic | Choice |
|-------|--------|
| C6 MCP API | **Hard switch** to `config_id`; reject old `connection_id` field |
| Menu layout | **Option 1**: Relabel existing items to App Data export/import; **add** Export/Import Connections (4 items total) |
| App-data ZIP `.key` | Still **excluded** from ZIP; post-export prompt to back up `.key` separately |
| Connection share crypto | User-set password + Argon2 (existing `export_connections` format v2); **not** Keychain material |
| P3 storage | OS Keychain (macOS) / equivalent via `keyring` crate; migrate from file `.key` then delete file |
| C2 quality | Full translation of all keys for de/es/fr/ja/ko/pt-BR/ru (+ polish zh-TW); remove `(Beta)` UI |
| Out of scope | P1 LTO CI tweak, P2 `max_tokens` default (unless pulled in later) |

## Architecture overview

```
App Data ZIP ──exclude──► .key (never in archive)
     │
     └─ after export ──► prompt + optional save_key_with_dialog

Local at-rest ──► Keychain/DPAPI (P3) ──encrypts──► connections.json passwords / ai_config.enc

Share to others ──► export_connections_with_dialog(password)
                 ◄── import_connections_*_with_dialog(password) → merge into Store (re-encrypt with local key)
```

---

## 1. S1+ — Separate `.key` backup prompt

### Behavior

- After successful `export_app_data_with_dialog`, show success message that **explicitly tells the user to back up the encryption key separately** if they need cross-machine password recovery.
- Offer a secondary action (dialog button or follow-up confirm): **Save encryption key…**
  - On accept: Rust `save_key_with_dialog` copies the current master key material to a user-chosen path (default name e.g. `datazen.key`).
  - Never put `.key` into the ZIP.
- Import path unchanged: if ZIP has no `.key`, preserve local key when present; otherwise new key → passwords unreadable (document in UI strings).

### IPC

- Add `save_encryption_key_with_dialog(default_file_name)` — dialog + write only; path never crosses webview.
- Optional E2E: `webdriver`-gated `export_encryption_key(path)` if needed for automation.

### Tests

- Unit: key export writes expected bytes / refuses if no key yet.
- E2E or source assert: MainWindow success path mentions separate key backup / calls save helper.

### i18n

- New keys under `appData.*` (en + zh-CN first; C2 propagates to all locales).

---

## 2. Connection export / import menu (share UX + password)

### Menu (native + `MenuBar.tsx`)

| Menu id | Label intent (i18n) |
|---------|---------------------|
| `export-config` | Export App Data (rename from “Export Config”) |
| `import-config` | Import App Data |
| `export-connections` | Export Connections |
| `import-connections` | Import Connections |

Wire new events: `menu:export-connections`, `menu:import-connections`.

### Why new `*_with_dialog` commands

Path-based `export_connections` / `import_connections_preview` are **`webdriver`-gated**. Production UI must use dialog-atomic IPC (same pattern as app-data backup).

### Export flow

1. Prompt user for password (min length validated; confirm match).
2. Native save dialog → write encrypted JSON (format version 2, salt, Argon2-derived key) — reuse existing encrypt helpers.
3. Success toast with count.

### Import flow (v1 — locked)

1. Frontend prompts for password (empty rejected).
2. Single IPC `import_connections_with_dialog(password)`:
   - Native open dialog → read file → decrypt (reuse preview crypto) → merge into Store → return `{ imported, overwritten, groupsAdded }`.
3. Merge policy (**fixed**): match on `ConnectionConfig.id`; existing id → **overwrite**; new id → **insert**; groups union.
4. Secrets re-encrypted with **local** master key before disk write (Keychain after P3).
5. Frontend refreshes connection list; no plaintext password list returned to webview.

Path-based `import_connections_preview` remains **webdriver-only** for E2E. Conflict-resolution UI is **out of scope** for v1.

**Supersedes** `docs/superpowers/specs/2026-08-07-backup-zip-i18n-design.md` § that removed connection JSON export: both App Data ZIP **and** passworded connection export coexist.

### Tests

- Rust: password round-trip; wrong password fails; dialog command unit-tested where feasible.
- E2E (webdriver path IPC): keep PIH-003; add UI source asserts for new menu ids.
- Vitest: command wiring camelCase.

---

## 3. C2 — Full translations

### Scope

- Locales: `de`, `es`, `fr`, `ja`, `ko`, `pt-BR`, `ru` — replace English placeholders with real translations for **every** key present in `en.ts` / `zh-CN.ts`.
- `zh-TW`: fix simplified/mixed strings; align key set and meaning with zh-CN where appropriate (Traditional Chinese).
- Remove `BETA_LOCALES` / `(Beta)` suffix from Settings language picker (or empty the beta set).
- Update `FULLY_TRANSLATED_LOCALES` to all 10.
- Menu label generator / Rust menu i18n strings stay in sync for new keys (`export-connections`, etc.).

### Quality bar

- Not machine-garbled: prefer fluent UI phrasing; keep placeholders `{name}` intact.
- Parity tests: every locale has exact same key set as `en`; **English equality rate** for non-en locales must be below a threshold for natural-language keys (allow shared tokens like `OK`, brand `DataZen`, SQL keywords if any).

### Delivery

- May land as **one commit per locale** or one batched C2 commit if review prefers; progress ledger tracks per-locale checkboxes.

---

## 4. P3 — Keychain migration + export password (already required)

### Local master key

- Introduce `keyring` (or Tauri-approved equivalent) service name e.g. `com.tbeasy.datazen` / account `app-encryption-key`.
- `Store::get_or_create_encryption_key`:
  1. Try Keychain.
  2. Else if `.key` file exists: load → store in Keychain → delete `.key` (best-effort).
  3. Else: generate random 32 bytes → Keychain.
- Fallback: if Keychain unavailable (CI/headless), keep file `.key` behind feature or env `DATAZEN_KEYRING=file` for tests.
- App-data ZIP still excludes `.key`; Keychain secrets are **never** in ZIP. Cross-machine: use connection export password **or** separately saved key file from S1+.

### Export connections password

- UI must require password (empty rejected).
- Backend already derives Argon2 key; ensure SSH secrets included (already).
- Document: recipients enter **the export password**, not the sender’s OS login / Keychain.

### Tests

- Unit with file fallback: migrate file → keyring mock or file mode.
- Encryption round-trip unchanged under new key source.

---

## 5. C6 — `config_id` hard switch

### Rules

| Name | Meaning |
|------|---------|
| `config_id` | Persistent `ConnectionConfig.id` |
| `connection_id` | Runtime handle from `ConnectionManager` |

### Changes

- MCP tool/prompt parameter structs: rename fields to `config_id`; JSON schema/docs updated.
- Deserialization: **do not** accept `connection_id` for those params (tests assert old JSON fails or field missing).
- `db_tools` / `resolve_connection` argument names aligned.
- Schema URI docs: first path segment is `config_id`.
- GUI IPC that already uses correct names: leave; misnamed TS/Rust params fixed.
- Update `docs/architecture/backend/services.md` and MCP docs.

### Tests

- MCP struct parse tests updated; negative test for old field name.
- Frontend/MCP client call sites updated.

---

## 6. P4 — Log hygiene

- `execute_query`: info logs `connection_id` + `sql_len` only; move `sql_preview` to `debug`.
- AI commands: do not `info!` full NL / SQL / tool args; use lengths / ids at info, full text at debug.
- Workflow: resolved SQL / row dumps at debug.
- Prefer small helper `truncate_for_log` only at debug call sites if needed.

### Tests

- Spot-check via grep in CI-ish unit test or `#[cfg(test)]` asserting source patterns is brittle; prefer code review + optional tracing subscriber test for one command.

---

## 7. P5 — Splash + i18n errors

- `src/main.tsx`: hide/remove splash **after** `bootstrap()` completes (success or catch); never leave splash stuck.
- Scan startup / connection user-visible hardcoded zh/en strings → locale keys.
- Tests: bootstrap/splash ordering if extractable; locale keys exist.

---

## Process (same as phases 1–3)

1. Update `docs/progress-code-review-fix.md` with new rows (S1+, ConnShare, C2, P3–P5, C6 rename).
2. Per item: implement + unit tests → **independent test agent** (no fix) → coding agent if fail → commit when green.
3. Commit message style: `fix(ID): …` / `feat(ID): …` / `test: …`.
4. Do not mix unrelated WIP.

## Suggested implementation order

One umbrella implementation plan with **7 independently committable tasks** (same order):

1. P4 (small, low risk)  
2. P5 splash/i18n  
3. C6 hard rename  
4. S1+ key backup prompt + IPC  
5. ConnShare dialog IPC + menu + password prompts  
6. P3 Keychain (file→keyring migration; test fallback `DATAZEN_KEYRING=file`)  
7. C2 translations last (so new keys from 1–6 are included)

C2 may be split into per-locale commits under the same task id.

## Non-goals

- Putting master key into ZIP.
- Sharing Keychain entries across machines.
- Full conflict-resolution UI for connection import (v1 overwrite-by-id).
- P1/P2 unless explicitly requested later.
