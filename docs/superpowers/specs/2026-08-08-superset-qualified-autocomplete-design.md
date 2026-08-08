# Design: Superset Qualified Name Autocomplete (`catalog.schema.table`)

**Date:** 2026-08-08  
**Branch:** `fix/schema-autocomplete-kiwi-superset`  
**Status:** Approved  
**Scope:** Superset only (v1)

## Goals

1. Support Hive / Presto-style qualified identifiers in the SQL editor: `hive.snap.some_table`.
2. Provide autocomplete at **catalog**, **schema**, and **table** levels.
3. Use a **hybrid load policy**: surface already-loaded catalogs at the top level; when the user types `catalog.` or `catalog.schema.`, fetch the next level on demand if missing.
4. Keep column completion on the existing flat `columnMap` path (no `table.column` dotted completion in v1).

## Non-goals

- Prefetching the full catalog → schema → table tree for a database on connect.
- Dotted column completion (`hive.snap.table.col`).
- Changing Kiwi / MySQL / PostgreSQL completion models (they stay flat table→columns).
- Changing Superset SQL execution / path parsing semantics beyond what completion needs.

## Decisions (approved)

| Topic | Choice |
|-------|--------|
| Load policy | Hybrid (option 3): use sidebar cache; lazy-fetch next segment on typed `.` |
| Column after table | Not in v1 (option 2); columns via existing `columnMap` |
| Approach | Nested CodeMirror `SQLNamespace` + `schemaStore` path ensure (Approach B) |
| SQL shape | `catalog.schema.table` (not `dbId.catalog.schema.table`) |
| Internal fetch path | Existing Superset `get_tables` paths: `{dbId}`, `{dbId}/{catalog}`, `{dbId}/{catalog}/{schema}` |

## Current state

- CodeMirror `@codemirror/lang-sql` already walks dotted parents against a nested `SQLNamespace`.
- `QueryPanel` builds a **flat** `SqlSchema`: `{ [tableName]: string[] }` from `schemaStore.tables` + `columnMap`.
- Superset schema tree uses path keys `dbId/catalog/schema` and recently syncs **leaf tables only** via `syncSchemaTables`.
- Superset driver `parse_database_path` maps those path keys to API calls; SQL identifiers use catalog/schema/table names without the numeric `dbId`.

## Architecture

```
SupersetSchemaTree                    SqlEditor / QueryPanel
  expand catalog/schema ──┐                 │
                          ▼                 │ typed "hive." / "hive.snap."
                   schemaStore              │
              ┌─ namespaceTree (nested) ◄────┘ ensureNamespacePath
              ├─ activeDbId
              ├─ tables / views / columnMap (unchanged flat path)
              └─ mergeNamespacePath / ensureNamespacePath
                          │
                          ▼
                   editorSchema (SQLNamespace)
                          │
                          ▼
                   CodeMirror sql({ schema })
```

### Units

1. **`namespaceTree` in `schemaStore`**  
   Nested map: `catalog → schema → table → []` (empty column arrays at leaves).  
   Also stores `activeDbId: string | null` for Superset fetch prefixes.

2. **`mergeNamespacePath(segments, children)`**  
   Pure merge helper: given `['hive']` + schema name list, or `['hive','snap']` + table name list, update the tree immutably. Tables land as `{ [name]: [] }`.

3. **`ensureNamespacePath(segments)`**  
   - Requires `connectionId` + `activeDbId`.  
   - `[]` → no-op (catalogs come from sidebar / explicit merge).  
   - `['hive']` → if schemas under `hive` missing, `get_tables(`${activeDbId}/hive`)`, merge schema names.  
   - `['hive','snap']` → if tables missing, `get_tables(`${activeDbId}/hive/snap`)`, merge table names as empty leaves.  
   - Deduplicate in-flight requests by path key.  
   - Ignore navigational sentinel rows (`schema === 'CATALOG'|'SCHEMA'`) when merging table-level results; treat them as children names at the correct level when merging from catalog/schema listing responses (same as the tree already does).

4. **Superset tree sync**  
   On successful load of catalogs / schemas / tables, call merge helpers (and set `activeDbId` when a database node is activated). Prefer SDK helper e.g. `syncSchemaNamespace(...)` so the plugin does not depend on store internals beyond the SDK.

5. **QueryPanel → editor**  
   - For `databaseType === 'superset'`, pass `namespaceTree` (plus optional flat fallback of current schema tables at top level only if needed — **default: nested only for Superset**).  
   - Keep using flat `tables`+`columnMap` for non-Superset.  
   - Watch editor path prefix (see below) and call `ensureNamespacePath`.

6. **Path detection (editor side)**  
   Lightweight helper: from cursor, read the dotted identifier path (unquoted / simple identifiers).  
   - Parents `['hive']` → ensure schemas  
   - Parents `['hive','snap']` → ensure tables  
   Debounce (~100–150ms). After load, schema prop update reconfigures CM (existing compartment pattern). Optionally re-trigger completion once (`startCompletion`) if the user is still at the same position.

### `SqlSchema` type

Widen `SqlSchema` to CodeMirror’s `SQLNamespace` (or a compatible recursive type) so nested objects are valid. Flat `Record<string, string[]>` remains a valid subset for non-Superset drivers.

## Data flow examples

1. User expands Superset DB `558` → tree loads catalogs → store merges top-level keys `hive`, … and sets `activeDbId = '558'`.  
2. User types `FROM h` → CM suggests `hive` from namespace.  
3. User types `hive.` → `ensureNamespacePath(['hive'])` fetches schemas → merge `snap`, … → CM suggests `snap`.  
4. User types `hive.snap.` → ensure tables → merge table leaves → CM suggests table names.  
5. User opens a table in the tree → existing `columnMap` load still feeds unqualified column completion for that active table set.

## Error handling

- Fetch failure: leave that path unloaded; do not wipe siblings. Surface nothing modal-blocking in the editor (optional `tracing`/console only).
- Missing `activeDbId`: skip ensure (no request).
- Empty catalog/schema lists: merge empty children markers so ensure does not refetch forever (track loaded paths in a `Set`).

## Testing

- Unit: `mergeNamespacePath` / loaded-path set behavior.  
- Unit: `ensureNamespacePath` calls correct `get_tables` arguments and dedupes in-flight.  
- Unit: path prefix parser (`hive.snap.` → `['hive','snap']`).  
- Component/store: Superset sync merge does not break flat `setLoadedTables` for column map.  
- No E2E required for v1 if unit coverage is solid (manual smoke: type `hive.snap.` against a live Superset connection).

## Implementation notes

- Reuse `databaseCommands.getTables` — no new IPC.  
- Plugin change lives in `datazen-driver-superset` (sync on expand); bump `plugins-registry.json` ref after push.  
- Host changes: `schemaStore`, `plugin-sdk`, `QueryPanel`, `SqlEditor` types, small path helper module.  
- Keep prior flat sync for “current schema tables” so Structure/table panels and `columnMap` keep working.

## Success criteria

- Typing `catalog.` completes schemas after at most one on-demand fetch (or instantly if sidebar already loaded them).  
- Typing `catalog.schema.` completes tables the same way.  
- Top-level completion lists catalogs already known to the store.  
- Column dotted completion is absent; unqualified column completion still works via `columnMap` when tables are loaded for the active schema.
