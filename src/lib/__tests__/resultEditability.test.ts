import { describe, expect, it } from 'vitest';
import {
  gridReadOnlyMessageKey,
  isColumnEditable,
  resolveGridEditability,
  resolveResultEditability,
  type GridReadOnlyReason,
  type ServerEditability,
} from '../resultEditability';

/** A verdict the back end would send for a plain single-table result. */
const writable: ServerEditability = {
  writable: true,
  identityColumns: [{ column: 'id', sourceColumn: 'id' }],
  editableColumns: [
    { column: 'id', sourceColumn: 'id' },
    { column: 'total', sourceColumn: 'total' },
  ],
  table: { catalog: null, schema: null, name: 'orders', aliased: false },
  reason: null,
};

/** The same verdict, read-only for a reason the back end named. */
function readOnly(reason: GridReadOnlyReason): ServerEditability {
  return { ...writable, writable: false, identityColumns: [], editableColumns: [], reason };
}

const columns = [{ name: 'id' }, { name: 'total' }];
const rows = [
  [1, 10],
  [2, 20],
];

describe('resolveGridEditability', () => {
  it('keeps a read-only result read-only even when the page data is perfect', () => {
    const verdict = resolveGridEditability(readOnly('multiTable'), rows, columns, 0);

    expect(verdict.writable).toBe(false);
    expect(verdict.writable === false && verdict.reason).toBe('multiTable');
  });

  it('refuses a row whose key value is NULL', () => {
    const page = [[null, 10]];
    const verdict = resolveGridEditability(writable, page, columns, 0);

    expect(verdict.writable).toBe(false);
    expect(verdict.writable === false && verdict.reason).toBe('keyValueNull');
  });

  it('refuses a page that contains the same row identity twice', () => {
    const page = [
      [1, 10],
      [1, 20],
    ];
    const verdict = resolveGridEditability(writable, page, columns, 0);

    expect(verdict.writable).toBe(false);
    expect(verdict.writable === false && verdict.reason).toBe('duplicateIdentity');
  });

  it('refuses a row index that is not on the page instead of throwing', () => {
    const verdict = resolveGridEditability(writable, rows, columns, 99);

    expect(verdict.writable).toBe(false);
    expect(verdict.writable === false && verdict.reason).toBe('rowNotFound');
    expect(resolveGridEditability(writable, rows, columns, -1).writable).toBe(false);
  });

  it('refuses a result the back end called writable but that names no key column', () => {
    const verdict = resolveGridEditability(
      { ...writable, identityColumns: [] },
      rows,
      columns,
      0,
    );

    expect(verdict.writable).toBe(false);
    expect(verdict.writable === false && verdict.reason).toBe('keyNotSelected');
  });

  it('lets a page with unique, non-NULL keys through', () => {
    const verdict = resolveGridEditability(writable, rows, columns, 1);

    expect(verdict.writable).toBe(true);
    expect(verdict.writable === true && verdict.identityColumns).toEqual([
      { column: 'id', sourceColumn: 'id' },
    ]);
  });

  it('combines the two verdicts with AND: either side read-only means read-only', () => {
    // The back end says writable, the page says the key is NULL.
    const nullKey = resolveGridEditability(writable, [[null, 10]], columns, 0);
    expect(nullKey.writable).toBe(false);

    // The back end says read-only, the page data is perfect.
    const serverRefuses = resolveGridEditability(readOnly('view'), rows, columns, 0);
    expect(serverRefuses.writable).toBe(false);
  });
});

describe('resolveResultEditability', () => {
  it('keeps a result editable when the narrow mapping verdict verified it', () => {
    // The narrow host-side mapping check recognises `SELECT * FROM <relation>`
    // and the write path re-verifies it against the live catalog. The origin
    // verdict does not withdraw it.
    const verdict = resolveResultEditability(true, readOnly('unknownOrigin'), rows, columns, 0);

    expect(verdict.writable).toBe(true);
  });

  it('does not let the page overrule a verdict the back end refused', () => {
    const verdict = resolveResultEditability(false, readOnly('aggregate'), rows, columns, 0);

    expect(verdict.writable).toBe(false);
    expect(verdict.writable === false && verdict.reason).toBe('aggregate');
  });

  it('lets the origin verdict grant write-back the narrow check cannot see', () => {
    const verdict = resolveResultEditability(false, writable, rows, columns, 0);

    expect(verdict.writable).toBe(true);
  });
});

describe('isColumnEditable', () => {
  it('only accepts a column that is a straight copy of a base column', () => {
    expect(isColumnEditable(writable, 'total')).toBe(true);
    expect(isColumnEditable(writable, 'doubled')).toBe(false);
    expect(isColumnEditable(readOnly('view'), 'total')).toBe(false);
  });
});

describe('gridReadOnlyMessageKey', () => {
  it('names one i18n key per reason and no visible copy of its own', () => {
    const reasons: GridReadOnlyReason[] = [
      'unknownOrigin',
      'ambiguousOrigin',
      'multiTable',
      'aggregate',
      'distinct',
      'setOperation',
      'derivedSource',
      'view',
      'noPrimaryKey',
      'keyNotSelected',
      'sessionReadOnly',
      'keyValueNull',
      'duplicateIdentity',
      'rowNotFound',
    ];

    for (const reason of reasons) {
      expect(gridReadOnlyMessageKey(reason)).toBe(`query.gridReadOnly.${reason}`);
    }
  });
});
