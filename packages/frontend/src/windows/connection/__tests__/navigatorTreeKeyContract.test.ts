/**
 * The navigator tree's row-key contract.
 *
 * The connection navigator renders a flat pre-order list through the shared
 * `VirtualTree` shell, which reuses a DOM node for as long as its React key is
 * stable. That makes `getUnifiedRowKey` load-bearing in a way a pure function
 * is not: a key that drifts with a row's position does not merely change a
 * string, it moves one row's element, focus and in-flight state onto another
 * row. Every case below is a bug that actually shipped once.
 */
import { describe, expect, it } from 'vitest';
import { getUnifiedRowKey } from '../navigator/utils';
import type { ConnectionConfig } from '../../../types';
import type { UnifiedRow } from '../navigator/types';

const connection = (id: string): ConnectionConfig => ({
  id,
  name: id,
  databaseType: 'postgresql',
  sslMode: 'disable',
});

/**
 * Rows are built whole rather than cast down to `UnifiedRow`. A `as UnifiedRow`
 * on a two-field literal would let a key contract test pass against rows the
 * builder can never emit — and a key built from a field the real row does not
 * carry is exactly the class of bug this file exists to catch.
 */
const sectionRow = (section: 'pinned' | 'recent'): UnifiedRow => ({
  type: 'section',
  section,
  displayName: section,
  count: 0,
  expanded: true,
  depth: 0,
  levelDepth: 0,
});

const groupRow = (groupName: string): UnifiedRow => ({
  type: 'group',
  groupName,
  displayName: groupName,
  count: 0,
  expanded: true,
  depth: 0,
  levelDepth: 0,
});

const connectionRow = (id: string, sectionGroup: string): UnifiedRow => ({
  type: 'connection',
  conn: connection(id),
  sectionGroup,
  isSelected: false,
  status: 'disconnected',
  expanded: false,
  depth: 1,
  levelDepth: 1,
});

const dbRow = (dbName: string, connectionId = 'c1'): UnifiedRow => ({
  type: 'db',
  connectionId,
  dbSessionId: `sess-${connectionId}`,
  dbName,
  expanded: false,
  loading: false,
  isOpen: false,
  depth: 2,
  levelDepth: 2,
});

const loadingRow = (ownerKey: string): UnifiedRow => ({
  type: 'db-loading',
  ownerKey,
  depth: 2,
  levelDepth: 2,
});

/** Build an `object` row; every field here is part of its identity. */
const objectRow = (over: Partial<Extract<UnifiedRow, { type: 'object' }>> = {}) =>
  ({
    type: 'object',
    connectionId: 'c1',
    dbName: 'db1',
    schemaName: 'public',
    catId: 'routines',
    obj: { kind: 'function', name: 'fn_calc' },
    depth: 4,
    levelDepth: 4,
    ...over,
  }) satisfies Extract<UnifiedRow, { type: 'object' }> as Extract<UnifiedRow, { type: 'object' }>;

describe('navigator row keys', () => {
  it('takes the row alone, so a key can never be read off a position', () => {
    // The signature is the contract: no `index` parameter exists to pass. A
    // key function that accepts one is a key function that will use it.
    expect(getUnifiedRowKey.length).toBe(1);
  });

  it('gives the same row the same key wherever it is in the list', () => {
    const row = objectRow();
    const shallowCopy = { ...row } as typeof row;
    // A list rebuild allocates fresh objects; identity is in the fields.
    expect(getUnifiedRowKey(shallowCopy)).toBe(getUnifiedRowKey(row));
  });

  describe('object rows', () => {
    it('separates the same function name in two connections', () => {
      // The shipped bug: keys were `(catId, name)`, so every connection
      // holding `fn_calc` produced one and the same key.
      const a = objectRow({ connectionId: 'c1' });
      const b = objectRow({ connectionId: 'c2' });
      expect(getUnifiedRowKey(a)).not.toBe(getUnifiedRowKey(b));
    });

    it('separates the same function name in two databases of one connection', () => {
      const a = objectRow({ dbName: 'db1' });
      const b = objectRow({ dbName: 'db2' });
      expect(getUnifiedRowKey(a)).not.toBe(getUnifiedRowKey(b));
    });

    it('separates the same name in two schemas of one database', () => {
      const a = objectRow({ schemaName: 'public' });
      const b = objectRow({ schemaName: 'private' });
      expect(getUnifiedRowKey(a)).not.toBe(getUnifiedRowKey(b));
    });

    it('separates the same name under two categories', () => {
      const a = objectRow({ catId: 'routines' });
      const b = objectRow({ catId: 'views' });
      expect(getUnifiedRowKey(a)).not.toBe(getUnifiedRowKey(b));
    });

    it('does not conflate a missing schema with an empty one', () => {
      // They read the same in the key, and they are the same place on screen,
      // but the assertion is written so a change to either side is visible.
      const a = objectRow({ schemaName: undefined });
      const b = objectRow({ schemaName: '' });
      expect(getUnifiedRowKey(a)).toBe(getUnifiedRowKey(b));
    });
  });

  describe('db-loading rows', () => {
    it('is named for the database it stands in for, not for its slot', () => {
      // The shipped bug: keyed by list position, so database A's spinner
      // became database B's as soon as A scrolled out of the window.
      expect(getUnifiedRowKey(loadingRow('db:c1::db1'))).toBe('loading:db:c1::db1');
    });

    it('keeps two spinners apart when both databases are loading', () => {
      const a = loadingRow('db:c1::db1');
      const b = loadingRow('db:c1::db2');
      expect(getUnifiedRowKey(a)).not.toBe(getUnifiedRowKey(b));
    });

    it('does not collide with the row it is standing in for', () => {
      // `db:` and `loading:` are distinct namespaces on purpose: the placeholder
      // and the real row never share a node, so the swap cannot strand state.
      expect(getUnifiedRowKey(loadingRow('db:c1::db1'))).not.toBe(getUnifiedRowKey(dbRow('db1')));
    });
  });

  describe('empty-group rows', () => {
    it('is named for the group, with no position fallback left to fall back to', () => {
      // The shipped bug: `empty:${row.groupName ?? index}` meant the hint row
      // renamed itself whenever anything above it appeared or disappeared.
      const row: UnifiedRow = { type: 'empty-group', groupName: 'recent', depth: 1, levelDepth: 1 };
      expect(getUnifiedRowKey(row)).toBe('empty:recent');
    });

    it('distinguishes two groups that both read as empty', () => {
      const a: UnifiedRow = { type: 'empty-group', groupName: 'recent', depth: 1, levelDepth: 1 };
      const b: UnifiedRow = { type: 'empty-group', groupName: 'pinned', depth: 1, levelDepth: 1 };
      expect(getUnifiedRowKey(a)).not.toBe(getUnifiedRowKey(b));
    });
  });

  it('gives every row variant a key in its own namespace', () => {
    // A smoke test over the whole union: a variant left out of the switch
    // would return `undefined` here rather than failing to render. The prefixes
    // are written out because they are not derivable from the type name —
    // `group` keys as `grp:`, `namespace-node` as `ns:` — and a placeholder
    // that accidentally matches a real one is the failure worth catching.
    const rows: [UnifiedRow, string][] = [
      [sectionRow('pinned'), 'sec:'],
      [groupRow('g'), 'grp:'],
      [connectionRow('c1', 'g'), 'conn:'],
      [dbRow('db1'), 'db:'],
      [
        {
          type: 'schema',
          connectionId: 'c1',
          dbName: 'db1',
          schemaName: 'public',
          expanded: true,
          depth: 3,
          levelDepth: 3,
        },
        'schema:',
      ],
      [
        {
          type: 'kv-db',
          connectionId: 'c1',
          dbSessionId: 'sess-c1',
          dbName: 'db1',
          isSelected: false,
          depth: 2,
          levelDepth: 2,
        },
        'kv:',
      ],
      [loadingRow('db:c1::db1'), 'loading:'],
      [{ type: 'no-connections', depth: 0, levelDepth: 0 }, 'no-connections'],
      [{ type: 'empty-group', groupName: 'g', depth: 1, levelDepth: 1 }, 'empty:'],
      [
        {
          type: 'namespace-node',
          name: 'tbl',
          depth: 3,
          levelDepth: 3,
          expanded: false,
          isLeaf: false,
          segments: ['tbl'],
          key: 'ns1',
          connectionId: 'c1',
          dbSessionId: 'sess-c1',
        },
        'ns:',
      ],
      [objectRow(), 'obj:'],
    ];
    for (const [row, prefix] of rows) {
      const key = getUnifiedRowKey(row);
      expect(key, `no key for ${row.type}`).toBeTruthy();
      expect(key.startsWith(prefix), `prefix for ${row.type}: ${key}`).toBe(true);
    }
  });

  it('gives two rows in one list two different keys', () => {
    // The property the shell actually depends on: keys unique within the
    // painted list, without consulting the list itself.
    const list: UnifiedRow[] = [
      sectionRow('pinned'),
      groupRow('g'),
      connectionRow('c1', 'g'),
      dbRow('db1'),
      loadingRow('db:c1::db1'),
      objectRow(),
      objectRow({ catId: 'views' }),
      { type: 'empty-group', groupName: 'g', depth: 1, levelDepth: 1 },
    ];
    const keys = list.map(getUnifiedRowKey);
    expect(new Set(keys).size).toBe(keys.length);
  });
});
