# Redis Deep Ops E2–E4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Redis deep console + Monitor (E2), Standalone/Cluster/Sentinel with mTLS (E3), and Pub/Sub + RedisJSON + Stream groups + DUMP/RESTORE IO (E4) on top of E1’s `plugin:redis` + driver UI.

**Architecture:** Keep Redis-specific logic in `packages/drivers/redis` (plugin commands + `ui/*`). Host `RedisConnectionView` stays a thin tab shell. Connection topology/TLS live in opaque `ConnectionConfig.options`. Console execution goes through `plugin:redis|exec` so Cluster routing applies. Pub/Sub uses a dedicated async task + Tauri events.

**Tech Stack:** Tauri 2, `redis` crate (aio + cluster + tls features as needed), React/TS, Zustand settings, Vitest, WebdriverIO E2E.

**Spec:** `docs/superpowers/specs/2026-08-09-redis-deep-ops-e2-e4-design.md`

## Global Constraints

- Host settings must never branch on `pluginId === 'redis'`.
- Redis toggles only in `pluginSettings.redis` (`allowFlush`, `clusterRouting`).
- Frontend `pluginInvoke` args use **camelCase** (Tauri wire format; same as host `invoke`).
- Do not expand host `KeyValueDriver` mutate surface; prefer `plugin:redis|*`.
- Implement in order **E2 → E3 → E4**; each phase must leave unit tests green; redis E2E skip-safe without Cluster/Sentinel envs.
- Branch / worktree: `feat/redis-deep-ops-e2-e4` at `.worktrees/redis-deep-ops-e2-e4`.
- Regenerated files (`generated.ts`, `plugin_init.rs`) stay stubs in git; inject only at build.

## File map

| File | Responsibility |
|------|----------------|
| `packages/drivers/redis/ui/RedisConsole.tsx` | Deep console UI |
| `packages/drivers/redis/ui/consoleHistory.ts` | History load/save helpers |
| `packages/drivers/redis/ui/redisCommands.ts` | Static command catalog for completion |
| `packages/drivers/redis/ui/MonitorPanel.tsx` | Monitor tab + sub-pages |
| `packages/drivers/redis/ui/infoParse.ts` | `INFO` text → sections |
| `packages/drivers/redis/ui/PubSubPanel.tsx` | Pub/Sub tab |
| `packages/drivers/redis/ui/JsonEditor.tsx` | RedisJSON tree editor |
| `packages/drivers/redis/ui/StreamEditor.tsx` | Stream detail + groups |
| `packages/drivers/redis/ui/ImportExport.tsx` | DUMP/RESTORE zip + JSON export |
| `packages/drivers/redis/ui/ConnectionWizard.tsx` | Topology wizard form contribution |
| `packages/drivers/redis/ui/settings.tsx` | Add `clusterRouting` |
| `packages/drivers/redis/ui/meta.ts` | `connectionForm: 'redis'` (plugin form) |
| `packages/drivers/redis/src/plugin.rs` | New commands + pubsub events |
| `packages/drivers/redis/src/ops_observe.rs` | INFO / MEMORY / Slowlog helpers |
| `packages/drivers/redis/src/ops_stream.rs` | Stream / XGROUP helpers |
| `packages/drivers/redis/src/ops_json.rs` | JSON.* helpers |
| `packages/drivers/redis/src/ops_io.rs` | DUMP/RESTORE + zip manifest |
| `packages/drivers/redis/src/connect.rs` | Topology + TLS client builder |
| `packages/drivers/redis/src/redis_driver.rs` | Use `connect.rs`; Sentinel rediscover |
| `packages/driver-api/src/types.rs` | `ConnectionConfig.options` |
| `src/types/index.ts` | `options?: Record<string, unknown>` |
| `src/windows/connection/RedisConnectionView.tsx` | Tabs: Items / Console / Monitor / PubSub |
| `e2e/specs/redis.ts` | E2–E4 smoke (skip when unavailable) |
| `docs/competitive-comparison-dbx.md` | Update Redis row after phases |

## Locked decisions (spec open points)

| Topic | Decision |
|-------|----------|
| Console history store | `{appData}/redis-console-history/{connectionId}.json` via existing path-safe file IPC or small plugin command `history_load` / `history_save` (prefer host `read/write` only if path is under appData whitelist; else plugin) |
| Console execution | `plugin:redis|exec` — does **not** use host `execute_query` for Redis console (keeps Cluster routing consistent) |
| Cluster Pub/Sub | Dedicated non-cluster multiplexed connection to a chosen node (pinned node or first master) for SUBSCRIBE; document limitation in UI when `clusterRouting=auto` |
| DUMP zip manifest | `manifest.json` schemaVersion `1`: `{ dbIndex, keys: [{ key, ttlSeconds, dumpFile }] }` |
| MEMORY sample | Default sample up to **200** keys from SCAN; cancellable busy flag |

## Locked plugin commands (new / E2–E4)

Frontend args: **camelCase**. Rust params: snake_case identifiers (Tauri maps).

| Command | Args | Returns |
|---------|------|---------|
| `exec` | `connectionId`, `dbIndex`, `commands: string` (multi-line) | `{ results: ExecResult[] }` |
| `info` | `connectionId`, `section?: string`, `nodeAddr?: string` | `string` (raw INFO) |
| `memory_sample` | `connectionId`, `dbIndex`, `limit: number` | `{ samples: { key, bytes }[], truncated: bool }` |
| `slowlog_get` | `connectionId`, `count: number` | `SlowlogEntry[]` |
| `slowlog_reset` | `connectionId`, `confirm: bool` | `()` — require `confirm===true` |
| `cluster_nodes` | `connectionId` | `ClusterNode[]` |
| `modules_list` | `connectionId` | `string[]` (module names) |
| `pubsub_subscribe` | `connectionId`, `channels: string[]`, `patterns: string[]` | `subscriptionId: string` |
| `pubsub_unsubscribe` | `connectionId`, `subscriptionId` | `()` |
| `pubsub_publish` | `connectionId`, `channel`, `message` | `number` (receivers) |
| `json_get` / `json_set` / `json_del` | `connectionId`, `dbIndex`, `key`, `path`, … | structured |
| `xrange` / `xadd` / `xgroup_create` / `xgroup_destroy` / `xpending` / `xack` / `xinfo_groups` | stream args | structured |
| `dump_keys` | `connectionId`, `dbIndex`, `keys: string[]` | `{ entries: { key, ttlSeconds, dumpBase64 }[] }` |
| `restore_keys` | `connectionId`, `dbIndex`, `entries`, `replace: bool` | `{ restored, errors }` |
| `stream_overview` | `connectionId`, `dbIndex`, `limit` | overview rows |

Pub/Sub messages: emit Tauri event `redis-pubsub-message` with payload `{ connectionId, subscriptionId, channel, payload, ts }`.

`pluginSettings.redis.clusterRouting`: `"auto" | "pinnedNode"`, default `"auto"`.

`ConnectionConfig.options` Redis keys:

```ts
type RedisConnectionOptions = {
  topology?: 'standalone' | 'cluster' | 'sentinel';
  clusterNodes?: string[]; // host:port
  sentinelMasterName?: string;
  sentinelNodes?: string[];
  sentinelNodePassword?: string;
  tls?: {
    enabled?: boolean;
    caPath?: string;
    certPath?: string;
    keyPath?: string;
    keyPassphrase?: string;
    insecureSkipVerify?: boolean;
  };
  pinnedNodeAddr?: string; // session UI may also keep this in component state
};
```

---

# Phase E2 — Console + Monitor

### Task 1: Observability ops + plugin commands

**Files:**
- Create: `packages/drivers/redis/src/ops_observe.rs`
- Modify: `packages/drivers/redis/src/lib.rs`, `plugin.rs`, `permissions/default.toml`
- Test: unit tests in `ops_observe.rs`

**Interfaces:**
- Produces: `parse_info_sections(raw: &str) -> Vec<(String, Vec<(String, String)>)>`; plugin commands `info`, `memory_sample`, `slowlog_get`, `slowlog_reset`, `modules_list`

- [ ] **Step 1: Write failing test** for INFO parser:

```rust
#[test]
fn parse_info_sections_splits_headers() {
    let raw = "# Server\r\nredis_version:7.0.0\r\n# Memory\r\nused_memory:100\r\n";
    let sections = parse_info_sections(raw);
    assert_eq!(sections[0].0, "Server");
    assert_eq!(sections[0].1[0].0, "redis_version");
    assert_eq!(sections[1].0, "Memory");
}
```

- [ ] **Step 2: Run** `cargo test -p datazen-driver-redis --lib parse_info_sections_splits_headers` — expect FAIL

- [ ] **Step 3: Implement** `ops_observe.rs` + wire plugin commands; regenerate permissions via build; extend `default.toml` allow list

- [ ] **Step 4: Run tests** — expect PASS

- [ ] **Step 5: Commit** `feat(redis): INFO/MEMORY/Slowlog plugin commands`

### Task 2: `exec` command for console

**Files:**
- Modify: `packages/drivers/redis/src/plugin.rs`, `ops.rs` or new `ops_exec.rs`
- Test: unit test splitting multi-line commands (no live Redis required for splitter)

**Interfaces:**
- Produces: `exec(connectionId, dbIndex, commands) -> { results: [{ command, ok, value?, error? }] }`
- Consumes: existing `with_conn` / shared driver

- [ ] **Step 1: Failing test** for command line splitter (skip empty / comments starting with `#` optional—keep simple: split on newlines, trim, drop empties)

```rust
#[test]
fn split_redis_commands_basic() {
    let lines = split_redis_commands("GET a\n\nSET b 1\n");
    assert_eq!(lines, vec!["GET a", "SET b 1"]);
}
```

- [ ] **Step 2–4: TDD implement splitter + `exec` plugin command** (run each line via `redis::cmd` from argv split; return stringified values)

- [ ] **Step 5: Commit** `feat(redis): exec plugin command for console`

### Task 3: Deep console UI

**Files:**
- Create: `packages/drivers/redis/ui/RedisConsole.tsx`, `consoleHistory.ts`, `redisCommands.ts`
- Modify: `src/windows/connection/RedisConnectionView.tsx` (replace `RedisQueryPanel` with driver console)
- Test: `src/windows/connection/__tests__/redisConsole.test.ts` (history + completion helpers)

**Interfaces:**
- Consumes: `pluginInvoke('redis', 'exec', { connectionId, dbIndex, commands })`
- Produces: UI with history ↑↓, completion from `REDIS_COMMANDS` + key cache prop, multi-result panes

- [ ] **Step 1: Failing Vitest** for `filterCompletions(prefix, commands, keys)`

```ts
import { filterCompletions } from '../../../../packages/drivers/redis/ui/redisCommands';

it('prefers commands then keys', () => {
  const out = filterCompletions('GE', ['GET', 'SET'], ['user:1', 'gear']);
  expect(out).toEqual(['GET', 'gear']);
});
```

- [ ] **Step 2–4: Implement catalog + console UI**; persist history JSON under appData via existing file commands **only if** path helpers already whitelist appData—otherwise add `history_get`/`history_set` plugin commands storing beside driver state in `{appData}/redis-console-history/`

- [ ] **Step 5: Commit** `feat(redis): deep console with history and completion`

### Task 4: Monitor panel UI

**Files:**
- Create: `packages/drivers/redis/ui/MonitorPanel.tsx`, `infoParse.ts` (TS mirror or reuse JSON from plugin)
- Modify: `RedisConnectionView.tsx` tabs
- Test: `infoParse.test.ts` for section parsing
- Locales: `redis.monitor`, `redis.info`, `redis.memory`, `redis.slowlog`, …

- [ ] **Step 1: Failing Vitest** for TS `parseInfoSections`

- [ ] **Step 2–4: Implement Monitor** with sub-nav Info / Memory / Slowlog; wire plugin commands; Slowlog reset uses confirm dialog

- [ ] **Step 5: Commit** `feat(redis): Monitor tab with Info Memory Slowlog`

### Task 5: E2 e2e + competitive note

**Files:**
- Modify: `e2e/specs/redis.ts`, `docs/competitive-comparison-dbx.md`
- Spec status: mark E2 subsection done in plan progress only; keep joint spec Status until all phases done

- [ ] **Step 1: Add RD-022+** — Monitor tab visible; console executes `PING`; skip-safe

- [ ] **Step 2: Run** `pnpm e2e:skip-build -- --spec e2e/specs/redis.ts` when webdriver binary exists (else document)

- [ ] **Step 3: Commit** `test(e2e): Redis E2 console and monitor smoke`

---

# Phase E3 — Topology + mTLS

### Task 6: `ConnectionConfig.options` round-trip

**Files:**
- Modify: `packages/driver-api/src/types.rs`, `src/types/index.ts`, store persistence if it strips unknown fields
- Test: Rust + TS round-trip

```rust
#[test]
fn connection_options_roundtrip() {
    let mut opts = serde_json::Map::new();
    opts.insert("topology".into(), json!("cluster"));
    let c = ConnectionConfig { options: Some(opts), ..dummy() };
    let v = serde_json::to_value(&c).unwrap();
    assert_eq!(v["options"]["topology"], "cluster");
}
```

- [ ] **Steps 1–5: TDD add `options: Option<Map<String, Value>>` / `options?: Record<string, unknown>` with `#[serde(default)]`; commit** `feat(config): opaque connection options bag`

### Task 7: Connect builder (TLS / Cluster / Sentinel)

**Files:**
- Create: `packages/drivers/redis/src/connect.rs`
- Modify: `Cargo.toml` redis features (`tokio-comp`, `aio`, `cluster-async`, `tls-rustls` or project-standard TLS stack—match existing workspace crates)
- Modify: `redis_driver.rs` connect/test paths
- Test: unit tests for URL/options parsing without network

**Interfaces:**
- Produces: `build_connection_plan(config: &ConnectionConfig) -> Result<ConnectionPlan, DriverError>`
- `ConnectionPlan` enum: Standalone / Cluster / Sentinel with TLS params

- [ ] **Step 1: Failing tests** for reading `options.topology` defaulting to standalone; TLS enabled when `options.tls.enabled` or sslMode requires encryption

- [ ] **Step 2–4: Implement connect**; Sentinel: resolve master then connect; store sentinel endpoints for rediscover on IO errors in `with_conn`

- [ ] **Step 5: Commit** `feat(redis): Cluster Sentinel and mTLS connect paths`

### Task 8: Wizard form + clusterRouting setting

**Files:**
- Create: `packages/drivers/redis/ui/ConnectionWizard.tsx`
- Modify: `meta.ts` → `connectionForm: 'redis'`; register form in `drivers-registry` / resolve-drivers `PLUGIN_FORMS` (follow Kiwi pattern)
- Modify: `settings.tsx` schema + Section for `clusterRouting`
- Modify: Workbench/Console to show node picker when `clusterRouting==='pinnedNode'`
- Test: wizard validation vitest; settings schema test

- [ ] **Steps 1–5: TDD validation** (sentinel requires master name + ≥1 node); commit `feat(redis): topology wizard and clusterRouting setting`

### Task 9: E3 e2e env hooks

**Files:**
- Modify: `e2e/.env.example`, `e2e/specs/redis.ts` (skip unless `E2E_REDIS_CLUSTER_NODES` / `E2E_REDIS_SENTINEL_*` set)

- [ ] **Steps 1–3: Add skip-gated cases**; commit `test(e2e): optional Cluster/Sentinel Redis cases`

---

# Phase E4 — Pub/Sub, JSON, Stream, IO

### Task 10: Pub/Sub backend + UI

**Files:**
- Modify: `plugin.rs` (subscribe task map `DashMap<subscriptionId, JoinHandle>`)
- Create: `PubSubPanel.tsx`
- Modify: `RedisConnectionView.tsx` tab
- Test: publish/subscribe unit with `redis` testcontainer **or** skip if unavailable; at minimum unsubscribe cleans map

- [ ] **Steps 1–5:** implement event emission + UI; commit `feat(redis): Pub/Sub tab with subscribe and publish`

### Task 11: RedisJSON probe + editor

**Files:**
- Create: `ops_json.rs`, `JsonEditor.tsx`
- Modify: `KeyEditors.tsx` / workbench create-type list to include `ReJSON` only when `modules_list` contains `ReJSON` / `RedisJSON`
- Test: probe parsing; hide path when modules empty

- [ ] **Steps 1–5:** commit `feat(redis): RedisJSON tree editor with module probe`

### Task 12: Stream detail + Monitor overview

**Files:**
- Create: `ops_stream.rs`, `StreamEditor.tsx`
- Modify: `KeyEditors.tsx` replace read-only stream JSON with `StreamEditor`
- Modify: `MonitorPanel.tsx` add Stream overview sub-page calling `stream_overview`
- Test: xgroup name validation unit test

- [ ] **Steps 1–5:** commit `feat(redis): Stream groups UI and monitor overview`

### Task 13: DUMP/RESTORE import-export

**Files:**
- Create: `ops_io.rs`, `ImportExport.tsx`
- Use host file dialogs (`dialog` plugin) for zip path; plugin does encode/decode
- Test: manifest serde round-trip; restore replace flag

Manifest v1:

```json
{
  "schemaVersion": 1,
  "dbIndex": 0,
  "keys": [{ "key": "a", "ttlSeconds": -1, "dumpFile": "a.bin" }]
}
```

- [ ] **Steps 1–5:** commit `feat(redis): DUMP/RESTORE zip import and export`

### Task 14: E4 e2e + docs + spec Implemented

**Files:**
- Modify: `e2e/specs/redis.ts`, `docs/competitive-comparison-dbx.md`
- Modify: spec Status → `Implemented` when all phases done

- [ ] **Step 1: E2E** Pub/Sub smoke (SUBSCRIBE/PUBLISH via UI) when Redis up; JSON/Stream skip if module/type absent

- [ ] **Step 2: Update competitive row** for E2–E4 complete

- [ ] **Step 3: Spec status Implemented**; commit `docs: mark Redis E2–E4 implemented; update competitive matrix`

---

## Spec coverage checklist

| Spec requirement | Task(s) |
|------------------|---------|
| Deep console history/completion/multi-result | 2, 3 |
| Monitor Info/Memory/Slowlog | 1, 4 |
| Monitor Stream overview | 12 |
| Wizard topology + mTLS | 6, 7, 8 |
| clusterRouting auto/pinned | 8 |
| Sentinel rediscover | 7 |
| Pub/Sub tab | 10 |
| RedisJSON + probe | 11 |
| Stream detail groups | 12 |
| DUMP/RESTORE + JSON export | 13 |
| E2E / competitive / status | 5, 9, 14 |
| camelCase pluginInvoke | all plugin UI tasks |
| No host redis settings branches | 8 (settings via contribution only) |

## Parallelism note

Within a phase, file-partitioned parallel implementers are OK (e.g. Task 3 UI ‖ Task 4 Monitor after Task 1–2 land). Do **not** start E3 until E2 Tasks 1–5 merged on the feature branch; same for E4 after E3.
