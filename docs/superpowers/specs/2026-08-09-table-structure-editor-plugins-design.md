# Table Structure Editor — Plugin Config + Capability Design

> Date: 2026-08-09  
> Status: draft for review  
> Branch context: `feat/abc-competitive-parity`  
> Related: [Schema Diff Deploy](../plans/2026-08-09-schema-diff-deploy.md), DBX table-structure docs / `tableStructureCapabilities.ts`

## 1. Problem

DataZen already has a **Table Structure Editor** (`TableStructureEditor.tsx`) and a separate **Indexes** tab (`IndexesView.tsx`). Gaps vs DBX:

- DDL and type lists are effectively **PostgreSQL-shaped** in the Host editor.
- Indexes are not planned in the same draft / SQL preview as columns.
- There is no shared **capability matrix** (per dialect / server version).
- Competitive docs incorrectly implied DataZen lacked a structure editor; the real gap is **depth and dialect coverage**, not presence.

We will close that gap without per-driver React editor components, and without forcing document/KV engines into a SQL table metaphor.

## 2. Goals (P1)

1. **Single Host shell** for create/alter table: columns + indexes in one draft, SQL preview, execute.
2. **UI is configuration-driven**: drivers export config (types, field visibility, index methods), **not** custom React structure-editor components.
3. **Backend owns truth for safety**: each SQL driver implements `structure_capabilities` (connection-aware, including server version) and `plan_structure_changes` (intent diff → dialect DDL).
4. **No Host-side capability lookup table** (unlike DBX’s `capabilityByType` map). Caps are **returned by the live driver** for the open connection; Host only renders whatever the driver reports. There is no central `match db_type { … }` registry of structure features in the app crate.
5. **All `supportsSQL` dialects can attach**: drivers that do not override the trait keep the **default unsupported** implementation (most edit actions disabled with clear UX).
6. **Version-aware capabilities**: encoded **inside the driver** (e.g. read `server_info`, then set fields on the returned struct). Same driver id, different server major/minor → different returned caps; unknown version → that driver’s conservative baseline. Host never branches on version.
7. **Opt-out for non-tabular models**: MongoDB, Redis, etc. do not enter this shell (same product split as DBX specialized workspaces).

## 3. Non-goals (P1)

- Foreign-key create/edit in the structure shell (FK remains read-only in existing view).
- MongoDB collection / validator / GridFS editor (separate epic if needed).
- Per-driver custom structure UI components.
- Field lineage, object browser parity with DBX.
- Online schema change (pt-osc / gh-ost).
- Replacing Schema Diff Deploy (cross-connection sync remains a separate path).

## 4. Product rules (locked)

| Decision | Choice |
|----------|--------|
| Dialect coverage ambition | All `supportsSQL` registry entries can mount; depth varies by adapter |
| UI strategy | Config-only; one Host shell |
| SQL generation | Rust driver trait (not Host `if dialect`) |
| Indexes | Same screen / same draft as columns in P1 |
| FK | Read-only in P1 |
| MongoDB / Redis / document / KV | Opt-out of structure shell |
| Version differences | Resolved **inside the driver** via connection `server_info`; frontend never parses version strings |
| Unknown / unsupported | Driver default trait impl = all-false / empty plan (fail closed) |
| Caps source of truth | **Driver method return value only** — no parallel Host table to keep in sync |

## 5. Architecture

### 5.1 Layers

```
Host TableStructureEditor (columns + indexes draft)
  - UI from StructureEditorUiConfig (driver meta)
  - enable/disable from StructureCapabilities (IPC)
  - preview/execute via plan_table_structure_changes
           |
     +-----+-----+
     |           |
 Frontend     Backend DatabaseDriver
 structureEditor  structure_capabilities(handle)  // version-aware
 config only      plan_structure_changes(diff)    // dialect DDL
```

### 5.2 Frontend config (plugin, no components)

Drivers contribute via existing UI meta / generated merge (same path as `DatabaseTypeMeta`), e.g.:

```ts
structureEditor?: {
  /** If omitted/false and !supportsSQL → Host hides entry points */
  enabled?: boolean;
  columnTypes: { value: string; label: string }[];
  defaultColumnType: string;
  fields: {
    comment?: boolean;
    charset?: boolean;
    collation?: boolean;
    unsigned?: boolean;
    length?: boolean; // show length/precision editors when type needs it
  };
  indexMethods: string[]; // e.g. ['btree','hash'] — may be narrowed by runtime caps
};
```

- Host never imports driver-specific editor components for this feature.
- Redis/Mongo omit `structureEditor` or set `enabled: false`; Host hides「编辑表结构 / 新建表」for those connection modes (or shows a short redirect to the specialized view).

### 5.3 Backend capability + plan (plugin)

Extend `packages/driver-api` `DatabaseDriver` with default methods that return unsupported / empty plan:

```rust
fn structure_capabilities(
    &self,
    handle: &ConnectionHandle,
) -> impl Future<Output = Result<StructureCapabilities, DriverError>> + Send;

fn plan_structure_changes(
    &self,
    handle: &ConnectionHandle,
    request: &StructureChangeRequest,
) -> impl Future<Output = Result<StructureChangePlan, DriverError>> + Send;
```

**`StructureCapabilities`** (names illustrative; serde `camelCase` for IPC):

- Column: `createTable`, `addColumn`, `dropColumn`, `renameColumn`, `alterType`, `alterNullability`, `alterDefault`, `alterPrimaryKey`, `reorderColumn`, `comment`
- Index: `createIndex`, `dropIndex`, `rebuildIndex`, `indexType`, `indexInclude`, `indexFilter`, `indexComment`
- Meta: `alterStrategy` (`none` | `direct` | `sqlite_rebuild`), `dialectId` (for diagnostics)
- Optional overlays returned with caps: `indexMethods: Vec<String>` (version-filtered)

**Version handling (required, driver-local only):**

1. Inside the driver impl, read server version from the live connection (reuse / extend `ServerInfo` or equivalent).
2. Build the `StructureCapabilities` value in that driver (baseline + version patches, e.g. PostgreSQL ≥ 11 → `index_include = true`).
3. If version cannot be parsed → that driver’s conservative baseline.
4. Host/frontend **must not** maintain a second map of type→caps and **must not** parse version strings; they only apply the IPC payload.

**Anti-pattern (explicitly forbidden):** a Host module like `structure_capabilities_by_db_type: HashMap<DatabaseType, Caps>` that duplicates driver knowledge. DBX uses that style in the desktop app; DataZen does **not**.

**`StructureChangeRequest`** is an intent diff (stable column ids, original vs current columns/indexes, create vs alter mode, table name). Drivers reject intents that violate caps with a clear validation error (do not silently emit wrong SQL).

**`StructureChangePlan`**: ordered statements `{ sql, summary, risk }` for preview + execute (reuse risk vocabulary from schema-diff where practical: additive / destructive / rewrite).

### 5.4 Host shell behavior

1. Load table schema + `get_structure_capabilities` + UI config.
2. Edit local draft (columns + indexes).
3. Disable controls when corresponding cap is false; show short reason via i18n.
4. Preview calls `plan_table_structure_changes`; show SQL list.
5. Execute statements (transaction policy: prefer driver/dialect honesty — PG may wrap; MySQL DDL auto-commit → do not claim full rollback). Align messaging with Schema Diff Deploy atomicity notes.
6. On success, refresh schema cache / tree.

Column reorder:

- If `reorderColumn` true → plan must emit physical reorder DDL when order changed.
- If false → UI may still allow local drag for display comfort **only if** we clearly do not persist order (P1 recommendation: **disable drag** when `reorderColumn` is false to avoid false expectations).

### 5.5 Relation to IndexesView / Schema Diff

| Feature | Role after P1 |
|---------|----------------|
| Structure shell | Primary path for column + index draft DDL on one table |
| `IndexesView` | Can remain as convenience tab or thin wrapper; must not diverge SQL generation (call same plan API or become read-only list + “Edit in structure”) |
| Schema Diff Deploy | Cross-connection desired→target deploy; structure editor is single-connection authoring |

### 5.6 DBX contrast (reference only)

| Topic | DBX | DataZen (this design) |
|-------|-----|------------------------|
| Caps storage | Desktop `capabilityByType` map + helpers | **No Host map** — `DatabaseDriver::structure_capabilities(handle)` |
| Version forks | Host helper adjusts caps (e.g. PG major) | Driver impl adjusts returned struct |
| MongoDB | Specialized workspace, not table editor | Same product split (opt-out) |
| DDL generation | Desktop dialect SQL helpers + caps | Driver `plan_structure_changes` |

## 6. IPC (snake_case args from frontend invoke keys per project convention / existing camelCase bridge)

Proposed commands (names finalizable in plan):

- `get_structure_capabilities(connection_id)` → `StructureCapabilities`
- `plan_table_structure_changes(connection_id, request)` → `StructureChangePlan`

Execution may reuse `execute_query` / `execute` for the planned SQL batch; do not invent a second executor unless transaction wrapping needs a dedicated helper (can share patterns with schema-diff deploy).

## 7. Adapter rollout

| Tier | Drivers | Expectation |
|------|---------|-------------|
| T0 | postgres, mysql, sqlite | Full P1 column + index plan quality |
| T1 | sqlserver, clickhouse, duckdb, … | Caps + plan for supported subset; rest disabled |
| T2 | Wire-reuse types (questdb, doris, …) | Explicit narrowed caps (do not blindly copy parent dialect) |
| Opt-out | mongodb, redis, … | No structure shell entry |

Shipping P1 code may land T0 complete + default unsupported for others in the same PR series; T1/T2 fill incrementally without Host changes.

## 8. Testing

- Unit (in driver crates / datazen tests calling driver fakes): version patches live **in the postgres driver** (e.g. PG 10 vs 14 `index_include`); plan SQL snapshots for PG/MySQL/SQLite add/drop/rename/index.
- Guardrail: Host crate must not grow a `capabilityByType`-style structure-caps table (code review / optional `rg` check in plan).
- Unit: Host helper `isControlEnabled(caps, 'renameColumn')`.
- Integration / e2e (existing `e2e/specs/table-structure.ts`): preview + save still work on PG; MySQL path when available.
- Regression: Mongo/Redis connections do not offer misleading「编辑表结构」that opens empty SQL shell.

## 9. Docs / i18n

- User-facing: short note in docs / 使用说明 (structure editor chapter or connection guide): preview DDL, dialect limits, version-based disabling.
- Competitive comparison: structure editor = present; gap = dialect depth / indexes-same-draft / versioned caps (already partially corrected).

## 10. Open items deferred to implementation plan

- Exact serde field names and whether caps IPC is synced on every editor open vs cached per connection id.
- Whether execute uses one multi-statement call or statement-by-statement with progress.
- PROTOCOL_VERSION bump policy when extending `driver-api` (must bump and sync plugins).

## 11. Success criteria

- PG / MySQL / SQLite: create table + alter columns + create/drop index in one preview/execute flow.
- Non-T0 SQL drivers: open editor without crash; unsupported actions disabled.
- MongoDB: no SQL table-structure editor entry (or explicit non-applicable message).
- No new per-driver structure editor React components introduced.
