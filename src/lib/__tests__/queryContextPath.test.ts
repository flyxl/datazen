import { describe, expect, it } from 'vitest';
import {
  inferSqlRelationPath,
  namespaceRootsFrom,
  resolveQueryContextPath,
  buildPathHierarchyDatabasePin,
  splitPathHierarchyDatabasePin,
  autoCompletePathHierarchyPath,
  buildPathHierarchySelectorSegments,
  pathHierarchySelectorSegmentsForUi,
  pathHierarchyConnectionRoot,
  PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS,
} from '../queryContextPath';
import type { SqlNamespace } from '../sqlNamespace';

describe('inferSqlRelationPath', () => {
  it('reads a MySQL-qualified table', () => {
    expect(
      inferSqlRelationPath(
        'SELECT * FROM trading_dev.t_afi_installment_order ORDER BY id DESC LIMIT 1;',
      ),
    ).toEqual(['trading_dev', 't_afi_installment_order']);
  });

  it('reads a three-level Superset path', () => {
    expect(inferSqlRelationPath('SELECT * FROM hive.snap.orders')).toEqual([
      'hive',
      'snap',
      'orders',
    ]);
  });

  it('handles quoted identifiers', () => {
    expect(inferSqlRelationPath('SELECT * FROM `trading_dev`.`t_order`')).toEqual([
      'trading_dev',
      't_order',
    ]);
  });

  it('returns empty when there is no relation', () => {
    expect(inferSqlRelationPath('SELECT 1')).toEqual([]);
  });
});

describe('resolveQueryContextPath', () => {
  it('switches MySQL selector to the SQL database', () => {
    expect(
      resolveQueryContextPath(
        'SELECT * FROM trading_dev.t_afi_installment_order ORDER BY id DESC LIMIT 1;',
        { databases: ['information_schema', 'trading_dev'], namespaceRoots: [] },
      ),
    ).toEqual(['trading_dev']);
  });

  it('does not treat PostgreSQL schema as a database', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM public.users', {
        databases: ['postgres', 'app'],
        namespaceRoots: [],
      }),
    ).toBeNull();
  });

  it('resolves cascading catalog/schema for path-hierarchy roots', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM hive.snap.orders', {
        databases: [],
        namespaceRoots: ['hive', 'pg'],
      }),
    ).toEqual(['hive', 'snap']);
  });

  it('ignores unqualified tables', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM users', {
        databases: ['trading_dev'],
        namespaceRoots: [],
      }),
    ).toBeNull();
  });

  it('switches as soon as the user types database.', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM trading_dev.', {
        databases: ['information_schema', 'trading_dev'],
        namespaceRoots: [],
      }),
    ).toEqual(['trading_dev']);
  });

  it('resolves long Superset table names', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM hive.snap.afi_id_loan_t_afi_installment_order', {
        databases: ['558:presto_afi_data'],
        namespaceRoots: ['hive'],
      }),
    ).toEqual(['hive', 'snap']);
  });

  it('switches path-hierarchy as soon as catalog. or catalog.schema.', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM hive.', {
        databases: [],
        namespaceRoots: ['hive', 'pg'],
      }),
    ).toEqual(['hive']);
    expect(
      resolveQueryContextPath('SELECT * FROM hive.snap.', {
        databases: [],
        namespaceRoots: ['hive', 'pg'],
      }),
    ).toEqual(['hive', 'snap']);
  });
});

describe('namespaceRootsFrom', () => {
  it('unions aliases, tree keys, and databases', () => {
    expect(namespaceRootsFrom({ hive: {}, pg: {} }, { hive: '1' }, ['extra']).sort()).toEqual([
      'extra',
      'hive',
      'pg',
    ]);
  });
});

describe('buildPathHierarchyDatabasePin', () => {
  it('joins connection root with catalog/schema segments', () => {
    expect(buildPathHierarchyDatabasePin('558:presto_afi_data', ['hive', 'snap'])).toBe(
      '558:presto_afi_data/hive/snap',
    );
  });

  it('returns root when namespace path is empty', () => {
    expect(buildPathHierarchyDatabasePin('558', [])).toBe('558');
  });
});

describe('splitPathHierarchyDatabasePin', () => {
  it('splits fetch paths from table-open context', () => {
    expect(splitPathHierarchyDatabasePin('558/hive/snap')).toEqual({
      root: '558',
      namespacePath: ['hive', 'snap'],
    });
  });
});

const supersetTree: SqlNamespace = {
  hive: { snap: {}, default: {} },
  pg: { public: {} },
};

describe('autoCompletePathHierarchyPath', () => {
  it('auto-picks a single root and single schema', () => {
    expect(autoCompletePathHierarchyPath({ hive: { snap: {} } }, {}, ['558:presto'], [])).toEqual([
      'hive',
      'snap',
    ]);
  });

  it('extends through single-option children after user picks catalog', () => {
    expect(autoCompletePathHierarchyPath({ hive: { snap: {} } }, {}, [], ['hive'])).toEqual([
      'hive',
      'snap',
    ]);
  });

  it('returns null when multiple roots and path is empty', () => {
    expect(autoCompletePathHierarchyPath(supersetTree, {}, [], [])).toBeNull();
  });

  it('returns null when path already complete', () => {
    expect(
      autoCompletePathHierarchyPath({ hive: { snap: {} } }, {}, [], ['hive', 'snap']),
    ).toBeNull();
  });
});

describe('buildPathHierarchySelectorSegments', () => {
  it('renders labels for single-option levels', () => {
    expect(
      buildPathHierarchySelectorSegments({ hive: { snap: {} } }, {}, [], ['hive', 'snap']),
    ).toEqual([
      { kind: 'label', name: 'hive' },
      { kind: 'label', name: 'snap' },
    ]);
  });

  it('shows select when multiple schemas exist', () => {
    expect(
      buildPathHierarchySelectorSegments({ hive: { snap: {}, default: {} } }, {}, [], ['hive']),
    ).toEqual([
      { kind: 'label', name: 'hive' },
      {
        kind: 'select',
        levelIndex: 1,
        options: ['snap', 'default'],
        value: '',
      },
    ]);
  });

  it('shows root select when multiple catalogs exist', () => {
    expect(buildPathHierarchySelectorSegments(supersetTree, {}, [], [])).toEqual([
      {
        kind: 'select',
        levelIndex: 0,
        options: ['hive', 'pg'],
        value: '',
      },
    ]);
  });

  it('shows next-level placeholder select while children are still loading', () => {
    expect(buildPathHierarchySelectorSegments({ hive: {} }, {}, [], ['hive'])).toEqual([
      { kind: 'label', name: 'hive' },
      { kind: 'select', levelIndex: 1, options: [], value: '' },
    ]);
  });
});

describe('pathHierarchyConnectionRoot', () => {
  it('prefers connection database over catalog currentDatabase', () => {
    expect(pathHierarchyConnectionRoot(['558:presto_afi_data'], undefined, 'hive')).toBe(
      '558:presto_afi_data',
    );
  });

  it('uses panel database when it is a known connection db', () => {
    expect(
      pathHierarchyConnectionRoot(['558:presto_afi_data'], '558:presto_afi_data', 'hive'),
    ).toBe('558:presto_afi_data');
  });

  it('falls back to id:name currentDatabase when databases list is not loaded yet', () => {
    expect(pathHierarchyConnectionRoot([], undefined, '558:presto_afi_data')).toBe(
      '558:presto_afi_data',
    );
  });
});

describe('pathHierarchySelectorSegmentsForUi', () => {
  it('falls back to catalog/schema placeholders when tree is empty', () => {
    expect(pathHierarchySelectorSegmentsForUi({}, {}, [], [])).toEqual(
      PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS,
    );
  });
});
