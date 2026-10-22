import { describe, expect, it } from 'vitest';
import {
  RESULT_ORIGIN_WRITE_BACK_ENABLED,
  resolveRowAddressability,
  gridReadOnlyMessageKey,
  isColumnEditable,
  originChannelWritable,
  resolveGridEditability,
  resolveResultEditability,
  resolveResultWriteBack,
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

  it('does not refuse a row when no locating key column is known', () => {
    // An unreadable catalog must not turn a writable result into a grey one:
    // the narrow write path re-verifies the identity against the live
    // catalog before it writes.
    const verdict = resolveGridEditability({ ...writable, identityColumns: [] }, rows, columns, 0);

    expect(verdict.writable).toBe(true);
    expect(resolveRowAddressability([], rows, columns, 0)).toBeNull();
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

  it('never lets the page overrule a refusal, and never lets a refusal overrule the page', () => {
    // Back end refuses, page is perfect.
    expect(resolveGridEditability(readOnly('view'), rows, columns, 0).writable).toBe(false);
    // Back end allows, page has a NULL key.
    const nullKey = resolveGridEditability(writable, [[null, 10]], columns, 0);
    expect(nullKey.writable).toBe(false);
    expect(nullKey.writable === false && nullKey.reason).toBe('keyValueNull');
  });

  it('reaches the user with the reason the back end gave, not one of its own', () => {
    const verdict = resolveGridEditability(readOnly('sessionReadOnly'), rows, columns, 0);
    expect(verdict.writable === false && verdict.reason).toBe('sessionReadOnly');
  });
});

describe('the release channel of the result grid', () => {
  it('is closed while the new write pipeline is not connected', () => {
    // The origin verdict is the only thing that could open a result the
    // narrow host-side check cannot see (`SELECT id, total FROM t`). The write
    // path that could carry such a write does not exist yet, so the channel
    // must stay shut: open, the grid would claim a cell is editable, the
    // narrow write path would refuse, and the refusal would be silent.
    expect(RESULT_ORIGIN_WRITE_BACK_ENABLED).toBe(false);
    expect(originChannelWritable(writable)).toBe(false);
  });

  it('is unreachable: the origin verdict alone cannot make a result writable', () => {
    const verdict = resolveResultEditability(false, writable, rows, columns, 0);

    expect(verdict.writable).toBe(false);
    expect(resolveResultWriteBack(false, writable)).toBe(false);
  });

  it('does not dilute a refusal: a refused verdict stays refused', () => {
    // Every refusal reason, with the page data as perfect as it gets.
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
    ];
    for (const reason of reasons) {
      const verdict = resolveResultEditability(false, readOnly(reason), rows, columns, 0);
      expect(verdict.writable).toBe(false);
      expect(verdict.writable === false && verdict.reason).toBe(reason);
    }
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
    expect(reasons).toHaveLength(14);
  });
});

describe('the reason codes the user can actually see today', () => {
  it('cannot be produced without a driver that implements the origin contract', () => {
    // The back end can only answer "the driver cannot tell" (unknownOrigin)
    // or "the session is read-only" (sessionReadOnly) while every driver
    // answers Undeterminable. The nine shape reasons need a Determined
    // origin; the three page-level reasons need a writable back-end verdict.
    const producibleWithoutADeterminedOrigin: GridReadOnlyReason[] = [
      'unknownOrigin',
      'sessionReadOnly',
    ];
    expect(producibleWithoutADeterminedOrigin).toHaveLength(2);
  });

  it('keeps the page-level reasons out of reach while the back end refuses', () => {
    // A refused verdict short-circuits before the page is even looked at, so
    // the page-level reasons cannot be produced from a refused verdict.
    const verdict = resolveGridEditability(readOnly('unknownOrigin'), [[null, 10]], columns, 0);
    expect(verdict.writable === false && verdict.reason).toBe('unknownOrigin');
  });
});
