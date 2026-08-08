# Hierarchical SQL Autocomplete Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the SQL editor nested identifier autocomplete (`database.catalog.schema.table` for Superset; `database.table` / `schema.table` / `database.schema.table` for other SQL drivers) with hybrid sidebar sync + on-demand fetch.

**Architecture:** A shared nested `SQLNamespace` lives in `schemaStore`. Schema trees merge loaded levels into it; the editor watches dotted path prefixes and calls `ensureNamespacePath` when the next level is missing. `QueryPanel` builds the CodeMirror `schema` from `namespaceTree` with `columnMap` overlaid on table leaves. No dotted column completion.

**Tech Stack:** TypeScript, Zustand, CodeMirror `@codemirror/lang-sql` (`SQLNamespace`), Vitest, Superset plugin (`datazen-driver-superset`).

**Spec:** `docs/superpowers/specs/2026-08-08-hierarchical-sql-autocomplete-design.md`

## Global Constraints

- Column dotted completion (`….table.col`) is out of scope; columns stay on flat `columnMap`.
- Superset SQL uses **display database name**, never numeric `dbId`.
- Reuse existing IPC (`get_databases`, `get_tables`, `use_database`, `get_columns`) only.
- Preserve flat `tables` / `views` / `currentDatabase` for Structure / Table panels.
- Branch: `fix/schema-autocomplete-kiwi-superset` (already has partial flat-table sync WIP — finish it inside these tasks, do not discard).
- No prefetch of full metadata trees on connect.

## File map

| File | Responsibility |
|------|----------------|
| `src/lib/sqlNamespace.ts` | Pure types + `mergeNamespacePath`, `pathKey`, `overlayColumnMap`, `isNamespaceLoaded` |
| `src/lib/sqlPathPrefix.ts` | Parse dotted identifier parents before cursor |
| `src/lib/buildEditorSchema.ts` | Combine `namespaceTree` + flat tables fallback + `columnMap` |
| `src/lib/ensureNamespace.ts` | Driver strategies for `ensureNamespacePath` (called from store) |
| `src/stores/schemaStore.ts` | Hold `namespaceTree`, `loadedPaths`, `supersetDbIds`; expose merge/ensure/reset |
| `src/plugin-sdk/index.ts` | `syncSchemaNamespace`, keep `syncSchemaTables` |
| `src/components/SqlEditor.tsx` | `SqlSchema` = `SQLNamespace`; optional `onQualifiedPath` callback |
| `src/windows/connection/QueryPanel.tsx` | Build nested schema; debounce path → ensure |
| `src/windows/connection/schema-tree/MultiDatabaseSchemaTree.tsx` | Merge DB/table levels into namespace |
| `src/windows/connection/schema-tree/StandardSchemaTree.tsx` | Merge schema/table (PG) or table level |
| `.plugins/superset/ui/SupersetSchemaTree.tsx` | Sync DB/catalog/schema/table levels + id map |
| `plugins-registry.json` | Bump superset ref after plugin push |
| Tests under `src/lib/__tests__/`, `src/stores/__tests__/` | Unit coverage per task |

---

### Task 1: Pure namespace helpers

**Files:**
- Create: `src/lib/sqlNamespace.ts`
- Create: `src/lib/__tests__/sqlNamespace.test.ts`

**Interfaces:**
- Produces:
  - `export type SqlNamespace = { [name: string]: SqlNamespace } | readonly string[]`
  - `export type NamespaceMergeKind = 'branch' | 'tables'`
  - `export function pathKey(segments: string[]): string`
  - `export function mergeNamespacePath(tree: SqlNamespace, segments: string[], kind: NamespaceMergeKind, names: string[]): SqlNamespace`
  - `export function overlayColumnMap(tree: SqlNamespace, columnMap: Record<string, string[]>): SqlNamespace`
  - `export function namespaceHasChild(tree: SqlNamespace, segments: string[]): boolean`

- [ ] **Step 1: Write failing tests**

```ts
import { describe, expect, it } from 'vitest';
import {
  mergeNamespacePath,
  overlayColumnMap,
  pathKey,
  namespaceHasChild,
} from '../sqlNamespace';

describe('pathKey', () => {
  it('joins segments with /', () => {
    expect(pathKey([])).toBe('');
    expect(pathKey(['a', 'b'])).toBe('a/b');
  });
});

describe('mergeNamespacePath', () => {
  it('merges branch children under a path', () => {
    let tree = mergeNamespacePath({}, ['db'], 'branch', ['hive', 'iceberg']);
    expect(tree).toEqual({ db: { hive: {}, iceberg: {} } });
    tree = mergeNamespacePath(tree, ['db', 'hive'], 'branch', ['snap']);
    expect(tree).toEqual({ db: { hive: { snap: {} }, iceberg: {} } });
  });

  it('merges table leaves as empty column arrays', () => {
    const tree = mergeNamespacePath({ db: { hive: { snap: {} } } }, ['db', 'hive', 'snap'], 'tables', [
      't1',
      't2',
    ]);
    expect(tree).toEqual({ db: { hive: { snap: { t1: [], t2: [] } } } });
  });

  it('does not wipe existing siblings', () => {
    const tree = mergeNamespacePath({ a: { x: [] }, b: {} }, [], 'branch', ['a', 'c']);
    expect(tree).toEqual({ a: { x: [] }, b: {}, c: {} });
  });
});

describe('overlayColumnMap', () => {
  it('fills matching table leaves with columns', () => {
    const tree = { app: { users: [] as string[], orders: [] as string[] } };
    expect(overlayColumnMap(tree, { users: ['id', 'name'] })).toEqual({
      app: { users: ['id', 'name'], orders: [] },
    });
  });
});

describe('namespaceHasChild', () => {
  it('detects loaded branch vs missing', () => {
    const tree = { db: { hive: {} } };
    expect(namespaceHasChild(tree, ['db'])).toBe(true);
    expect(namespaceHasChild(tree, ['db', 'hive'])).toBe(true);
    expect(namespaceHasChild(tree, ['db', 'missing'])).toBe(false);
  });
});
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
pnpm exec vitest run src/lib/__tests__/sqlNamespace.test.ts
```

Expected: FAIL (module not found)

- [ ] **Step 3: Implement `src/lib/sqlNamespace.ts`**

```ts
export type SqlNamespace = { [name: string]: SqlNamespace } | readonly string[];

export type NamespaceMergeKind = 'branch' | 'tables';

export function pathKey(segments: string[]): string {
  return segments.join('/');
}

function isLeaf(node: SqlNamespace): node is readonly string[] {
  return Array.isArray(node);
}

function asBranch(node: SqlNamespace | undefined): Record<string, SqlNamespace> {
  if (!node || isLeaf(node)) return {};
  return { ...node };
}

export function mergeNamespacePath(
  tree: SqlNamespace,
  segments: string[],
  kind: NamespaceMergeKind,
  names: string[],
): SqlNamespace {
  const root = asBranch(tree);

  const setAt = (
    node: Record<string, SqlNamespace>,
    segs: string[],
  ): Record<string, SqlNamespace> => {
    if (segs.length === 0) {
      const next = { ...node };
      for (const name of names) {
        if (kind === 'tables') {
          next[name] = Array.isArray(next[name]) ? next[name] : [];
        } else if (!(name in next) || Array.isArray(next[name])) {
          next[name] = asBranch(next[name]);
        }
      }
      return next;
    }
    const [head, ...rest] = segs;
    const child = asBranch(node[head]);
    return { ...node, [head]: setAt(child, rest) };
  };

  return setAt(root, segments);
}

export function namespaceHasChild(tree: SqlNamespace, segments: string[]): boolean {
  if (segments.length === 0) return true;
  let node: SqlNamespace = tree;
  for (const seg of segments) {
    if (isLeaf(node)) return false;
    if (!(seg in node)) return false;
    node = node[seg];
  }
  return true;
}

/** Deep-clone tree and replace table leaves whose names appear in columnMap. */
export function overlayColumnMap(
  tree: SqlNamespace,
  columnMap: Record<string, string[]>,
): SqlNamespace {
  if (isLeaf(tree)) return tree;
  const out: Record<string, SqlNamespace> = {};
  for (const [key, child] of Object.entries(tree)) {
    if (isLeaf(child)) {
      out[key] = columnMap[key] ?? [...child];
    } else {
      out[key] = overlayColumnMap(child, columnMap);
    }
  }
  return out;
}
```

- [ ] **Step 4: Run tests — expect PASS**

```bash
pnpm exec vitest run src/lib/__tests__/sqlNamespace.test.ts
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/sqlNamespace.ts src/lib/__tests__/sqlNamespace.test.ts
git commit -m "feat(sql): add nested SQLNamespace merge helpers"
```

---

### Task 2: Dotted path prefix parser

**Files:**
- Create: `src/lib/sqlPathPrefix.ts`
- Create: `src/lib/__tests__/sqlPathPrefix.test.ts`

**Interfaces:**
- Produces: `export function parseQualifiedPathParents(text: string, cursor: number): string[]`
  - Returns parent segments before an incomplete identifier at `cursor`.
  - `hive.snap.` at end → `['hive','snap']`
  - `hive.sn` → `['hive']`
  - `FROM users` → `[]`

- [ ] **Step 1: Write failing tests**

```ts
import { describe, expect, it } from 'vitest';
import { parseQualifiedPathParents } from '../sqlPathPrefix';

describe('parseQualifiedPathParents', () => {
  it('returns parents after a trailing dot', () => {
    const sql = 'SELECT * FROM hive.snap.';
    expect(parseQualifiedPathParents(sql, sql.length)).toEqual(['hive', 'snap']);
  });

  it('returns parents for partial last segment', () => {
    const sql = 'SELECT * FROM hive.sn';
    expect(parseQualifiedPathParents(sql, sql.length)).toEqual(['hive']);
  });

  it('returns empty at top-level identifier', () => {
    const sql = 'SELECT * FROM hi';
    expect(parseQualifiedPathParents(sql, sql.length)).toEqual([]);
  });

  it('ignores dots inside strings', () => {
    const sql = "SELECT 'a.b.' FROM hive.";
    expect(parseQualifiedPathParents(sql, sql.length)).toEqual(['hive']);
  });
});
```

- [ ] **Step 2: Run — expect FAIL**

```bash
pnpm exec vitest run src/lib/__tests__/sqlPathPrefix.test.ts
```

- [ ] **Step 3: Implement parser**

Scan left from `cursor` over `[A-Za-z0-9_$]+` and `.` tokens; stop at whitespace/punctuation outside identifiers. Skip if cursor is inside a single/double-quoted string (simple scan from line start or full doc). Return all segments except the incomplete trailing identifier (if no trailing `.`, drop the last segment).

```ts
const IDENT = /[A-Za-z0-9_$]/;

export function parseQualifiedPathParents(text: string, cursor: number): string[] {
  const pos = Math.max(0, Math.min(cursor, text.length));
  // If inside quotes, return []
  if (isInsideQuotes(text, pos)) return [];

  let i = pos;
  // If mid-identifier, rewind to start of that identifier
  while (i > 0 && IDENT.test(text[i - 1]!)) i--;

  const parts: string[] = [];
  let j = i;
  // Walk left collecting ident (. ident)*
  while (j > 0) {
    // optional whitespace not allowed inside qualified name — stop
    if (text[j - 1] === '.') {
      j--;
      // read ident to the left
      let k = j;
      while (k > 0 && IDENT.test(text[k - 1]!)) k--;
      if (k === j) break;
      parts.unshift(text.slice(k, j));
      j = k;
      continue;
    }
    break;
  }
  // partial identifier after last dot was excluded by rewind; if cursor was after '.',
  // parts already are parents. If cursor was mid-ident, we rewound to ident start —
  // need parents only: walk left for .ident chain before that start.
  // Simpler approach: extract full qualified token touching cursor, split by '.', drop last.
  const token = extractQualifiedToken(text, pos);
  if (!token) return [];
  const segs = token.value.split('.').filter(Boolean);
  if (token.endsWithDot) return segs;
  if (segs.length <= 1) return [];
  return segs.slice(0, -1);
}

// Implement extractQualifiedToken + isInsideQuotes in the same file.
```

- [ ] **Step 4: Run — expect PASS**

```bash
pnpm exec vitest run src/lib/__tests__/sqlPathPrefix.test.ts
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/sqlPathPrefix.ts src/lib/__tests__/sqlPathPrefix.test.ts
git commit -m "feat(sql): parse dotted path parents for autocomplete ensure"
```

---

### Task 3: schemaStore namespace state + merge APIs

**Files:**
- Modify: `src/stores/schemaStore.ts`
- Modify: `src/stores/__tests__/schemaStore.test.ts`
- Modify: `src/plugin-sdk/index.ts` (export sync helpers)

**Interfaces:**
- Produces on store:
  - `namespaceTree: SqlNamespace`
  - `loadedPaths: Set<string>` (serialize as array in setState if needed; use `Set` in memory)
  - `supersetDbIds: Record<string, string>` // name → id
  - `mergeNamespace(segments, kind, names): void` — merges + marks `pathKey(segments)` loaded
  - `registerSupersetDatabases(entries: { name: string; id: string }[]): void` — merges top-level branches + id map
  - `setLoadedTables` also merges into namespace (MySQL: `[db]` tables; PG: group by schema under `[db]` or top-level schemas when single-db)
- Consumes: Task 1 helpers

- [ ] **Step 1: Write failing tests** for `mergeNamespace`, `registerSupersetDatabases`, and `setLoadedTables` PG grouping

```ts
it('mergeNamespace updates namespaceTree and loadedPaths', async () => {
  const { useSchemaStore } = await import('../schemaStore');
  useSchemaStore.getState().reset();
  useSchemaStore.getState().mergeNamespace(['db'], 'branch', ['hive']);
  expect(useSchemaStore.getState().namespaceTree).toEqual({ db: { hive: {} } });
  expect(useSchemaStore.getState().loadedPaths.has('db')).toBe(true);
});

it('registerSupersetDatabases maps name to id', async () => {
  const { useSchemaStore } = await import('../schemaStore');
  useSchemaStore.getState().reset();
  useSchemaStore.getState().registerSupersetDatabases([{ name: 'presto_afi_data', id: '558' }]);
  expect(useSchemaStore.getState().supersetDbIds).toEqual({ presto_afi_data: '558' });
  expect(useSchemaStore.getState().namespaceTree).toEqual({ presto_afi_data: {} });
});

it('setLoadedTables merges mysql-style database.table namespace', async () => {
  const { useSchemaStore } = await import('../schemaStore');
  useSchemaStore.getState().reset();
  useSchemaStore.setState({ isMultiDatabase: true });
  useSchemaStore.getState().setLoadedTables('app', [
    { name: 'users', tableType: 'table', schema: null, rowCount: null },
  ]);
  expect(useSchemaStore.getState().namespaceTree).toEqual({ app: { users: [] } });
});

it('setLoadedTables groups postgresql schemas under database when multi-db', async () => {
  const { useSchemaStore } = await import('../schemaStore');
  useSchemaStore.getState().reset();
  useSchemaStore.setState({ isMultiDatabase: true });
  useSchemaStore.getState().setLoadedTables('warehouse', [
    { name: 't', tableType: 'table', schema: 'public', rowCount: null },
  ]);
  expect(useSchemaStore.getState().namespaceTree).toEqual({
    warehouse: { public: { t: [] } },
  });
});

it('setLoadedTables uses schema.table when single-db postgresql', async () => {
  const { useSchemaStore } = await import('../schemaStore');
  useSchemaStore.getState().reset();
  useSchemaStore.setState({ isMultiDatabase: false });
  useSchemaStore.getState().setLoadedTables('warehouse', [
    { name: 't', tableType: 'table', schema: 'public', rowCount: null },
  ]);
  expect(useSchemaStore.getState().namespaceTree).toEqual({ public: { t: [] } });
});
```

Pass a `databaseType` into `setLoadedTables` **or** store `databaseType` on the schema store from `loadForConnection` so grouping can detect PG vs MySQL. Prefer storing `databaseType: string | null` on the store set in `loadForConnection` / CustomSchemaTreeHost.

- [ ] **Step 2: Run — expect FAIL**

```bash
pnpm exec vitest run src/stores/__tests__/schemaStore.test.ts
```

- [ ] **Step 3: Implement store fields + methods**

Rules for `setLoadedTables` namespace merge after setting flat tables/views:
- If `databaseType === 'superset'`: do not flatten here (plugin merges explicitly); still set flat tables for columnMap.
- Else if any table has non-null `schema` and schema is not `CATALOG`/`SCHEMA`:
  - `isMultiDatabase` → merge tables under `[database, schema]`
  - else → merge under `[schema]`
- Else (MySQL family): merge table names under `[database]` as `tables`.

Also on `loadForConnection` after resolving `databases`:
- `mergeNamespace([], 'branch', databases)` so top-level DB names exist for MySQL/PG/Kiwi.
- Reset `namespaceTree`, `loadedPaths`, `supersetDbIds` in `reset()` and at start of `loadForConnection`.

SDK additions:

```ts
export function syncSchemaNamespace(
  segments: string[],
  kind: 'branch' | 'tables',
  names: string[],
  options?: { connectionId?: string },
): void {
  if (options?.connectionId) {
    useSchemaStore.setState({ connectionId: options.connectionId });
  }
  useSchemaStore.getState().mergeNamespace(segments, kind, names);
}

export function registerSupersetDatabases(
  entries: { name: string; id: string }[],
  connectionId?: string,
): void {
  if (connectionId) useSchemaStore.setState({ connectionId });
  useSchemaStore.getState().registerSupersetDatabases(entries);
}
```

- [ ] **Step 4: Run — expect PASS**

```bash
pnpm exec vitest run src/stores/__tests__/schemaStore.test.ts
```

- [ ] **Step 5: Commit**

```bash
git add src/stores/schemaStore.ts src/stores/__tests__/schemaStore.test.ts src/plugin-sdk/index.ts
git commit -m "feat(schema): store nested namespaceTree for SQL autocomplete"
```

---

### Task 4: `ensureNamespacePath` driver strategies

**Files:**
- Create: `src/lib/ensureNamespace.ts`
- Create: `src/lib/__tests__/ensureNamespace.test.ts`
- Modify: `src/stores/schemaStore.ts` — add `ensureNamespacePath(segments: string[]): Promise<void>` delegating to helper with store getters/setters

**Interfaces:**
- Consumes: `databaseCommands`, store state (`connectionId`, `databaseType`, `loadedPaths`, `supersetDbIds`, `isMultiDatabase`, `namespaceTree`)
- Produces: `ensureNamespacePath` on store; pure `export async function ensureNamespacePath(...)` testable with injected deps

- [ ] **Step 1: Write failing tests** with mocked commands

```ts
const getDatabases = vi.fn();
const getTables = vi.fn();
const useDatabase = vi.fn();

// Superset: [] loads databases
// Superset: ['presto'] → get_tables('558') when map presto→558, merge catalogs from CATALOG rows
// Superset: ['presto','hive'] → get_tables('558/hive')
// Superset: ['presto','hive','snap'] → get_tables('558/hive/snap') tables
// MySQL: ['app'] → useDatabase + getTables(conn, app)
// Skip when path already in loadedPaths
// Dedupe concurrent ensures for same key
```

Navigation row helpers (match Superset tree):
- `item.schema === 'CATALOG'` → child name = last segment of `item.name` path (`558/hive` → `hive`) **or** use display segment after `dbId/`
- `item.schema === 'SCHEMA'` → last segment of path

For Superset catalog listing under dbId, `TableInfo.name` is `558/hive` — strip `${dbId}/` prefix for SQL segment.

- [ ] **Step 2: Run — expect FAIL**

```bash
pnpm exec vitest run src/lib/__tests__/ensureNamespace.test.ts
```

- [ ] **Step 3: Implement strategies**

```ts
export interface EnsureDeps {
  connectionId: string;
  databaseType: string | null;
  isMultiDatabase: boolean;
  loadedPaths: Set<string>;
  supersetDbIds: Record<string, string>;
  mergeNamespace: (segments: string[], kind: 'branch' | 'tables', names: string[]) => void;
  registerSupersetDatabases: (entries: { name: string; id: string }[]) => void;
  getDatabases: (connectionId: string) => Promise<string[]>;
  getTables: (connectionId: string, database: string) => Promise<TableInfo[]>;
  useDatabase: (connectionId: string, database: string) => Promise<void>;
}

const inflight = new Map<string, Promise<void>>();

export async function ensureNamespacePath(segments: string[], deps: EnsureDeps): Promise<void> {
  const key = `${deps.connectionId}|${segments.join('.')}`;
  const existing = inflight.get(key);
  if (existing) return existing;
  const p = runEnsure(segments, deps).finally(() => inflight.delete(key));
  inflight.set(key, p);
  return p;
}
```

`runEnsure` behavior:
1. If `loadedPaths.has(pathKey(segments))` return.
2. Switch on `databaseType`:
   - `superset`: as spec.
   - `postgresql`:  
     - `[]` → getDatabases → merge branch (if not loaded).  
     - multi `[db]` → useDatabase + getTables → group schemas → for each schema merge tables. Mark `[db]` loaded.  
     - single `[schema]` → if flat tables already contain schema, merge from memory; else load preferred db tables and filter.
   - default SQL (`mysql`/`mariadb`/`kiwi`/others with supportsSQL):  
     - `[]` → getDatabases → branch.  
     - `[db]` → useDatabase + getTables → table leaves.
3. On success mark `pathKey(segments)` loaded via `mergeNamespace` (merge already marks).
4. On failure: do not mark loaded; swallow error (return).

Wire store method:

```ts
ensureNamespacePath: async (segments) => {
  const s = get();
  if (!s.connectionId) return;
  await ensureNamespacePath(segments, { ...depsFrom(s), mergeNamespace: get().mergeNamespace, ... });
},
```

- [ ] **Step 4: Run — expect PASS**

```bash
pnpm exec vitest run src/lib/__tests__/ensureNamespace.test.ts src/stores/__tests__/schemaStore.test.ts
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/ensureNamespace.ts src/lib/__tests__/ensureNamespace.test.ts src/stores/schemaStore.ts
git commit -m "feat(schema): ensureNamespacePath with per-driver fetch strategies"
```

---

### Task 5: buildEditorSchema + QueryPanel + SqlEditor wiring

**Files:**
- Create: `src/lib/buildEditorSchema.ts`
- Create: `src/lib/__tests__/buildEditorSchema.test.ts`
- Modify: `src/components/SqlEditor.tsx` — type `SqlSchema` as `SQLNamespace` from `@codemirror/lang-sql` (or re-export from `sqlNamespace.ts`); add prop `onQualifiedPath?: (parents: string[]) => void` fired from `updateListener` when parents change
- Modify: `src/windows/connection/QueryPanel.tsx` — use `buildEditorSchema`; debounce `ensureNamespacePath`

**Interfaces:**
- `buildEditorSchema({ namespaceTree, tables, views, columnMap }): SqlNamespace`
  - Start from `namespaceTree` if non-empty; else fallback flat `{ [table]: cols }` from tables/views (backward compatible).
  - Always `overlayColumnMap` at the end.

- [ ] **Step 1: Tests for buildEditorSchema**

```ts
it('prefers nested tree and overlays columns', () => {
  expect(
    buildEditorSchema({
      namespaceTree: { app: { users: [] } },
      tables: [],
      views: [],
      columnMap: { users: ['id'] },
    }),
  ).toEqual({ app: { users: ['id'] } });
});

it('falls back to flat tables when namespace empty', () => {
  expect(
    buildEditorSchema({
      namespaceTree: {},
      tables: [{ name: 'users', tableType: 'table', schema: null, rowCount: null }],
      views: [],
      columnMap: { users: ['id'] },
    }),
  ).toEqual({ users: ['id'] });
});
```

- [ ] **Step 2: Implement builder + wire QueryPanel**

In `SqlEditor` `updateListener`:

```ts
const parents = parseQualifiedPathParents(update.state.doc.toString(), update.state.selection.main.head);
// compare to lastParentsRef; if changed, onQualifiedPathRef.current?.(parents)
```

In `QueryPanel`:

```ts
const namespaceTree = useSchemaStore((s) => s.namespaceTree);
const ensureNamespacePath = useSchemaStore((s) => s.ensureNamespacePath);
const editorSchema = useMemo(
  () => buildEditorSchema({ namespaceTree, tables, views, columnMap }),
  [namespaceTree, tables, views, columnMap],
);

const ensureTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
const handleQualifiedPath = useCallback((parents: string[]) => {
  if (ensureTimer.current) clearTimeout(ensureTimer.current);
  ensureTimer.current = setTimeout(() => {
    void ensureNamespacePath(parents);
  }, 120);
}, [ensureNamespacePath]);
```

Pass `onQualifiedPath={handleQualifiedPath}` to `SqlEditor`.

Keep existing `loadColumnMap` effect.

- [ ] **Step 3: Run unit tests**

```bash
pnpm exec vitest run src/lib/__tests__/buildEditorSchema.test.ts src/components/__tests__/resolveCmDialect.test.ts
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/buildEditorSchema.ts src/lib/__tests__/buildEditorSchema.test.ts \
  src/components/SqlEditor.tsx src/windows/connection/QueryPanel.tsx
git commit -m "feat(editor): wire nested schema and on-demand path ensure"
```

---

### Task 6: Host schema trees merge into namespace

**Files:**
- Modify: `src/windows/connection/schema-tree/MultiDatabaseSchemaTree.tsx`
- Modify: `src/windows/connection/schema-tree/StandardSchemaTree.tsx`
- Modify: `src/windows/connection/schema-tree/SchemaTree.tsx` (set `databaseType` on store in CustomSchemaTreeHost + when mounting standard trees)
- Modify: `src/windows/connection/schema-tree/__tests__/SchemaTree.test.tsx`

**Interfaces:**
- Consumes: `mergeNamespace`, `setLoadedTables` / `activateDatabase` from prior WIP

- [ ] **Step 1: Extend SchemaTree tests**

After expanding mysql `alpha` with tables `t1`,`t2`, assert:

```ts
expect(useSchemaStore.getState().namespaceTree).toEqual({
  alpha: { t1: [], t2: [] },
});
```

For postgresql multi-db expand with `{ name: 'orders', schema: 'public' }`:

```ts
expect(useSchemaStore.getState().namespaceTree).toEqual({
  db1: { public: { orders: [] } },
});
```

- [ ] **Step 2: Implement merges**

`MultiDatabaseSchemaTree`: when tables fetched / activated, `setLoadedTables` already merges — ensure `databaseType` is on store (set in `loadForConnection` options — already passed). Also on `loadForConnection` completion, branches for `databases` exist (Task 3).

`StandardSchemaTree`: after its normal load (uses store `loadForConnection` without skip), `setLoadedTables`/`loadTables` path merges schemas — verify StandardSchemaTree uses store tables (no extra work if Task 3 covers loadTables). If StandardSchemaTree only reads store, Task 3 is enough — still set `databaseType` in `loadForConnection` from props.

`SchemaTree` `CustomSchemaTreeHost`: also `useSchemaStore.setState({ databaseType: props.databaseType })`.

- [ ] **Step 3: Run**

```bash
pnpm exec vitest run src/windows/connection/schema-tree/__tests__/SchemaTree.test.tsx
```

- [ ] **Step 4: Commit**

```bash
git add src/windows/connection/schema-tree/
git commit -m "feat(schema-tree): feed nested namespace from multi/standard trees"
```

---

### Task 7: Superset plugin — sync all four levels

**Files:**
- Modify: `.plugins/superset/ui/SupersetSchemaTree.tsx`
- Modify: `.plugins/superset/ui/plugin-meta.ts` (keep `sqlDialect: 'postgresql'` if already set)
- Push plugin repo + Modify: `plugins-registry.json` ref

**Interfaces:**
- Consumes SDK: `registerSupersetDatabases`, `syncSchemaNamespace`, `syncSchemaTables`

- [ ] **Step 1: After databases list loads**

```ts
registerSupersetDatabases(
  entries.map((e) => ({ name: e.name, id: e.id })),
  connectionId,
);
```

- [ ] **Step 2: On catalog load for db**

Path segments: `[db.name]` branch names = catalog display segments (`hive`).

```ts
syncSchemaNamespace([db.name], 'branch', catalogNames, { connectionId });
```

- [ ] **Step 3: On schema load**

```ts
syncSchemaNamespace([db.name, catalogName], 'branch', schemaNames, { connectionId });
```

- [ ] **Step 4: On table load**

```ts
syncSchemaNamespace([db.name, catalogName, schemaName], 'tables', tableNames, { connectionId });
syncSchemaTables(`${dbId}/${catalog}/${schema}`, items, connectionId); // keep flat/columnMap path
```

Derive `db.name` / ids from path prefixes already used in the tree (`row.path` / `parseDatabaseEntry`).

- [ ] **Step 5: Commit + push plugin, bump host registry**

```bash
cd .plugins/superset
git checkout -b fix/hierarchical-autocomplete-sync
# commit, push, ff-merge to main (same process as prior superset fix)
cd ../..
# set plugins-registry.json superset.ref to new SHA
git add plugins-registry.json
git commit -m "chore(plugins): bump superset for hierarchical namespace sync"
```

- [ ] **Step 6: Manual smoke checklist** (record in commit message or PR)

1. Superset: `db.` → catalogs; `db.cat.` → schemas; `db.cat.sch.` → tables  
2. MySQL multi-db: `dbname.` → tables  
3. PostgreSQL: `public.` → tables  

---

### Task 8: Full unit suite + cleanup

**Files:**
- Any stragglers from WIP on the branch (`resolveCmDialect`, flat sync tests)

- [ ] **Step 1: Run full unit tests**

```bash
pnpm test:unit
```

Expected: all pass.

- [ ] **Step 2: Fix any type errors from `SqlSchema` widening**

```bash
pnpm exec tsc --noEmit
```

- [ ] **Step 3: Final commit if needed**

```bash
git add -A
git commit -m "test: finish hierarchical autocomplete coverage"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Nested SQLNamespace in store | 1, 3 |
| Hybrid load / ensure on `.` | 2, 4, 5 |
| Superset `db.catalog.schema.table` + name→id | 3, 4, 7 |
| MySQL/Kiwi `database.table` | 3, 4, 6 |
| PostgreSQL schema / multi-db | 3, 4, 6 |
| No dotted columns; columnMap overlay | 1, 5 |
| Sidebar sync shares tree | 6, 7 |
| SDK helpers for plugins | 3, 7 |
| Unit tests listed in spec | 1–6, 8 |

## Self-review notes

- No TBD placeholders in tasks.
- `setLoadedTables` / `syncSchemaTables` from branch WIP are explicitly reused in Tasks 3 and 7.
- `loadedPaths` marking happens inside `mergeNamespace` so empty lists still prevent refetch.
- OLAP: same SDK merge API; no separate task unless plugin tree exists in workspace during implementation — if present, mirror Superset sync patterns in a follow-up commit under Task 7.
