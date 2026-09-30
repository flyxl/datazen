/**
 * The equivalence gate for the navigator: 13 row variants, rendered twice.
 *
 * The shared `VirtualTree` shell knows nothing about connections, databases or
 * object categories. Everything a screen reader hears about a navigator row is
 * decided by two host-owned pieces — `buildNavigatorFlatRows` (which row exists,
 * how deep it is painted) and `navigatorRowAria` (what it announces) — and the
 * shell only converts `levelDepth` into a 1-based `aria-level`. That leaves the
 * tree's accessibility resting on two facts that are easy to state and easy to
 * break:
 *
 *  1. **Every variant announces something.** A new row type that forgets to
 *     return ARIA renders silently and is heard as a leaf.
 *  2. **Search changes the announced level by exactly one, and only for the
 *     rows that lost a parent.** A global object search drops the group header
 *     between a connection and its databases, so every row below announces one
 *     rung higher while still painting at the same indent.
 *
 * The second is the subtle one, and the reason indent and level are asserted
 * separately throughout: the two numbers are *supposed* to disagree under a
 * search, and "harmonising" them is the regression this file exists to catch.
 */
import { describe, expect, it } from 'vitest';
import { PINNED_GROUP_KEY, RECENT_GROUP_KEY } from '../../../lib/connectionLocator';
import { TREE_TOP_LEVEL, ariaLevelOf, indentOf } from '@datazen/ui';
import { buildNavigatorFlatRows } from '../navigator/buildFlatRows';
import { isNavigatorBranch, navigatorRowAria } from '../navigator/treeRowAria';
import type { BuildNavigatorFlatRowsParams } from '../navigator/buildFlatRows';
import type { ConnectionSchemaState } from '../../../stores/schemaStoreState';
import type { ConnectionConfig, TableInfo } from '../../../types';
import type { UnifiedRow } from '../navigator/types';

const INDENT_BASE_REM = 0;
const INDENT_STEP_REM = 0.75;
const indentRem = (row: UnifiedRow) => indentOf(row.depth, INDENT_BASE_REM, INDENT_STEP_REM);

interface Dump {
  type: UnifiedRow['type'];
  label: string;
  /** `null` for a decoration: no role, no level. */
  level: number | null;
  /** `null` when the attribute is absent, which is not the same as `false`. */
  expanded: boolean | null;
  indentRem: number;
}

/** A readable stand-in for the label the row paints. */
function rowKeyLabel(row: UnifiedRow): string {
  switch (row.type) {
    case 'section':
      return row.section;
    case 'group':
      return row.groupName;
    case 'connection':
      return row.conn.id;
    case 'db':
    case 'kv-db':
      return `${row.connectionId}/${row.dbName}`;
    case 'schema':
      return row.schemaName;
    case 'category':
      return row.key;
    case 'table':
      return row.item.name;
    case 'object':
      return `${row.obj.name}#${row.catId}`;
    case 'db-loading':
      return row.ownerKey;
    case 'namespace-node':
      return row.key;
    case 'empty-group':
      return `empty:${row.groupName}`;
    case 'no-connections':
      return 'no-connections';
  }
}

function dump(rows: UnifiedRow[]): Dump[] {
  return rows.map((row) => {
    const aria = navigatorRowAria(row);
    return {
      type: row.type,
      label: rowKeyLabel(row),
      level: aria?.['aria-level'] ?? null,
      expanded: aria?.['aria-expanded'] ?? null,
      indentRem: indentRem(row),
    };
  });
}

/*
 * Every variant, written out rather than produced by the builder.
 *
 * The builder only emits `kv-db`, the key-value `db-loading` and
 * `namespace-node` for drivers whose meta says "key-value" or "path hierarchy",
 * which depends on the generated driver registry. Asserting the ARIA contract
 * against a registry-gated fixture would make it silently vacuous on a basic
 * build, so the variant table is literal: the union is the spec, and this is
 * the spec written down. `buildNavigatorFlatRows` gets its own section below.
 */
const DEPTH = 2;

const VARIANTS: UnifiedRow[] = [
  {
    type: 'section',
    section: 'pinned',
    displayName: 'Pinned',
    count: 1,
    expanded: true,
    depth: 0,
    levelDepth: 0,
  },
  {
    type: 'group',
    groupName: 'team-a',
    displayName: 'Team A',
    count: 1,
    expanded: true,
    depth: 0,
    levelDepth: 0,
  },
  {
    type: 'connection',
    conn: { id: 'c1', name: 'c1', type: 'postgres' } as never,
    sectionGroup: 'team-a',
    isSelected: true,
    status: 'connected',
    expanded: true,
    depth: DEPTH,
    levelDepth: DEPTH,
  },
  {
    type: 'db',
    connectionId: 'c1',
    dbSessionId: 's1',
    dbName: 'db1',
    expanded: true,
    loading: false,
    isOpen: true,
    depth: DEPTH + 1,
    levelDepth: DEPTH + 1,
  },
  {
    type: 'schema',
    connectionId: 'c1',
    dbName: 'db1',
    schemaName: 'public',
    expanded: true,
    depth: DEPTH + 2,
    levelDepth: DEPTH + 2,
  },
  {
    type: 'category',
    key: 'c1::db1::public::tables',
    cat: { id: 'tables' } as never,
    count: 1,
    expanded: true,
    depth: DEPTH + 3,
    levelDepth: DEPTH + 3,
  },
  {
    type: 'table',
    item: { name: 'users', schema: 'public', tableType: 'table' } as TableInfo,
    catId: 'tables',
    isSelected: false,
    connectionId: 'c1',
    dbSessionId: 's1',
    dbName: 'db1',
    depth: DEPTH + 4,
    levelDepth: DEPTH + 4,
  },
  {
    type: 'object',
    obj: { name: 'fn_calc' } as never,
    catId: 'routines',
    connectionId: 'c1',
    dbName: 'db1',
    schemaName: 'public',
    depth: DEPTH + 4,
    levelDepth: DEPTH + 4,
  },
  {
    type: 'kv-db',
    connectionId: 'c2',
    dbSessionId: 's2',
    dbName: 'db0',
    isSelected: false,
    depth: DEPTH + 1,
    levelDepth: DEPTH + 1,
  },
  { type: 'db-loading', ownerKey: 'conn:c2', depth: DEPTH + 1, levelDepth: DEPTH + 1 },
  {
    type: 'namespace-node',
    name: 'tbl',
    depth: DEPTH + 2,
    levelDepth: DEPTH + 2,
    expanded: false,
    isLeaf: false,
    segments: ['tbl'],
    key: 'ns:db1::tbl',
    connectionId: 'c3',
    dbSessionId: 's3',
  },
  { type: 'empty-group', groupName: 'team-b', depth: 1, levelDepth: 1 },
  { type: 'no-connections', depth: 0, levelDepth: 0 },
];

const ALL_TYPES = [
  'section',
  'group',
  'connection',
  'db',
  'schema',
  'category',
  'table',
  'object',
  'kv-db',
  'db-loading',
  'namespace-node',
  'empty-group',
  'no-connections',
] as const;

const DECORATIONS = new Set<UnifiedRow['type']>(['empty-group', 'no-connections']);
/** Rows that own no child list, so `aria-expanded` must be absent, not `false`. */
const LEAVES = new Set<UnifiedRow['type']>(['table', 'object', 'kv-db', 'db-loading']);

describe('navigator row variants · ARIA contract', () => {
  it('covers all 13 variants', () => {
    const seen = new Set(VARIANTS.map((r) => r.type));
    expect(seen.size).toBe(13);
    expect([...seen].sort()).toEqual([...ALL_TYPES].sort());
  });

  it('announces a role and a level for every variant except the two decorations', () => {
    for (const row of VARIANTS) {
      const aria = navigatorRowAria(row);
      if (DECORATIONS.has(row.type)) {
        expect(aria, `${row.type} must stay out of the tree structure`).toBeNull();
      } else {
        expect(aria?.role, `${row.type} announced no role`).toBe('treeitem');
        expect(aria?.['aria-level'], `${row.type} announced no level`).toBe(
          row.levelDepth + TREE_TOP_LEVEL,
        );
      }
    }
  });

  it('starts at level 1, because aria-level is 1-based while depth is 0-based', () => {
    expect(TREE_TOP_LEVEL).toBe(1);
    for (const row of VARIANTS) {
      if (DECORATIONS.has(row.type)) continue;
      // One conversion, in one place: `ariaLevelOf`. Asserting the arithmetic
      // here is what stops a renderer from growing a second opinion.
      expect(ariaLevelOf(row), row.type).toBe(row.levelDepth + TREE_TOP_LEVEL);
    }
    // And the two roots really are at the top rung.
    for (const type of ['section', 'group'] as const) {
      const root = VARIANTS.find((r) => r.type === type)!;
      expect(ariaLevelOf(root)).toBe(TREE_TOP_LEVEL);
    }
  });

  it('omits aria-expanded on the rows that own no child list', () => {
    for (const row of dump(VARIANTS)) {
      if (LEAVES.has(row.type)) {
        // Absent, not `false`: a `false` tells assistive tech a leaf is
        // collapsed and therefore activatable, which is a lie about a table.
        expect(row.expanded, `${row.type} reported aria-expanded`).toBeNull();
      }
    }
  });

  it('answers "is this a branch" from the row, never from its position', () => {
    // A `namespace-node` is a branch only while it has children; the same type
    // answers differently depending on the row, which is the whole point of
    // asking the row rather than its index.
    const node = VARIANTS.find((r) => r.type === 'namespace-node') as Extract<
      UnifiedRow,
      { type: 'namespace-node' }
    >;
    expect(isNavigatorBranch(node)).toBe(true);
    expect(isNavigatorBranch({ ...node, isLeaf: true })).toBe(false);
  });
});

describe('navigator row variants · the two ARIA functions must agree', () => {
  /**
   * `navigatorRowAria` and `isNavigatorBranch` both answer "does this row own a
   * child list", and for a `namespace-node` they are required to give the same
   * answer. They used to disagree: the announcement wrote `aria-expanded` for
   * every `namespace-node` while the branch probe read `isLeaf`, so a namespace
   * leaf announced itself as a collapsed disclosure of a list it does not have.
   *
   * This is the cross-check that gap was missing. It is deliberately phrased
   * against `isNavigatorBranch` rather than against a hand-listed set of leaf
   * *types*, because `namespace-node` is polymorphic — one type that is a branch
   * at depth 2 and a leaf at depth 4 — and any type-keyed list has to lie about
   * one of those two.
   */
  it('never announces a disclosure on a row the shell calls a leaf', () => {
    // The polymorphic variant has to be present in *both* shapes, or this check
    // is vacuous exactly where it matters: the table above only carries the
    // branch form, and the branch form is the one that was already correct.
    const branch = VARIANTS.find((r) => r.type === 'namespace-node') as Extract<
      UnifiedRow,
      { type: 'namespace-node' }
    >;
    const rows = [...VARIANTS, { ...branch, isLeaf: true, name: 'mv1' }];
    // A guard against the list silently losing its one leaf of that type.
    expect(rows.filter((r) => r.type === 'namespace-node' && r.isLeaf)).toHaveLength(1);

    for (const row of rows) {
      if (DECORATIONS.has(row.type)) continue;
      const announced = navigatorRowAria(row)?.['aria-expanded'];
      if (!isNavigatorBranch(row)) {
        // The defect: this read `aria-expanded="false"`.
        expect(announced, `${rowKeyLabel(row)} is a leaf but announced a disclosure`).toBe(
          undefined,
        );
      }
    }
  });

  it('covers the polymorphic variant in both of its states, not just the branch one', () => {
    const branch = VARIANTS.find((r) => r.type === 'namespace-node') as Extract<
      UnifiedRow,
      { type: 'namespace-node' }
    >;
    // The leaf state is not in the 13-variant table, because the table is keyed
    // by type and this type has two shapes. It has to be spelled out.
    const leaf: Extract<UnifiedRow, { type: 'namespace-node' }> = { ...branch, isLeaf: true };

    expect(isNavigatorBranch(branch)).toBe(true);
    expect(navigatorRowAria(branch)?.['aria-expanded']).toBe(false);

    expect(isNavigatorBranch(leaf)).toBe(false);
    // Absent, not `false`. Both halves are asserted, because the bug was one
    // function being right while the other was wrong.
    expect(navigatorRowAria(leaf)).toEqual({ role: 'treeitem', 'aria-level': ariaLevelOf(leaf) });
    expect(navigatorRowAria(leaf)).not.toHaveProperty('aria-expanded');
  });

  it('keeps the connection exception explicit rather than accidental', () => {
    // The one variant where the two answers are *meant* to differ: a database's
    // parent is its connection even while the connection is idle, so the shell
    // must keep treating the row as a branch — but there is nothing to disclose
    // yet, so no `aria-expanded`. If this ever starts announcing `false`, the
    // exception has turned into the bug above.
    //
    // Note the deliberate difference from the leaf case, asserted differently
    // below: here the property is *present and undefined* (React omits an
    // attribute whose value is `undefined`, so the DOM is identical), whereas a
    // namespace leaf is now genuinely keyless. Both are correct; only the
    // keyless form is the stronger statement, so only it claims one.
    const base = VARIANTS.find((r) => r.type === 'connection') as Extract<
      UnifiedRow,
      { type: 'connection' }
    >;
    const idle = { ...base, status: 'error' as const, expanded: false };
    expect(isNavigatorBranch(idle)).toBe(true);
    expect(navigatorRowAria(idle)?.['aria-expanded']).toBeUndefined();
    expect(navigatorRowAria(idle)).toEqual({ role: 'treeitem', 'aria-level': ariaLevelOf(idle) });
  });
});

describe('navigator row variants · search shifts the level by exactly one', () => {
  /**
   * The builder's rule, restated: under a search the group header that sat
   * between a connection and its databases is gone, so rows at or below
   * `CONNECTION_CHILD_DEPTH` announce one rung higher and paint where they were.
   */
  const searched = (row: UnifiedRow): UnifiedRow => {
    if (DECORATIONS.has(row.type)) return row;
    const hidden = row.depth >= DEPTH ? 1 : 0;
    return { ...row, levelDepth: row.levelDepth - hidden } as UnifiedRow;
  };

  it('drops the announced level by one, and never by two or by none', () => {
    for (const row of VARIANTS) {
      if (DECORATIONS.has(row.type)) continue;
      const before = navigatorRowAria(row)!['aria-level']!;
      const after = navigatorRowAria(searched(row))!['aria-level']!;
      expect(after, `${row.type} (depth ${row.depth})`).toBe(
        row.depth >= DEPTH ? before - 1 : before,
      );
    }
  });

  it('paints the same indent in both states while announcing a different level', () => {
    const rows = VARIANTS.filter((r) => r.depth >= DEPTH && !DECORATIONS.has(r.type));
    expect(rows.length).toBeGreaterThan(0);
    for (const row of rows) {
      const before = dump([row])[0]!;
      const after = dump([searched(row)])[0]!;
      expect(after.indentRem, `${row.type} indent moved`).toBe(before.indentRem);
      expect(after.level).toBe(before.level! - 1);
    }
  });

  it('leaves the roots alone: a search hides nothing above a connection', () => {
    for (const row of VARIANTS.filter((r) => r.depth < DEPTH && !DECORATIONS.has(r.type))) {
      expect(navigatorRowAria(searched(row))!['aria-level'], row.type).toBe(
        navigatorRowAria(row)!['aria-level'],
      );
    }
  });
});

/*
 * The same rule, measured on real builder output rather than on a table of
 * hand-written rows. The fixture is a plain SQL connection, so it reaches the
 * variants a SQL navigator actually shows; the key-value and path-hierarchy
 * variants are gated on the generated driver registry and are covered by the
 * literal table above plus `ConnectionNavigatorTree.test.tsx`.
 */
const connection = (id: string): ConnectionConfig => ({
  id,
  name: id,
  databaseType: 'postgresql',
  host: 'localhost',
  port: 5432,
  username: 'u',
  password: '',
  database: 'db1',
  sslMode: 'disable',
});

function schemaState(): ConnectionSchemaState {
  return {
    currentDatabase: null,
    currentSchema: null,
    databases: ['db1'],
    databaseType: 'PostgreSQL',
    isMultiDatabase: true,
    tables: [{ name: 'users', schema: 'public', tableType: 'table' } as TableInfo],
    views: [],
    schemaNames: ['public'],
    tableCatalogs: {},
    relationColumns: {},
    namespaceTree: {},
    loadedPaths: new Set(),
    pathItems: {},
    pathAliases: {},
    namespaceOwnedByPlugin: false,
    schemaEpoch: 0,
    expanded: new Set(),
    selectedId: null,
    loading: false,
    ensuringCount: 0,
    error: null,
    columnInflight: new Set(),
  };
}

function params(
  query: string,
  grouped: BuildNavigatorFlatRowsParams['grouped'],
  expandedGroups: Set<string> = new Set([PINNED_GROUP_KEY, RECENT_GROUP_KEY]),
  expandedConnections: Set<string> = new Set([`${PINNED_GROUP_KEY}::c1`]),
): BuildNavigatorFlatRowsParams {
  return {
    grouped,
    expandedGroups,
    expandedConnections,
    expandedDbs: new Set(['c1::db1']),
    expandedSchemas: new Set(['c1::db1::public']),
    expandedCats: new Set(['c1::db1::public::tables', 'c1::db1::public::routines']),
    activeConnections: { c1: { status: 'connected', dbSessionId: 's1' } as never },
    activeConnectionId: 'c1',
    schemas: new Map([['s1', schemaState()]]),
    dbTablesMap: { 's1::db1': [] },
    dbObjectsMap: {},
    loadingDbs: new Set(),
    openDbs: {},
    query,
    t: (key) => key as string,
  };
}

const GROUPED = [
  { group: PINNED_GROUP_KEY, connections: [connection('c1')] },
  { group: RECENT_GROUP_KEY, connections: [] },
];
const plain = dump(buildNavigatorFlatRows(params('', GROUPED)));
const searching = dump(buildNavigatorFlatRows(params('users', GROUPED)));
/** No groups at all, which is the only way to reach `no-connections`. */
const bare = dump(buildNavigatorFlatRows(params('', [])));
/**
 * The same tree under a *user-created* group name.
 *
 * `GROUPED` above uses the two reserved group keys, and the builder special-
 * cases them: pinned renders as a `section`. A group whose name is the user's
 * own renders as a `group` instead. Both are ordinary and both are in the
 * product, so the plain-state set is fixture-dependent and must be labelled as
 * such wherever it is written down.
 */
// Same shape as `GROUPED` — one group with a connection, one empty — but with
// user-chosen names, so the builder has to spell the root `group` instead of
// `section`. Comparing the two sets isolates the root spelling and nothing else.
const CUSTOM = [
  { group: 'team-b', connections: [connection('c1')] },
  { group: 'team-z', connections: [] },
];
// A connection is keyed `group::connectionId`, so moving it into a custom group
// moves its expansion key too — otherwise the group paints and stops there.
const CUSTOM_EXPANDED = [new Set(['team-b', 'team-z']), new Set(['team-b::c1'])] as const;
const customGrouped = dump(
  buildNavigatorFlatRows(params('', CUSTOM, CUSTOM_EXPANDED[0], CUSTOM_EXPANDED[1])),
);
const find = (rows: Dump[], label: string) => rows.find((r) => `${r.type}:${r.label}` === label);

describe('buildNavigatorFlatRows · the shift on real rows', () => {
  it('produces the roots and the hint row, and drops both under a search', () => {
    // Measured, not assumed: a search replaces the grouped sections with one
    // unranked list, so it emits no `section` and no `group` row at all, and an
    // empty group is filtered away rather than padded with its hint row. The
    // point of writing both sets out is that a future change to *which* row
    // roots a search has to be a deliberate edit here.
    //
    // FIXTURE PRECONDITION on the plain set: it is measured under the two
    // *reserved* group keys, where pinned renders as a `section`. It is not a
    // claim about every navigator. The next test pins the other case.
    expect([...new Set(plain.map((r) => r.type))].sort()).toEqual([
      'category',
      'connection',
      'db',
      'empty-group',
      'schema',
      'section',
      'table',
    ]);
    expect([...new Set(searching.map((r) => r.type))].sort()).toEqual([
      'category',
      'connection',
      'db',
      'schema',
      'table',
    ]);
    expect(bare.map((r) => r.type)).toContain('no-connections');
  });

  it('adds a `group` row under a user-created group name, and a search still does not', () => {
    // The plain set above is a fact about the reserved group keys, not about
    // the navigator. With a group the user named, the builder emits `group`
    // where it emits `section` for pinned — so the ordinary union is eight, not
    // seven. Quoting the seven-set without this precondition would overstate it.
    expect([...new Set(customGrouped.map((r) => r.type))].sort()).toEqual([
      'category',
      'connection',
      'db',
      'empty-group',
      'group',
      'schema',
      'table',
    ]);
    // `group` and `section` are the two roots; exactly one of them is present,
    // decided by which kind of group the user has.
    expect(customGrouped.some((r) => r.type === 'section')).toBe(false);
    expect(plain.some((r) => r.type === 'group')).toBe(false);
    // And the searched set is genuinely root-free either way, so the level
    // shift below does not depend on which root spelling was in play.
    const searchedCustom = dump(
      buildNavigatorFlatRows(params('users', CUSTOM, CUSTOM_EXPANDED[0], CUSTOM_EXPANDED[1])),
    );
    expect([...new Set(searchedCustom.map((r) => r.type))].sort()).toEqual(
      [...new Set(searching.map((r) => r.type))].sort(),
    );
  });

  it('drops one level for every row below the header the search removed', () => {
    for (const label of ['db:c1/db1', 'schema:public', 'category:c1::db1::public::tables']) {
      const before = find(plain, label);
      const after = find(searching, label);
      expect(before, `${label} in the plain tree`).toBeTruthy();
      expect(after, `${label} in the searched tree`).toBeTruthy();
      expect(after!.level, label).toBe(before!.level! - 1);
    }
  });

  it('paints the searched rows at the indent they had before', () => {
    for (const label of ['db:c1/db1', 'schema:public', 'category:c1::db1::public::tables']) {
      expect(find(searching, label)!.indentRem, label).toBe(find(plain, label)!.indentRem);
    }
  });

  it('announces a level on every row it emits, and none on the two decorations', () => {
    for (const row of [...plain, ...searching, ...bare]) {
      if (DECORATIONS.has(row.type)) expect(row.level, row.type).toBeNull();
      else expect(row.level, row.type).not.toBeNull();
    }
  });
});
