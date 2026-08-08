# Design: Hierarchical SQL Identifier Autocomplete

**Date:** 2026-08-08  
**Branch:** `fix/schema-autocomplete-kiwi-superset`  
**Status:** Approved (revised)  
**Scope:** All SQL drivers that expose hierarchical names (Superset, PostgreSQL, MySQL/MariaDB, Kiwi, OLAP if present); not Redis / non-SQL views.

## Goals

1. Autocomplete **every level** of a dotted SQL identifier up to the table (not columns).
2. **Superset** hierarchy: `database.catalog.schema.table` (database = connection sidebar DB display name, e.g. `presto_afi_data`, never the numeric id in SQL).
3. **Other SQL drivers** use the same nested completion machinery with driver-appropriate depth:
   - MySQL / MariaDB / Kiwi (multi-db): `database.table`
   - PostgreSQL (and similar): `database.schema.table` when multi-db; `schema.table` when locked to one database
4. **Hybrid load policy**: show already-loaded siblings at the current level; when the user types a trailing `.` and the next level is missing, fetch on demand.
5. Column completion stays on the existing flat `columnMap` path (no `….table.column` dotted completion in v1).

## Non-goals

- Prefetching an entire engine’s metadata tree on connect.
- Dotted column completion.
- Changing query execution / `use_database` semantics beyond what completion fetches need.
- Redis / key-value “table” completion changes.

## Decisions

| Topic | Choice |
|-------|--------|
| Load policy | Hybrid: sidebar / prior ensures feed the tree; typed `.` triggers `ensureNamespacePath` |
| Column after table | Not in v1; columns via `columnMap` |
| Approach | Nested CodeMirror `SQLNamespace` + shared `schemaStore` namespace (Approach B, generalized) |
| Superset SQL shape | `databaseName.catalog.schema.table` (display name, not numeric `dbId`) |
| Superset fetch paths | `{dbId}`, `{dbId}/{catalog}`, `{dbId}/{catalog}/{schema}` via existing `get_tables` |
| Name → id map | Store `dbName → dbId` (and reverse) when listing Superset databases |
| Other drivers | Same `namespaceTree` + ensure; fetch via `get_databases` / `get_tables` / table `schema` field |
| Editor schema | All SQL connection views pass nested `SQLNamespace` (flat table→cols remains valid subset / merged at leaves when columns known) |

## Hierarchy by driver

| Driver | Namespace shape | Ensure / fetch |
|--------|-----------------|----------------|
| Superset | `db → catalog → schema → table → []` | `get_tables` with Superset path keys; resolve `db` via name→id map |
| MySQL / MariaDB / Kiwi | `database → table → []` (or cols if loaded) | `get_databases`; `get_tables(conn, database)` |
| PostgreSQL | multi-db: `database → schema → table → []`; single-db lock: `schema → table → []` | `use_database` + `get_tables`; group by `TableInfo.schema` |
| OLAP (if plugin present) | Follow plugin tree (typically catalog/schema/table or db/schema/table); same merge API | Same as host `get_tables` contract |

Top-level completion always lists the **outermost** loaded segment (Superset databases, or MySQL databases, or PG schemas when single-db, etc.).

## Current state

- CodeMirror `@codemirror/lang-sql` walks dotted parents against nested `SQLNamespace`.
- `QueryPanel` builds a **flat** `{ [tableName]: cols[] }` only.
- Multi-db trees (`MultiDatabaseSchemaTree`) and Superset custom tree keep local caches; store sync for flat tables is partial.
- PostgreSQL `TableInfo.schema` already carries schema names but is unused for nested CM completion.

## Architecture

```
Schema trees (standard / multi-db / Superset / plugins)
        │ merge on expand / load
        ▼
 schemaStore.namespaceTree + loadedPaths + (supersetDbIds)
        ▲
        │ ensureNamespacePath(segments)
 QueryPanel / SqlEditor (path prefix watcher)
        │
        ▼
 editorSchema: SQLNamespace  →  CodeMirror sql({ schema })
```

### Store units

1. **`namespaceTree: SQLNamespace`**  
   Recursive object. Table leaves are `string[]` (columns) or `[]` until `columnMap` is known.

2. **`loadedPaths: Set<string>`**  
   Keys like `db`, `db/catalog`, `db/catalog/schema` (Superset) or `mydb`, `mydb`+tables (MySQL). Prevents refetch loops on empty children.

3. **`supersetDbIds: Record<string, string>`**  
   Display name → numeric id (and optional id→name). Cleared on connection reset.

4. **`mergeNamespacePath(segments, kind, names | tablesWithCols)`**  
   Immutable merge. `kind: 'branch' | 'tables'`:
   - `branch`: children become empty objects `{}` (further nestable).
   - `tables`: children become `[]` or column arrays.

5. **`ensureNamespacePath(segments, databaseType)`**  
   Driver strategy (small switch or registry helper):
   - **Superset**
     - `[]` → ensure databases: `get_databases`, merge DB display names as branches; fill `supersetDbIds`.
     - `[db]` → `get_tables(dbId)` → catalogs (or schemas if catalog-less).
     - `[db, cat]` → schemas.
     - `[db, cat, sch]` → tables as leaves.
   - **MySQL family / Kiwi**
     - `[]` → `get_databases` → DB branches (if not already in `databases`).
     - `[db]` → `get_tables(conn, db)` → table leaves.
   - **PostgreSQL**
     - multi-db: `[]` → databases; `[db]` → load tables, group by `schema` into branches then tables.
     - single-db: `[]` → schemas from current tables or fetch; `[schema]` → tables under that schema.
   - In-flight dedupe by path key + `connectionId`.

6. **Tree sync**  
   Every schema tree that loads a level calls the same merge helpers (SDK: `syncSchemaNamespace`). Superset also registers db name/id on DB list load and when expanding a DB.

7. **QueryPanel**  
   Always build `editorSchema` from `namespaceTree`, and **overlay** `columnMap` onto table leaves when present (so unqualified / same-level column completion still works).  
   Path watcher: parse dotted parents at cursor → `ensureNamespacePath` (debounce 100–150ms) → store update → CM reconfigure; optional `startCompletion` refresh.

8. **`SqlSchema` type**  
   Alias to CodeMirror `SQLNamespace` (recursive).

## Data flow examples

### Superset

1. Connection opens / tree loads DB list → merge top-level `presto_afi_data`, … + id map.  
2. `FROM p` → completes `presto_afi_data`.  
3. `presto_afi_data.` → ensure catalogs → `hive`, …  
4. `presto_afi_data.hive.` → ensure schemas → `snap`, …  
5. `presto_afi_data.hive.snap.` → ensure tables → table names.  
6. Sidebar expand of the same paths writes into the same tree (no double semantic).

### MySQL / Kiwi

1. Multi-db session lists `app`, `other` → top-level completion.  
2. `app.` → ensure tables under `app`.

### PostgreSQL

1. Single DB: top-level `public`, `analytics`, … then `public.` → tables.  
2. Multi-db: `warehouse.` → schemas → `warehouse.public.` → tables.

## Error handling

- Failed ensure: do not mark path loaded; do not clear siblings; no modal.
- Unknown Superset DB name (no id map): skip fetch until databases ensured.
- Empty children: mark path loaded so ensure does not spin.

## Testing

- Unit: `mergeNamespacePath`, `loadedPaths`, Superset name↔id.  
- Unit: `ensureNamespacePath` per driver strategy (mocked `get_databases` / `get_tables`).  
- Unit: path prefix parser (`a.b.c.` → `['a','b','c']`).  
- Unit: QueryPanel/schema builder overlays `columnMap` onto nested leaves.  
- Manual smoke: Superset four-level + MySQL `db.table` + PG `schema.table`.

## Implementation notes

- Reuse existing IPC only.  
- Superset plugin: sync all levels (database list, catalog, schema, tables); bump registry ref.  
- Host: `schemaStore`, SDK sync helpers, `QueryPanel`, `SqlEditor` types, path helper, MultiDatabaseSchemaTree / StandardSchemaTree merges (group PG by schema).  
- Preserve flat `tables` / `views` / `currentDatabase` for Structure panels and `columnMap` loading.

## Success criteria

- Superset: top-level databases; each of `db.`, `db.cat.`, `db.cat.sch.` completes the next level (cache hit or one fetch).  
- MySQL/Kiwi: `database.` completes tables.  
- PostgreSQL: schema (and database when multi-db) levels complete.  
- No dotted column completion; `columnMap` still powers column suggestions for loaded tables.
