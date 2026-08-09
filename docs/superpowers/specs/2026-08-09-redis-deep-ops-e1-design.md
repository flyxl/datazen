# Redis Deep Ops E1 + Plugin Settings Extension Points

> **Status:** Implemented  
> **Plan:** `docs/superpowers/plans/2026-08-09-redis-deep-ops-e1.md`  
> **Date:** 2026-08-09  
> **Scope of this spec:** E1 only (shippable). Later milestones are listed for roadmap clarity, each with its own future spec/plan.

## 1. Context

DataZen Redis today is a **read-only SCAN browser** plus a thin command executor (`kv_scan_keys` / `kv_get_key` + Queries tab). Competitors (DBX, TablePlus, Another Redis Desktop Manager) expose mutating UX, batch ops, observability, and richer console tooling.

Program order (approved):

**E1 → E2 → E3 → E4 → A → B → D → C**

| ID | Topic |
|----|--------|
| **E1** | Redis CRUD / batch / Size / Flush + host-neutral plugin settings |
| E2 | Console (history/completion) + INFO / MEMORY / Slowlog |
| E3 | TLS + Cluster + Sentinel connection modes |
| E4 | Pub/Sub + RedisJSON editor + Stream consumer groups + key import/export |
| A | In-DB object search (connected + on-demand); **phase-2 must add cross-all-saved-connections search** |
| B | Schema Diff productization |
| D | File preview (Parquet/CSV/JSON) |
| C | Moat packaging (Workflow templates, Explain+charts narrative, MCP docs) |

## 2. Goals (E1)

1. Type-aware **CRUD editors** for `string` / `hash` / `list` / `set` / `zset` (create key, edit value/elements, delete key).
2. **Batch ops:** multi-select delete; delete-by-pattern (second confirm); batch TTL; batch rename-by-prefix.
3. Show **Size** column in the key list (backend already computes size in scan).
4. **FLUSHDB / FLUSHALL** behind a plugin setting (default off) + typed confirmation when enabled.
5. **Stream** remains **read-only** in E1 (full Stream UX in E4).
6. Host application code must **not** contain Redis-specific settings fields or Redis business branches.

## 3. Non-goals (E1)

- Console history/completion, Slowlog, MEMORY/INFO dashboards (E2).
- `rediss://`, Cluster, Sentinel (E3).
- Pub/Sub, RedisJSON tree editor, Stream groups, DUMP/RESTORE import-export (E4).
- Expanding host `KeyValueDriver` with a large mutate surface (rejected).
- Hard-coding `redis.allowFlush` (or similar) into `AppSettings` / `SettingsWindow`.

## 4. Architecture principles

### 4.1 Driver-owned commands (Kiwi pattern)

Redis path driver gains a `tauriPlugin` block in `drivers-registry.json`, same pattern as Kiwi:

- Rust: `packages/drivers/redis` optional `tauri-plugin` feature; `init()` registers `plugin:redis|*` commands.
- Host: `resolve-drivers.mjs` injects dep features, `plugin_init.rs`, and `redis:default` ACL.
- Frontend: `pluginInvoke('redis', command, args)` from driver UI only.

Host keeps existing **read** KV IPC (`kv_scan_keys`, `kv_get_key`) for E1 unless migration is trivial; **all mutating / flush / batch** go through `plugin:redis|*`. A later cleanup may move reads into the plugin; not required to ship E1.

### 4.2 Driver-owned UI

Deep Redis UX lives under `packages/drivers/redis/ui/` (editors, batch toolbar, optional settings section). Host `RedisConnectionView` becomes a thin shell that mounts the driver view, or is replaced via driver frontend registration (`FRONTEND_DRIVER_CONFIG` / `connectionView` contribution). Host must not grow Redis-only editing logic.

### 4.3 Host-neutral plugin settings (both channels)

Host provides **extension points only**:

| Piece | Responsibility |
|-------|----------------|
| Storage | `AppSettings.pluginSettings: Record<string, unknown>` keyed by `pluginId` (e.g. `"redis"`). Opaque to host. |
| Persistence | Existing settings load/save path; merge unknown keys safely; default `{}`. |
| Settings UI | A single host “Extensions” / “Drivers” area that **discovers** contributions from active drivers. |
| Channel A | Driver exports a React `SettingsSection` component → host mounts it. |
| Channel B | Driver exports a JSON Schema (+ optional UI schema) → host renders a **generic** form. |
| Discovery | Generated registry entries (or static exports merged in `generated.ts`) listing `{ pluginId, settingsSection?, settingsSchema? }`. Prefer Section when both exist; else Schema; else hide. |

**Hard rule:** No `if (pluginId === 'redis')` in host settings code. Redis Flush gate lives in Redis’s Section and/or Schema (`allowFlush`, default `false`). Redis UI and Redis plugin commands read `pluginSettings.redis` themselves (frontend passes flag / backend re-checks from store if commands receive app handle).

### 4.4 Why not expand `KeyValueDriver`

Mutations, batch semantics, Flush gates, and later Cluster/JSON are Redis-specific. Polluting the shared KV trait couples hypothetical future KV engines to Redis ops. Plugin commands keep the host thin and match existing Kiwi ACL story.

## 5. E1 product behavior

### 5.1 Editors

| Type | E1 behavior |
|------|-------------|
| string | Edit value; save via plugin command |
| hash | Add/edit/delete fields |
| list | Push/pop/set-by-index (as needed for day-to-day edit) |
| set | Add/remove members |
| zset | Add/update score/remove members |
| stream | Read-only detail (existing JSON/entries view); no write UI |

Also: create key (type picker), rename single key, set/persist TTL on detail panel.

### 5.2 Batch bar

- Multi-select rows → Delete (confirm count).
- “Delete matching pattern” → confirm with pattern + approximate/exact count when cheap.
- Batch set TTL / Persist.
- Batch rename prefix (`oldPrefix` → `newPrefix`) with dry-run preview count when feasible.

Partial failures return per-key errors; UI shows summary and refreshes scan.

### 5.3 Size column

Display size from scan payload; empty/unknown shown as `—`.

### 5.4 Flush

- Default: controls hidden / disabled (`allowFlush !== true`).
- When enabled in Redis plugin settings: show Flush DB / Flush All.
- Execute: typed confirmation (`db index` or `ALL`) then `plugin:redis|flush_*`.
- Backend must reject flush if setting is not enabled (defense in depth), not only hide the button.

### 5.5 Empty databases

E1 may list logical DBs `0..15` (or INFO-derived + allow select empty index) so users can switch into empty DBs; exact UX left to plan but must not block creating first key in an empty DB.

## 6. Command surface (illustrative)

Exact names locked in the implementation plan. Directionally:

- `set_string`, `hash_*`, `list_*`, `set_*`, `zset_*`
- `delete_keys`, `rename`, `set_ttl`
- `batch_delete_pattern`, `batch_set_ttl`, `batch_rename_prefix`
- `flush_db`, `flush_all` (gated)

All require `connection_id` (+ `db_index` as needed). Prefer structured args over raw command strings for mutate paths.

## 7. Security & safety

- Destructive ops: confirmations in UI; Flush double-gated (setting + typed confirm + backend check).
- Plugin ACL: only allow-listed redis commands in `redis:default`.
- No secrets in `pluginSettings` beyond what settings encryption already covers for `AppSettings`.
- Pattern delete / flush must not run without explicit user confirmation.

## 8. i18n

- Host: generic Extensions section chrome only (`settings.extensions.*`).
- Redis strings: prefer driver-local or shared locale keys under a redis namespace contributed without host hardcoding business copy beyond mounting.

(Plan may use existing `src/locales` `redis.*` keys for E1 speed if driver-local i18n is not ready; still no Redis **logic** in host settings.)

## 9. Testing

- Rust: redis plugin command unit/integration tests (mutate, batch partial failure, flush gate).
- Frontend: Vitest for editors, batch confirmations, settings Section/Schema mount without host redis branches.
- Host: `pluginSettings` round-trip merge/default.
- E2E: extend `e2e/specs/redis.ts` for create/edit/delete, batch delete, flush disabled-by-default.

## 10. Rollout

1. Land host `pluginSettings` + Extensions discovery (unblocks all plugins).
2. Add redis `tauriPlugin` + mutate commands.
3. Move/enhance Redis UI in driver package; wire Size + batch + editors.
4. Flush gated by redis plugin settings.
5. Docs: update competitive notes / AGENTS pointer as needed in plan tasks.

## 11. Open points for the plan (not blockers for this spec)

- Whether E1 migrates `kv_scan_keys` into `plugin:redis` or keeps host read IPC temporarily.
- Exact list/set/zset editor control set (minimal vs full AnotherRedis parity).
- JSON Schema dialect version for Channel B (recommend draft-07 or a small internal subset).

## 12. Success criteria

- User can edit the five core types and run the approved batch ops without using the Queries tab.
- Size column visible.
- Flush impossible until redis plugin setting enabled; then requires typed confirm; backend enforces gate.
- `rg` / review: host settings has **no** redis-specific fields or `pluginId === 'redis'` branches.
- Existing redis e2e browse cases still pass; new write-path cases pass under basic drivers build.
