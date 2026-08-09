# Redis Deep Ops E1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Redis mutating UX (CRUD + batch + Size + gated Flush) via `plugin:redis|*` and driver UI, plus host-neutral `pluginSettings` with SettingsSection **and** JSON Schema channels — no Redis-specific host settings logic.

**Architecture:** Host stores opaque `pluginSettings[pluginId]`. Settings page discovers driver contributions (React section and/or JSON Schema). Redis crate adds optional `tauri-plugin` feature (Kiwi pattern). Mutate/batch/flush commands live in the driver; UI moves under `packages/drivers/redis/ui/`. Host keeps `kv_scan_keys` / `kv_get_key` for reads in E1.

**Tech Stack:** Tauri 2 plugin commands, Rust redis crate, React/TS, Zustand settings store, Vitest, WebdriverIO E2E.

**Spec:** `docs/superpowers/specs/2026-08-09-redis-deep-ops-e1-design.md`

## Global Constraints

- Host must never branch on `pluginId === 'redis'` in settings.
- No `redis.allowFlush` (or similar) field on `AppSettings` — only inside `pluginSettings.redis`.
- Mutate/batch/flush → `pluginInvoke('redis', …)` only; do not expand `KeyValueDriver` mutate surface.
- Stream remains read-only in E1.
- Flush: default off; UI typed confirm; backend rejects when `allowFlush !== true`.
- Branch / worktree: `feat/redis-deep-ops-e1` at `.worktrees/redis-deep-ops-e1`.
- Frontend IPC args use `snake_case` keys matching Rust.
- Prefer Section when both Section + Schema exist for a plugin; else Schema; else hide.

## File map

| File | Responsibility |
|------|----------------|
| `src/types/index.ts` | `pluginSettings` on `AppSettings` |
| `src-tauri/src/store/mod.rs` | Same field + default `{}` + serde |
| `src/stores/settingsStore.ts` | Default `pluginSettings: {}` |
| `src/plugin-sdk/settings.ts` | Types: `PluginSettingsContribution`, schema form helpers |
| `scripts/resolve-drivers.mjs` | Emit `PLUGIN_SETTINGS_ENTRIES` into `generated.ts` |
| `src/plugins/generated.ts` | Generated contributions (do not hand-edit) |
| `src/windows/settings/PluginSettingsSection.tsx` | Host Extensions UI (generic) |
| `src/windows/settings/SettingsWindow.tsx` | Mount Extensions area |
| `src/locales/en.ts` (+ zh-CN min) | `settings.extensions.*` only |
| `drivers-registry.json` | Redis `tauriPlugin` block |
| `packages/drivers/redis/Cargo.toml` | `tauri-plugin` feature + deps |
| `packages/drivers/redis/build.rs` | `tauri_plugin::Builder` |
| `packages/drivers/redis/permissions/` | `default.toml` allow list |
| `packages/drivers/redis/src/plugin.rs` | `init()` + command handlers |
| `packages/drivers/redis/src/ops.rs` | Pure Redis mutate/batch helpers (unit-tested) |
| `packages/drivers/redis/ui/RedisWorkbench.tsx` | Full Items UX (browse+edit+batch) |
| `packages/drivers/redis/ui/settings.tsx` | `RedisSettingsSection` + `redisSettingsSchema` |
| `packages/drivers/redis/ui/meta.ts` | Unchanged connection meta |
| `src/windows/connection/RedisConnectionView.tsx` | Thin re-export / mount of driver workbench |
| `src/lib/__tests__/pluginSettingsForm.test.ts` | Schema form + merge |
| `e2e/specs/redis.ts` | Extend write-path cases |

## Locked command names

| Command | Args (snake_case) | Notes |
|---------|-------------------|--------|
| `set_string` | `connection_id`, `db_index`, `key`, `value` | |
| `hash_set` | `connection_id`, `db_index`, `key`, `field`, `value` | |
| `hash_del` | `connection_id`, `db_index`, `key`, `fields: string[]` | |
| `list_push` | `connection_id`, `db_index`, `key`, `side: "left"\|"right"`, `values: string[]` | |
| `list_set` | `connection_id`, `db_index`, `key`, `index`, `value` | |
| `list_pop` | `connection_id`, `db_index`, `key`, `side` | |
| `set_add` / `set_remove` | `connection_id`, `db_index`, `key`, `members: string[]` | |
| `zset_add` | `connection_id`, `db_index`, `key`, `members: {member, score}[]` | |
| `zset_remove` | `connection_id`, `db_index`, `key`, `members: string[]` | |
| `delete_keys` | `connection_id`, `db_index`, `keys: string[]` | |
| `rename` | `connection_id`, `db_index`, `key`, `new_key` | |
| `set_ttl` | `connection_id`, `db_index`, `key`, `ttl_seconds: i64` | `-1` = persist |
| `batch_delete_pattern` | `connection_id`, `db_index`, `pattern` | SCAN+DEL; return `{ deleted, errors }` |
| `batch_set_ttl` | `connection_id`, `db_index`, `keys`, `ttl_seconds` | per-key errors ok |
| `batch_rename_prefix` | `connection_id`, `db_index`, `old_prefix`, `new_prefix`, `keys?` | if `keys` omitted, SCAN `old_prefix*` |
| `flush_db` | `connection_id`, `db_index`, `allow_flush: bool` | reject unless `allow_flush` |
| `flush_all` | `connection_id`, `allow_flush: bool` | reject unless `allow_flush` |
| `count_matching` | `connection_id`, `db_index`, `pattern` | for confirm dialogs |

Flush gate: frontend passes `allow_flush` from `pluginSettings.redis.allowFlush === true`; command **must** error if false. (E1 does not require Rust to read settings store.)

JSON Schema for redis settings (Channel B) — properties: `allowFlush` boolean default false.

---

### Task 1: Host `pluginSettings` storage

**Files:**
- Modify: `src/types/index.ts` (`AppSettings`)
- Modify: `src-tauri/src/store/mod.rs` (`AppSettings` + `Default`)
- Modify: `src/stores/settingsStore.ts` (`DEFAULT_SETTINGS`)
- Test: `src-tauri/src/store/mod.rs` unit test OR `src/lib/__tests__/pluginSettingsMerge.test.ts`

**Interfaces:**
- Produces: `AppSettings.pluginSettings: Record<string, unknown>` (TS) / `serde_json::Map` or `HashMap<String, Value>` (Rust), serde `camelCase` key `pluginSettings`, `#[serde(default)]`

- [ ] **Step 1: Write failing Rust round-trip test** in `store/mod.rs`:

```rust
#[test]
fn plugin_settings_roundtrip_opaque() {
    let settings = AppSettings {
        plugin_settings: {
            let mut m = serde_json::Map::new();
            m.insert("redis".into(), serde_json::json!({ "allowFlush": true }));
            m
        },
        ..AppSettings::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    assert!(json.contains("pluginSettings"));
    let parsed: AppSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.plugin_settings.get("redis").unwrap()["allowFlush"], true);
}
```

- [ ] **Step 2: Run test — expect FAIL** (field missing)

```bash
cargo test -p datazen --lib store::tests::plugin_settings_roundtrip_opaque
```

- [ ] **Step 3: Add field** to Rust + TS + DEFAULT_SETTINGS (`pluginSettings: {}`). Ensure old `settings.json` without the key deserializes via `default`.

- [ ] **Step 4: Run test — expect PASS**

- [ ] **Step 5: Commit**

```bash
git add src/types/index.ts src-tauri/src/store/mod.rs src/stores/settingsStore.ts
git commit -m "feat(settings): opaque pluginSettings bag on AppSettings"
```

---

### Task 2: Plugin settings discovery + generic Extensions UI

**Files:**
- Create: `src/plugin-sdk/settings.ts`
- Create: `src/windows/settings/JsonSchemaSettingsForm.tsx`
- Create: `src/windows/settings/PluginSettingsSection.tsx`
- Modify: `src/plugin-sdk/index.ts` (re-export)
- Modify: `scripts/resolve-drivers.mjs` (emit `PLUGIN_SETTINGS_ENTRIES`)
- Modify: clean stub in `scripts/plugin-deinject.mjs` `cleanGeneratedTsContent` to include empty `PLUGIN_SETTINGS_ENTRIES`
- Modify: `src/windows/settings/SettingsWindow.tsx`
- Modify: `src/locales/en.ts`, `src/locales/zh-CN.ts` (`settings.extensions.title`, `.empty`)
- Test: `src/lib/__tests__/pluginSettingsForm.test.ts`

**Interfaces:**
- Produces:
  - `export type PluginSettingsContribution = { pluginId: string; label: string; SettingsSection?: ComponentType<{ value: unknown; onChange: (next: unknown) => void }>; schema?: object }`
  - `export const PLUGIN_SETTINGS_ENTRIES: PluginSettingsContribution[]` in generated.ts
  - `mergePluginSettings(all, pluginId, next): AppSettings['pluginSettings']`
  - Prefer `SettingsSection` over `schema` when both set

- [ ] **Step 1: Failing Vitest** for `mergePluginSettings` and schema boolean coerce:

```ts
import { describe, expect, it } from 'vitest';
import { mergePluginSettings, readBooleanField } from '../../plugin-sdk/settings';

describe('plugin settings helpers', () => {
  it('merges one plugin bucket without clobbering others', () => {
    const next = mergePluginSettings(
      { kiwi: { x: 1 } },
      'redis',
      { allowFlush: true },
    );
    expect(next).toEqual({ kiwi: { x: 1 }, redis: { allowFlush: true } });
  });

  it('readBooleanField defaults', () => {
    expect(readBooleanField({}, 'allowFlush', false)).toBe(false);
    expect(readBooleanField({ allowFlush: true }, 'allowFlush', false)).toBe(true);
  });
});
```

- [ ] **Step 2: Run — expect FAIL**

```bash
npx vitest run src/lib/__tests__/pluginSettingsForm.test.ts
```

- [ ] **Step 3: Implement helpers + `JsonSchemaSettingsForm`** supporting a **minimal subset**: `type: object`, `properties` with `boolean` (+ `title`/`description`), `default`. No `$ref`.

- [ ] **Step 4: Wire `PLUGIN_SETTINGS_ENTRIES`** in `resolve-drivers.mjs` from `FRONTEND_DRIVER_CONFIG[id].settings` if present:

```js
// example config shape later used by redis:
// settings: {
//   pluginId: 'redis',
//   label: 'Redis',
//   sectionExport: 'RedisSettingsSection', // optional
//   sectionPath: '../../packages/drivers/redis/ui/settings',
//   schemaExport: 'redisSettingsSchema',   // optional
//   schemaPath: same,
// }
```

Emit imports + array into `generated.ts`. Stub clean generated must export `export const PLUGIN_SETTINGS_ENTRIES = [];` (update `cleanGeneratedTsContent`).

- [ ] **Step 5: `PluginSettingsSection`** maps `PLUGIN_SETTINGS_ENTRIES`, renders Section or Schema form, writes via `updateSettings({ pluginSettings: mergePluginSettings(...) })`. **No redis string checks.**

- [ ] **Step 6: Mount in SettingsWindow**; add i18n keys; run Vitest + `node scripts/check-managed-stubs.mjs` after restore if needed.

- [ ] **Step 7: Commit**

```bash
git commit -m "feat(settings): Extensions UI for plugin SettingsSection and JSON Schema"
```

---

### Task 3: Redis `tauriPlugin` scaffolding + ACL

**Files:**
- Modify: `drivers-registry.json` (redis `tauriPlugin`)
- Modify: `packages/drivers/redis/Cargo.toml`
- Create: `packages/drivers/redis/build.rs`
- Create: `packages/drivers/redis/permissions/default.toml` (+ allow files as kiwi)
- Create: `packages/drivers/redis/src/plugin.rs` (stub `init` + one ping command OR first real command)
- Modify: `packages/drivers/redis/src/lib.rs` (`mod plugin` under feature)

**Interfaces:**
- Registry:

```json
"tauriPlugin": {
  "id": "redis",
  "initFn": "datazen_driver_redis::init",
  "commands": ["set_string", "hash_set", "hash_del", "list_push", "list_set", "list_pop", "set_add", "set_remove", "zset_add", "zset_remove", "delete_keys", "rename", "set_ttl", "batch_delete_pattern", "batch_set_ttl", "batch_rename_prefix", "flush_db", "flush_all", "count_matching"]
}
```

- Produces: `pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R>` behind `feature = "tauri-plugin"`

- [ ] **Step 1: Mirror Kiwi layout** — copy permission/build patterns from `.plugins/kiwi` adapted to crate name `datazen-driver-redis` / plugin id `redis`.

- [ ] **Step 2: `cargo check -p datazen-driver-redis --features tauri-plugin`**

- [ ] **Step 3: Run `node scripts/resolve-drivers.mjs --drivers=basic`** — verify `plugin_init` references redis init and capabilities gain `redis:default`. Then `node scripts/plugin-file-stash.mjs restore`.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(redis): tauri-plugin scaffolding and registry ACL wiring"
```

---

### Task 4: Redis mutate ops + plugin commands (Rust)

**Files:**
- Create: `packages/drivers/redis/src/ops.rs`
- Modify: `packages/drivers/redis/src/plugin.rs` (handlers using connection manager — see below)
- Test: `ops.rs` `#[cfg(test)]` for pure helpers (prefix rename planning, ttl sentinel)

**Connection access:** Plugin commands need the live multiplexed connection. E1 approach: accept `connection_id` + `db_index`, use `tauri::State` / app handle to reach host `AppState` **only if** the driver-api already exposes a handle pattern. If redis plugin cannot import host `AppState` (crate boundary), use the same pattern as Kiwi: store session map inside the redis plugin keyed by connection id that the host driver already uses.

**Concrete E1 approach (locked):** Reuse the existing `RedisDriver` connection table inside the driver crate. Plugin commands call into `RedisDriver::with_conn(connection_id, db_index, |conn| async { ... })` helpers added next to `KeyValueDriver` impl (same process, same crate). No host `AppState` import in the plugin module.

- [ ] **Step 1: Failing unit tests** for prefix rename plan:

```rust
#[test]
fn plan_rename_prefix_rewrites() {
    let planned = plan_rename_prefix("user:", "u:", &["user:1".into(), "user:2".into()]);
    assert_eq!(planned, vec![("user:1".into(), "u:1".into()), ("user:2".into(), "u:2".into())]);
}
```

- [ ] **Step 2: Implement `ops.rs` + wire all commands** in `plugin.rs` with `#[tauri::command]` / `generate_handler!`.

- [ ] **Step 3: Flush commands**

```rust
if !allow_flush {
    return Err("Flush is disabled in Redis extension settings".into());
}
```

- [ ] **Step 4: `cargo test -p datazen-driver-redis`**

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(redis): mutate, batch, and gated flush plugin commands"
```

---

### Task 5: Redis settings contribution (Section + Schema)

**Files:**
- Create: `packages/drivers/redis/ui/settings.tsx`
- Modify: `scripts/resolve-drivers.mjs` `FRONTEND_DRIVER_CONFIG.redis.settings`
- Test: Vitest that `redisSettingsSchema.properties.allowFlush` exists

```tsx
export const redisSettingsSchema = {
  type: 'object',
  properties: {
    allowFlush: {
      type: 'boolean',
      title: 'Allow FLUSHDB / FLUSHALL',
      description: 'Dangerous. Off by default.',
      default: false,
    },
  },
} as const;

export function RedisSettingsSection({
  value,
  onChange,
}: {
  value: unknown;
  onChange: (next: unknown) => void;
}) {
  const v = (value && typeof value === 'object' ? value : {}) as { allowFlush?: boolean };
  return (
    <label className="flex items-start gap-2 text-sm">
      <input
        type="checkbox"
        checked={v.allowFlush === true}
        onChange={(e) => onChange({ ...v, allowFlush: e.target.checked })}
      />
      <span>Allow FLUSHDB / FLUSHALL</span>
    </label>
  );
}
```

- [ ] **Step 1: Add file + FRONTEND_DRIVER_CONFIG wiring**
- [ ] **Step 2: resolve-drivers basic; confirm generated imports settings**
- [ ] **Step 3: Commit**

```bash
git commit -m "feat(redis): plugin settings Section and JSON Schema for allowFlush"
```

---

### Task 6: Move workbench UI — Size column + editors + batch

**Files:**
- Create: `packages/drivers/redis/ui/RedisWorkbench.tsx` (move logic from host view)
- Create: `packages/drivers/redis/ui/KeyEditors.tsx` (string/hash/list/set/zset)
- Create: `packages/drivers/redis/ui/BatchBar.tsx`
- Modify: `src/windows/connection/RedisConnectionView.tsx` → re-export workbench
- Modify: locales `redis.*` as needed for new actions
- Test: `packages/drivers/redis/ui/__tests__/keyEditors.test.tsx` (or under `src/lib/__tests__` if vitest root excludes packages — prefer `src/windows/connection/__tests__/redisWorkbench.test.tsx` importing workbench)

**Behavior checklist:**
- Size column from `KeyEntry.size`
- Multi-select + batch delete / pattern delete / batch TTL / batch rename prefix
- Detail editors call `pluginInvoke('redis', …)`
- Create key dialog (type + name)
- Stream detail read-only
- DB list: allow selecting `db0`–`db15` even if empty (synthetic list merge with `get_databases`)
- Flush buttons only if `readBooleanField(pluginSettings.redis, 'allowFlush', false)`; typed confirm `ALL` or db index string

- [ ] **Step 1: Move browse UI; add Size column; keep reads on `databaseCommands.kvScanKeys`**
- [ ] **Step 2: Editors + create/rename/ttl**
- [ ] **Step 3: BatchBar**
- [ ] **Step 4: Flush UI gated**
- [ ] **Step 5: Vitest for editor save handlers (mock `pluginInvoke`)**
- [ ] **Step 6: Commit**

```bash
git commit -m "feat(redis): workbench UI with editors, batch ops, size, gated flush"
```

---

### Task 7: E2E + docs polish

**Files:**
- Modify: `e2e/specs/redis.ts`
- Modify: `docs/competitive-comparison-dbx.md` (Redis row: batch + console note → E1 batch landed; console still E2)
- Modify: `docs/superpowers/specs/2026-08-09-redis-deep-ops-e1-design.md` Status → Implemented (when done)

- [ ] **Step 1: Add cases** — create string key, edit, delete; batch delete two keys; assert Flush control absent by default
- [ ] **Step 2: Run** `pnpm e2e:skip-build -- --spec e2e/specs/redis.ts` if webdriver binary present; else note in PR
- [ ] **Step 3: Commit**

```bash
git commit -m "test(e2e): Redis E1 write-path and flush-default-off coverage"
```

---

## Spec coverage check

| Spec requirement | Task |
|------------------|------|
| pluginSettings opaque bag | T1 |
| Section + Schema channels, no host redis branches | T2, T5 |
| redis tauriPlugin + ACL | T3 |
| Mutate/batch/flush commands + backend flush gate | T4 |
| CRUD editors five types; stream RO | T6 |
| Batch delete/pattern/TTL/prefix rename | T6 |
| Size column | T6 |
| Flush setting + typed confirm | T5, T6 |
| Empty DB selectable | T6 |
| E2E / competitive doc | T7 |
| Keep kv_* reads | T6 |

## Placeholder scan

None intentional. Command names locked in table above.
