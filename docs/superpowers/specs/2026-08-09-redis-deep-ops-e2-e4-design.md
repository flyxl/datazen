# Redis Deep Ops E2–E4

> **Status:** Approved  
> **Plan:** `docs/superpowers/plans/2026-08-09-redis-deep-ops-e2-e4.md`  
> **Date:** 2026-08-09  
> **Depends on:** E1 Implemented (`docs/superpowers/specs/2026-08-09-redis-deep-ops-e1-design.md`)  
> **Scope:** One joint design for E2 + E3 + E4. Implementation may land as sequential PRs (E2 → E3 → E4) on the same branch series.

## 1. Context

E1 shipped driver-owned Redis mutate UX via `plugin:redis|*`, workbench UI under `packages/drivers/redis/ui/`, and host-neutral `pluginSettings` (including `allowFlush`).

Remaining competitive gaps vs DBX / RedisInsight / Another Redis Desktop Manager:

| Milestone | Topic |
|-----------|--------|
| **E2** | Deep console + Monitor (Info / Memory / Slowlog) |
| **E3** | TLS (mTLS) + Cluster + Sentinel connection modes |
| **E4** | Pub/Sub + RedisJSON editor + Stream consumer groups + key import/export |

Program order after this package remains **A → B → D → C** (in-DB search, Schema Diff, file preview, moat packaging)—out of scope here.

## 2. Goals

### E2

1. **Deep command console:** per-connection command history, static Redis command completion, key-name completion from current SCAN/cache, multi-result panes for multi-statement runs.
2. **Monitor tab** with sub-pages: **Info**, **Memory**, **Slowlog** (and Stream overview contributed by E4).

### E3

1. **Connection wizard:** choose topology first (Standalone / Cluster / Sentinel), then nodes / master name, then TLS.
2. **Full mTLS:** system CA and/or custom CA; client certificate + private key + optional passphrase; optional skip-verify for troubleshooting.
3. **Cluster routing setting** in `pluginSettings.redis`: `clusterRouting: "auto" | "pinnedNode"` (default `"auto"`); pinned mode shows a node picker.
4. **Sentinel failover:** on connection loss, re-query Sentinel for current master and reconnect; toast when master changes.

### E4

1. **Pub/Sub tab:** subscribe (channels / patterns) and publish; live message stream.
2. **RedisJSON tree editor:** full tree edit when module present; **capability probe** (`MODULE LIST` / probe); hide JSON entry points when absent.
3. **Stream:** full UX in key detail (browse messages, manage consumer groups, read pending, XACK); **Monitor → Stream overview** (group counts / lag).
4. **Import/export:** DUMP/RESTORE fidelity (multi-key / pattern → zip + manifest) plus optional human-readable JSON export (best-effort, not type-perfect).

## 3. Non-goals

- Expanding host `KeyValueDriver` with a large mutate / topology surface.
- Host settings branches on `pluginId === 'redis'` (plugin settings stay opaque + driver Section/Schema).
- Teaching the generic SQL editor Redis-specific history/completion (console lives in driver UI).
- Emulating RedisJSON via string JSON when the module is missing.
- Cross-connection global search (roadmap **A**).

## 4. Architecture principles

### 4.1 Continue E1 pattern (scheme A)

- All Redis-specific IPC: `plugin:redis|*` with camelCase args (Tauri wire format; same as host invoke).
- Deep UI under `packages/drivers/redis/ui/`; host `RedisConnectionView` remains a thin tab shell.
- Host-neutral `AppSettings.pluginSettings.redis` for Redis-only toggles (`allowFlush`, `clusterRouting`, future flags).

### 4.2 Connection config extension

Topology and TLS material that do not fit existing `ConnectionConfig` fields live in an **opaque options bag** on the connection config (e.g. `options: Record<string, unknown>` / Rust `HashMap` or `serde_json::Value`), keyed for Redis without host interpreting Redis semantics.

Suggested logical keys (names locked in plan):

- `redis.topology`: `standalone` | `cluster` | `sentinel`
- `redis.clusterNodes` / seed nodes
- `redis.sentinelMasterName`, sentinel endpoints, optional node password
- `redis.tls`: enable, caPath, certPath, keyPath, keyPassphrase, insecureSkipVerify

Host connection persistence must round-trip `options` unchanged. Redis driver form + `connect` read them.

### 4.3 Tabs (connection window)

| Tab | Owner | Contents |
|-----|--------|----------|
| Items / 数据浏览 | Driver workbench | E1 CRUD/batch + JSON editor (when capable) + Stream detail + import/export actions |
| Queries / 命令 | Driver console | Deep console (replaces thin host `SqlEditor` panel for Redis) |
| Monitor / 监控 | Driver | Sub-pages: Info · Memory · Slowlog · Stream overview |
| Pub/Sub | Driver | Subscribe + publish |

Host shell only mounts tabs and passes `connectionId` / initial database.

## 5. E2 product behavior

### 5.1 Console

- Editor accepts Redis CLI syntax (multi-line / multi-command as today).
- **History:** persist per connection id (local app data or encrypted store alongside settings—plan chooses; must not leak passwords).
- **Completion:** static command catalog + key names from last SCAN / in-memory cache for current DB.
- **Results:** each statement gets its own result pane/tab when multiple commands run.
- Execution still goes through existing query path **or** plugin `exec` if plan migrates—must respect Cluster routing mode.

### 5.2 Monitor

- **Info:** run/parse `INFO` (sections collapsible; Cluster: optional per-node when pinned or when API returns multi-node).
- **Memory:** `INFO memory` summary + sampling of large keys (`MEMORY USAGE` / SCAN sample); not a full RedisInsight offline RDB analyzer in this milestone unless trivial.
- **Slowlog:** table from `SLOWLOG GET`; refresh; clear only with confirm if exposed.

## 6. E3 product behavior

### 6.1 Wizard form

Steps:

1. Topology: Standalone / Cluster / Sentinel.
2. Endpoints: host:port or node list; Sentinel master name + sentinel list; auth / ACL user.
3. TLS / mTLS paths and skip-verify.
4. Name / group / color (existing chrome).

Test connection must exercise the selected topology + TLS.

### 6.2 Runtime

- **Standalone:** current multiplexed client + `SELECT`.
- **Cluster:** redis-rs (or equivalent) cluster client; honor `clusterRouting`:
  - `auto`: follow `MOVED`/`ASK`; UI mostly transparent; Monitor may list nodes.
  - `pinnedNode`: all ops target selected node until user changes picker.
- **Sentinel:** resolve master at connect; on failure, rediscover + reconnect + toast.

SSH tunnel (existing) remains available and composes with TLS when configured.

## 7. E4 product behavior

### 7.1 Pub/Sub

- Dedicated tab: subscribe to channels and/or patterns; show live messages (timestamp, channel, payload).
- Publish form: channel + message.
- Unsubscribe / disconnect subscription cleanly on tab close or connection drop.
- Cluster note: Pub/Sub semantics differ on Cluster—document and implement per Redis rules (subscribe on appropriate connection); plan specifies exact client approach.

### 7.2 RedisJSON

- When capable: detect ReJSON keys; tree view with add/edit/delete path; save via `JSON.*` commands through plugin.
- When not capable: no JSON-specific create type / editor chrome; raw type remains whatever Redis reports.

### 7.3 Stream

- Key detail: message browser (XRANGE/XREVRANGE), create/destroy groups, consumers, pending, XACK, XADD helper.
- Monitor → Stream overview: high-level lag / group counts for streams seen or sampled (exact sampling in plan).

### 7.4 Import / export

- **Primary:** `DUMP` / `RESTORE` with TTL; batch by selection or pattern; package as zip + manifest (key, ttl, db index).
- **Optional:** JSON export for human inspection (lossy for binary / exotic types).
- Import confirms overwrite policy (skip / replace).

## 8. Plugin command surface (directional)

E1 commands remain. E2–E4 add (names locked in plan), e.g.:

- Observability: `info`, `memory_sample`, `slowlog_get`, `slowlog_reset` (gated confirm)
- Topology helpers: `cluster_nodes`, `sentinel_masters` (as needed)
- Pub/Sub: subscribe lifecycle may use events/channels rather than request/response-only; plan chooses Tauri event vs long poll
- JSON: `json_get`, `json_set`, …
- Stream: `xadd`, `xrange`, `xgroup_*`, `xpending`, `xack`, …
- Import/export: `dump_keys`, `restore_keys` (or file-path dialogs via host file IPC + plugin decode)

Frontend args: **camelCase**. ACL: extend `redis:default`.

## 9. Settings (`pluginSettings.redis`)

| Key | Type | Default | Notes |
|-----|------|---------|-------|
| `allowFlush` | boolean | `false` | E1 |
| `clusterRouting` | `"auto"` \| `"pinnedNode"` | `"auto"` | E3; UI node picker when pinned |

Exposed via existing Redis SettingsSection and/or JSON Schema (both channels already supported).

## 10. Security & safety

- mTLS key material stored with connection secrets (same encryption as passwords); never log PEM contents.
- Skip-verify TLS only when user explicitly enables; warn in UI.
- Slowlog reset / mass restore / pattern export: confirmations.
- Pub/Sub: no automatic subscribe to `*` without user action.
- Plugin ACL allow-list only.

## 11. i18n

New strings under `redis.*` (and console/monitor/pubsub/stream/json/export keys) in all locale files; Beta locales may mirror English.

## 12. Testing

- Rust: topology URL/TLS builder unit tests; flush/routing gates; DUMP/RESTORE round-trip; JSON probe false path.
- Vitest: console completion helpers; Monitor parsers; wizard step validation; settings schema for `clusterRouting`.
- E2E: extend `e2e/specs/redis.ts` for Monitor tab presence, console history smoke, Pub/Sub smoke when Redis available; Cluster/Sentinel/TLS skipped unless env endpoints provided (`E2E_REDIS_CLUSTER_*` etc.).

## 13. Rollout (implementation order)

1. **E2:** console UI move + history/completion/multi-result; Monitor Info/Memory/Slowlog; plugin commands; e2e smoke.
2. **E3:** `options` on ConnectionConfig; wizard form; TLS/mTLS + Cluster + Sentinel connect path; `clusterRouting` setting; Sentinel rediscover.
3. **E4:** Pub/Sub tab; JSON probe + editor; Stream detail + overview; import/export.

Each step should leave `main`/branch green (unit tests + redis e2e skip-safe).

## 14. Success criteria

- Redis connection window has Items / Console / Monitor / Pub/Sub; Monitor has Info · Memory · Slowlog · Stream overview.
- Console supports history, static + key completion, multi-result panes.
- User can create Standalone / Cluster / Sentinel connections with mTLS fields; Cluster respects routing setting; Sentinel survives failover with rediscovery.
- Pub/Sub subscribe+publish works; JSON editor only when module present; Stream groups operable in detail; DUMP/RESTORE zip import/export works.
- Host settings still has **no** Redis-specific business branches.

## 15. Open points for the plan (non-blockers)

- Exact persistence backend for console history.
- Whether console execution migrates off host `execute_query` to `plugin:redis|exec`.
- Cluster Pub/Sub connection strategy detail.
- Zip manifest schema version for DUMP packages.
- MEMORY sampling limits and cancellation.
