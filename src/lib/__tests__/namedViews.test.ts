/**
 * The named-view helpers (08 §9.2).
 *
 * Two things are pinned here besides the behaviour:
 *
 * - the **key sets** of the record, with the same literals the Rust suite
 *   uses (`modules/src/named_views/tests.rs`). A field
 *   that appears on one side only is a contract break that neither side can
 *   see alone.
 * - the **parity** of the two `FilterCondition` definitions — the port's
 *   (`packages/backend-client`) and the host's (`src/types/filters.ts`). The
 *   port carries the operator as a plain string (it must not know which
 *   operators a dialect declares), so the parity is: the same fields, the
 *   same types, except the operator, which the host narrows.
 */
import { describe, expect, it } from 'vitest';
import type { FilterCondition as PortFilterCondition } from '@datazen/backend-client';
import type { FilterCondition as HostFilterCondition } from '../../types';
import {
  buildNamedViewExport,
  namedViewScopeKey,
  normalizeViewName,
  parseNamedViewExport,
  readViewFilters,
  resolveImportName,
  sortNamedViews,
  viewFromTableState,
  viewNamesEqual,
  viewsForScope,
  type NamedView,
  type NamedViewScope,
} from '../namedViews';

/**
 * The key sets of the serialized record.
 *
 * The same literals the Rust suite pins. A change on one side only fails on
 * that side, which is the point.
 */
const NAMED_VIEW_KEYS =
  'columnOrder,createdAt,filterLogic,filters,id,isDefault,name,scope,sorts,updatedAt,visibleColumns';
const SCOPE_KEYS = 'connectionId,database,schema,table';
const FILTER_KEYS_WITH_CONNECTOR = 'column,connector,operator,value';
const FILTER_KEYS_WITHOUT_CONNECTOR = 'column,operator,value';

const SCOPE: NamedViewScope = {
  connectionId: 'conn-1',
  database: 'app',
  schema: null,
  table: 'orders',
};

const VIEW: NamedView = {
  id: 'view-0001',
  name: 'Open orders',
  scope: SCOPE,
  filters: [
    { column: 'id', operator: 'eq', value: 1, connector: 'or' },
    { column: 'name', operator: 'eq', value: 'x' },
  ],
  filterLogic: 'and',
  sorts: [{ column: 'id', descending: false }],
  visibleColumns: null,
  columnOrder: null,
  isDefault: false,
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: null,
};

function sortedKeys(value: object): string {
  return Object.keys(value).sort().join(',');
}

// ── The parity of the two FilterCondition definitions ────────────────────

/** Whether two types have exactly the same fields. */
type SameKeys<A, B> =
  Exclude<keyof A, keyof B> extends never
    ? Exclude<keyof B, keyof A> extends never
      ? true
      : false
    : false;

/** Whether two types have the same type for one field. */
type SameField<A, B, K extends keyof A & keyof B> = [A[K]] extends [B[K]]
  ? [B[K]] extends [A[K]]
    ? true
    : false
  : false;

/** Whether one type is assignable to another. */
type Assignable<A, B> = [A] extends [B] ? true : false;

type Assert<T extends true> = T;

/**
 * The fields are the same on both sides.
 *
 * The operator is the one field that differs **on purpose**: the port must
 * not know which operators a dialect declares, so it carries a plain string
 * and the host narrows it. Everything else — the column, the value and the
 * connector — has to be the same type, or a saved view would lose a field on
 * the way through the port.
 */
export type FilterConditionFieldsMatch = Assert<SameKeys<HostFilterCondition, PortFilterCondition>>;
export type FilterConditionColumnMatches = Assert<
  SameField<HostFilterCondition, PortFilterCondition, 'column'>
>;
export type FilterConditionValueMatches = Assert<
  SameField<HostFilterCondition, PortFilterCondition, 'value'>
>;
export type FilterConditionConnectorMatches = Assert<
  SameField<HostFilterCondition, PortFilterCondition, 'connector'>
>;
/** The host's operator is a narrowing of the port's plain string. */
export type FilterConditionOperatorNarrows = Assert<
  Assignable<HostFilterCondition['operator'], PortFilterCondition['operator']>
>;

// ── The scope ───────────────────────────────────────────────────────────

describe('the scope of a saved view', () => {
  it('is connection scoped and stable', () => {
    expect(namedViewScopeKey(SCOPE)).toBe(namedViewScopeKey({ ...SCOPE }));
    expect(namedViewScopeKey({ ...SCOPE, connectionId: 'conn-2' })).not.toBe(
      namedViewScopeKey(SCOPE),
    );
    expect(namedViewScopeKey({ ...SCOPE, table: 'users' })).not.toBe(namedViewScopeKey(SCOPE));
  });

  it('tells a missing database from an empty one', () => {
    const missing = namedViewScopeKey({ ...SCOPE, database: null });
    const empty = namedViewScopeKey({ ...SCOPE, database: '' });
    expect(missing).not.toBe(empty);
    expect(missing).not.toBe(namedViewScopeKey(SCOPE));
  });

  it('never carries a runtime session id', () => {
    // The scope is persisted, so it may only name the persistent connection.
    expect(Object.keys(SCOPE).sort().join(',')).toBe(SCOPE_KEYS);
    expect(JSON.stringify(SCOPE)).not.toContain('dbSessionId');
  });
});

// ── The names ───────────────────────────────────────────────────────────

describe('the name of a saved view', () => {
  it('is trimmed and its whitespace runs are collapsed', () => {
    expect(normalizeViewName('  Open   orders  ')).toBe('Open orders');
    expect(normalizeViewName('Open\torders\nnow')).toBe('Open orders now');
  });

  it('is compared case-insensitively', () => {
    expect(viewNamesEqual('Users', 'users')).toBe(true);
    expect(viewNamesEqual('  Open   orders ', 'open orders')).toBe(true);
    expect(viewNamesEqual('Users', 'User')).toBe(false);
  });

  it('gets a suffix when an import carries a name that is taken', () => {
    const taken = new Set(['users']);
    expect(resolveImportName('Orders', taken)).toBe('Orders');
    expect(resolveImportName('Users', taken)).toBe('Users (2)');
    expect(resolveImportName('Users', new Set(['users', 'Users (2)']))).toBe('Users (3)');
  });
});

// ── The view of a table state ───────────────────────────────────────────

describe('the view of a table state', () => {
  it('captures the applied filters, not the draft', () => {
    const applied = [{ column: 'id', operator: 'eq' as const, value: 1 }];
    const draft = [{ column: 'id', operator: 'eq' as const, value: 2 }];
    const view = viewFromTableState({
      scope: SCOPE,
      filters: applied,
      draftFilters: draft,
      filterLogic: 'and',
    });

    expect(view.filters).toEqual(applied);
    expect(view.filters).not.toEqual(draft);
    expect(view.scope).toEqual(SCOPE);
    expect(view.filterLogic).toBe('and');
    expect(view.id).toBe('');
    expect(view.isDefault).toBe(false);
    expect(view.updatedAt).toBeNull();
  });

  it('carries the sorts and the column layout', () => {
    const view = viewFromTableState({
      scope: SCOPE,
      filters: [],
      filterLogic: 'or',
      sorts: [{ column: 'id', descending: true }],
      visibleColumns: ['id'],
      columnOrder: ['id', 'name'],
    });

    expect(view.sorts).toEqual([{ column: 'id', descending: true }]);
    expect(view.visibleColumns).toEqual(['id']);
    expect(view.columnOrder).toEqual(['id', 'name']);
  });
});

// ── The list ────────────────────────────────────────────────────────────

describe('the list of saved views', () => {
  it('holds the default first and then the names in dictionary order', () => {
    const views: NamedView[] = [
      { ...VIEW, id: 'a', name: 'Zulu' },
      { ...VIEW, id: 'b', name: 'alpha' },
      { ...VIEW, id: 'c', name: 'Mike' },
      { ...VIEW, id: 'd', name: 'Default one', isDefault: true },
    ];
    expect(sortNamedViews(views).map((view) => view.name)).toEqual([
      'Default one',
      'alpha',
      'Mike',
      'Zulu',
    ]);
  });

  it('holds the views of one scope only', () => {
    const other: NamedView = { ...VIEW, id: 'e', scope: { ...SCOPE, table: 'users' } };
    expect(viewsForScope([VIEW, other], SCOPE).map((view) => view.id)).toEqual(['view-0001']);
  });
});

// ── The filters of a view ───────────────────────────────────────────────

describe('the filters of a saved view', () => {
  it('keeps the per-condition connector', () => {
    const readout = readViewFilters(VIEW.filters);
    expect(readout.unknownOperators).toEqual([]);
    expect(readout.filters[0].connector).toBe('or');
    expect(readout.filters[1].connector).toBeUndefined();
  });

  it('refuses an operator the host does not know instead of dropping it', () => {
    const readout = readViewFilters([{ column: 'id', operator: 'matchesRegex', value: 1 }]);
    expect(readout.unknownOperators).toEqual(['matchesRegex']);
    expect(readout.filters).toEqual([]);
  });
});

// ── The export file ─────────────────────────────────────────────────────

describe('the export file', () => {
  it('survives a round trip', () => {
    const file = buildNamedViewExport([VIEW], new Date('2026-01-02T03:04:05.000Z'));
    expect(file.version).toBe(1);
    expect(file.exportedAt).toBe('2026-01-02T03:04:05.000Z');

    const parsed = parseNamedViewExport(JSON.stringify(file));
    expect(parsed.views).toEqual([VIEW]);
    expect(parsed.version).toBe(1);
  });

  it('refuses a payload that does not hold an export', () => {
    expect(() => parseNamedViewExport('not json')).toThrow(/JSON/);
    expect(() => parseNamedViewExport('[]')).toThrow(/saved-view export/);
    expect(() => parseNamedViewExport('{}')).toThrow(/version/);
    expect(() => parseNamedViewExport('{"version":2,"views":[]}')).toThrow(/version/);
    expect(() => parseNamedViewExport('{"version":1,"views":{}}')).toThrow(/list of views/);
  });
});

// ── The shape the host writes ───────────────────────────────────────────

describe('the record on the wire', () => {
  it('has exactly the fields the host writes', () => {
    // The same literals the Rust suite pins: a field that appears on one
    // side only fails on that side.
    expect(sortedKeys(VIEW)).toBe(NAMED_VIEW_KEYS);
    expect(sortedKeys(VIEW.scope)).toBe(SCOPE_KEYS);
    expect(sortedKeys(VIEW.filters[0])).toBe(FILTER_KEYS_WITH_CONNECTOR);
    expect(sortedKeys(VIEW.filters[1])).toBe(FILTER_KEYS_WITHOUT_CONNECTOR);
  });

  it('carries an update time that may be absent', () => {
    // The host's field is an optional timestamp, so the wire carries `null`
    // for "never updated" — and the type says exactly that.
    expect(VIEW.updatedAt).toBeNull();
  });
});
