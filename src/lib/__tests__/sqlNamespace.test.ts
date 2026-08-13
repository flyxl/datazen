import { describe, expect, it } from 'vitest';
import {
  isSchemaGroupingSchema,
  mergeNamespacePath,
  overlayColumnMap,
  pathKey,
  namespaceHasChild,
  namespaceChildNames,
  namespaceBranchChildNames,
} from '../sqlNamespace';

describe('isSchemaGroupingSchema', () => {
  it('rejects nullish and path-nav sentinels', () => {
    expect(isSchemaGroupingSchema(null)).toBe(false);
    expect(isSchemaGroupingSchema(undefined)).toBe(false);
    expect(isSchemaGroupingSchema('CATALOG')).toBe(false);
    expect(isSchemaGroupingSchema('SCHEMA')).toBe(false);
  });

  it('accepts real schema labels', () => {
    expect(isSchemaGroupingSchema('public')).toBe(true);
    expect(isSchemaGroupingSchema('dbo')).toBe(true);
  });
});

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
    const tree = mergeNamespacePath(
      { db: { hive: { snap: {} } } },
      ['db', 'hive', 'snap'],
      'tables',
      ['t1', 't2'],
    );
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

describe('namespaceChildNames', () => {
  it('lists keys including table-leaf children', () => {
    const tree = { hive: { snap: { orders: [] } }, pg: {} };
    expect(namespaceChildNames(tree, []).sort()).toEqual(['hive', 'pg']);
    expect(namespaceChildNames(tree, ['hive'])).toEqual(['snap']);
    expect(namespaceChildNames(tree, ['hive', 'snap'])).toEqual(['orders']);
  });
});

describe('namespaceBranchChildNames', () => {
  it('skips table-leaf children so selectors stop at schema', () => {
    const tree = { hive: { snap: { orders: [] } }, pg: {} };
    expect(namespaceBranchChildNames(tree, ['hive', 'snap'])).toEqual([]);
    expect(namespaceBranchChildNames(tree, ['hive'])).toEqual(['snap']);
  });
});
