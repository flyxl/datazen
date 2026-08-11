# Multi-Track Platform Hardening

**Date:** 2026-08-11  
**Status:** Approved (design dialogue)  
**Organization:** Single mega-spec + five parallelizable workstreams (Approach A)

## Goals

1. **Updater:** Use GitHub Releases as the update distribution server; fix `update endpoint did not respond with a successful status code` (missing `latest.json` / unsigned artifacts / placeholder pubkey).
2. **History SQLite:** Persist workflow history and SQL query history in a local SQLite database; add clear-data UI with retention presets + custom days.
3. **Data sync policy:** Same dialect family → **direct** sync (no IR); cross SQL dialect (e.g. PostgreSQL ↔ MySQL) → keep **IR** path; cross category (SQL ↔ document / SQL ↔ KV / document ↔ KV) → unsupported (targets visible but disabled with hint). Account for version differences on same-type pairs.
4. **Window flash + lazy load:** Eliminate blank/flash on **main and all sub-windows** via delayed show until ready; deepen frontend lazy loading for faster perceived startup.
5. **Plugin locales:** Host `src/locales` contains only host strings; driver strings live in driver packages (`document.*` → `mongo.*` under mongodb); build script merges enabled drivers’ locales.

## Non-goals

- Removing IR-based cross-SQL sync (explicitly kept).
- Live resize of DB connection pools (separate prior work).
- Migrating dashboard monitor run history into the same SQLite in this track (optional follow-up; Monitor already has its own retention settings).
- CDN / third-party update mirrors beyond GitHub Releases.
- Translating driver `meta.ts` English product labels (optional later).

## Decisions (locked)

| Topic | Choice |
|-------|--------|
| Spec shape | One mega-spec; parallel workstreams where independent |
| Sync “same family” | Dialect family (normalize PG/MySQL aliases); direct path, no IR |
| Cross SQL dialects | Still supported via existing IR |
| Cross category | Unsupported; gray out + hint |
| History retention UI | Presets 7 / 30 / 90 days + custom days (1–365) + clear all |
| Updater keys | Generate with `tauri signer`; create GitHub secrets via `gh secret set`; no secrets exist today |
| Locales migration | Full driver strings out; `document.*` renamed to `mongo.*` in mongodb package |
| Window flash | **All windows** (main + every `create_sub_window` / docs / settings / connection / …) |

## Parallelization

```text
Wave 0 (parallel):
  W1 Updater (keys + CI + latest.json)
  W3 Sync policy (UI + backend gates + direct path scaffolding)
  W4 Window flash + lazy load (all windows)
  W5 Locales merge pipeline + move redis/mongo keys

Wave 1 (parallel with late Wave 0):
  W2 History SQLite + Settings clear UI + JSON migration

Wave 2 (after W5 key moves):
  Host locale cleanup / typecheck; fix any remaining host imports of mongo/redis keys
```

**Conflict hotspots to coordinate:**
- W4 ↔ W5: splash / settings strings stay host; no driver keys in splash path.
- W3 ↔ W5: new sync i18n keys are **host** keys (`sync.*`).
- W2 ↔ W4: Settings “Data cleanup” section loads lazily with Settings window.

---

## W1 — Updater / GitHub Releases

### Problem

Endpoint `https://github.com/flyxl/datazen/releases/latest/download/latest.json` returns **404**. CI skips updater artifacts when `TAURI_SIGNING_PRIVATE_KEY` is unset; `tauri.conf.json` pubkey is still a placeholder.

### Design

1. **Keys (local + GitHub)**  
   - Generate: `pnpm tauri signer generate -w <secure path>` (or equivalent).  
   - Set repo secrets with `gh secret set`:
     - `TAURI_SIGNING_PRIVATE_KEY`
     - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (if used)  
   - Replace `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` with the real public key.  
   - Document rotation in `docs/updater.md` (do **not** commit private key).

2. **CI (Basic SKU only)**  
   - Keep existing `UPDATER_BUILD=1` path when secret present.  
   - After Basic platform artifacts exist, add a job to **assemble and upload `latest.json`** for the release tag (via `tauri-action` updater JSON or a small script that maps renamed assets → Tauri v2 updater schema).  
   - Ensure `.sig` and updater bundles (e.g. macOS `.app.tar.gz`) are uploaded alongside installers.

3. **Client**  
   - Keep current frontend `UpdateSection` / `updater.ts`; no endpoint URL change required if `latest.json` lands on the existing path.  
   - Users on builds with the placeholder pubkey must install a new Basic build once to verify signatures.

### Acceptance

- `curl -sfL …/releases/latest/download/latest.json` returns 200 with platform entries.  
- Settings → Check for updates against a newer signed Basic release succeeds (or reports “up to date”) without endpoint status errors.

---

## W2 — History SQLite + clear data

### Storage

- New DB file: `{appData}/history.sqlite`.  
- Tables at minimum:
  - `query_history` (id, sql, connection_id/config_id, success, error, duration_ms, created_at, …)
  - `workflow_history` (id, workflow_id, name, status, payload JSON, created_at, …)
- Access via a small Rust module (e.g. `rusqlite` or `sqlx` sqlite) owned by Host store layer — **not** driver SQLite pools.

### Migration

- On first open after upgrade: if `history/queries.json` or `workflow_history/*.json` exist, import then rename/move aside (e.g. `.migrated`) so re-import does not duplicate.  
- Preserve existing count caps as soft limits **in addition to** time-based purge (exact numbers can match today’s 1000 / 100 until Settings overrides).

### Clear UI

- Settings section (e.g. **Data / Privacy** or under Behavior):  
  - Scope: SQL history, Workflow history, or both.  
  - Actions: keep last **7 / 30 / 90** days; **custom N days** (1–365); **clear all**.  
- IPC: `purge_history({ scope, retainDays: number | null })` where `null` / sentinel means delete all.  
- Wire SQL history clear into UI (today backend-only).

### Acceptance

- New runs appear in SQLite; UI lists still work.  
- Purge with retain=30 deletes older rows only.  
- Clear all empties selected scopes.  
- Old JSON files are migrated once without data loss for recent entries.

---

## W3 — Data sync strategy

### Pairing matrix

| Pair | Sync path | Target dropdown |
|------|-----------|-----------------|
| Same dialect family (e.g. postgresql↔postgresql/cloudberry after normalize; mysql↔mariadb; mongodb↔mongodb; redis↔redis; elasticsearch↔elasticsearch) | **Direct** (native copy / driver-assisted), **no IR** | Enabled |
| Cross SQL dialect (postgresql↔mysql, sqlite↔postgresql, …) | Existing **IR** pipeline | Enabled |
| Cross category (any SQL ↔ document; SQL ↔ KV; document ↔ KV; …) | **Forbidden** | Shown, **disabled**, hint「暂不支持」 |

### Direct vs IR

- Host chooses path in `compare` / `sync` entrypoints based on a shared `sync_pairing` helper:
  - `Direct { family }`
  - `Ir`
  - `Unsupported { reason }`
- Direct path: same-family adapters execute without IR translation; still surface **version skew** warnings when `server_version` (or equivalent) major/minor differs materially (warn, do not hard-block unless unsafe — default **warn**).
- IR path: unchanged behavior for supported SQL↔SQL pairs.

### UI

- Source select: all connections.  
- Target select: all connections listed; unsupported options **disabled** with tooltip/hint; same connection id still rejected.  
- Optional badge: Direct / IR on the compare button row.

### Backend

- Enforce the same matrix in `compare_databases` / `sync_tables` (defense in depth).  
- Update or split E2E: keep PG→MySQL IR case; add same-family direct case; assert cross-category rejected early.

### Acceptance

- Mongo→Redis / PG→Mongo targets grayed out with hint.  
- PG→MySQL still syncs via IR.  
- PG→PG (or mongo→mongo) uses direct path (observable via log or API flag in tests).

---

## W4 — Window flash (all windows) + frontend lazy load

### Problem

Flash/blank is not limited to main: sub-windows also show before theme/content is ready. Today readiness is split (HTML early `show` for main; Rust `on_page_load` for subs; splash hides on `React.render`).

### Design — unified ready gate (every window)

1. All windows start **`visible: false`** with dark/light `backgroundColor` matching splash.  
2. **Do not** show on bare HTML load or solely on `PageLoadEvent::Finished`.  
3. Frontend bootstrap (per window label) sequence:
   - Apply theme (and pack if cheap)  
   - `React.render` + resolve active window’s lazy shell  
   - First meaningful paint (or `Suspense` resolved for that window)  
   - Then `window.show()` + `hideSplash()`  
4. Rust may keep a **safety timeout show** (e.g. 8–10s) so a failed frontend never leaves an invisible window (ACL/module load).  
5. Apply identically to: main, settings, connection, workflow, dashboard, backup, data-sync, docs, and any future `create_sub_window` callers.

### Design — lazy load

- Keep `App.tsx` per-`windowKind` lazy.  
- Add inner lazy boundaries for heavy panels (connection views, Redis workbench, ER, chart, data-sync body).  
- Defer non-critical startup work (updater check, secondary stores) until after first show.  
- Optional Vite manual chunks for `vendor` / window groups if measurable.

### Acceptance

- Opening main and each major sub-window shows splash/solid bg then content — no white flash, no empty chrome flash.  
- Cold start time-to-interactive improved or unchanged; main bundle no longer pulls unused window graphs eagerly.

---

## W5 — Plugin locales ownership

### Layout

```text
packages/drivers/redis/locales/{zh-CN,en,...}.ts   # redis.*
packages/drivers/mongodb/locales/{zh-CN,en,...}.ts # mongo.*  (renamed from document.*)
src/locales/*                                      # host-only keys
```

### Build merge

- Extend `resolve-drivers.mjs` (or sibling invoked by it) to:
  - Collect locale modules for **enabled** drivers only  
  - Emit generated merge consumed by host i18n (e.g. `src/plugins/generated-locales.ts` or fold into `generated.ts`)  
  - Host `getTranslation` / `useI18n` resolves host dict + merged plugin dict (plugin keys namespaced by prefix)
- Stub / restore rules: generated locale merge is **not** committed injected (same as other resolve-drivers outputs); git keeps empty stub.

### Code moves

- Move all `redis.*` out of `src/locales`.  
- Rename `document.*` → `mongo.*`; move to mongodb package; update `DocumentConnectionView` (or mongodb UI) imports.  
- Plugin UI uses plugin-sdk i18n that types plugin keys or accepts `string` namespaced keys.  
- Host `TranslationKey` no longer includes redis/mongo keys.

### Acceptance

- Building with `DATAZEN_DRIVERS=basic` without redis strips `redis.*` from runtime merge (or leaves them absent).  
- With redis enabled, Redis UI strings still resolve.  
- Typecheck/tests: host locale key parity excludes plugin keys.

---

## Testing (cross-cutting)

| Track | Minimum verification |
|-------|----------------------|
| W1 | CI dry-run / docs checklist; curl latest.json after next signed release |
| W2 | Rust unit tests for migrate + purge; Settings UI unit/e2e smoke |
| W3 | Unit tests for pairing helper; UI test disabled options; sync e2e matrix update |
| W4 | Manual all-window open; optional e2e screenshot/timing; vitest for ready-gate helper |
| W5 | Locale parity tests; resolve-drivers stub/inject/restore; Redis/Mongo UI smoke |

## Error handling

- Updater: surface clear UI when endpoint 404 or signature mismatch.  
- History SQLite: if DB corrupt, recreate empty + log; do not block app start.  
- Sync unsupported: fail fast with stable error code before connect work where possible.  
- Window ready timeout: show anyway + log warning.  
- Locale merge missing key: fallback en → key string (existing pattern).

## Rollout notes

- W1 requires a new Basic release after pubkey + secrets land.  
- W2 migration is one-shot per machine.  
- W3 may change defaults for users who relied on SQL↔document attempts (those already failed at runtime).  
- W5 requires `pnpm drivers:restore` discipline so injected locale merges are not committed.
