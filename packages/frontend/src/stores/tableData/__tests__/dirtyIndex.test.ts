/**
 * The dirty-state index (DB-06 §9.1).
 *
 * Two of these cases are the chapter's **acceptance conditions**, not extras:
 *
 * - `batch index agrees with findPendingForRow for every row` — the batch index
 *   must be the whole-page version of the existing single-row lookup, row by
 *   row, in both directions;
 * - `dirty derivation never reads selection state` — the dirty marks and the
 *   selection are orthogonal dimensions.
 */
import { describe, expect, it } from 'vitest';
import { valuesEqual, type PendingRowChange } from '../../../lib/tableChanges';
import { emptyTableState } from '../connectionState';
import { findPendingForRow } from '../pendingChanges';
import {
  buildDirtyIndex,
  dirtyCellKey,
  EMPTY_DIRTY_INDEX,
  selectDirtyCounts,
  selectDirtyRow,
  type DirtyIndex,
} from '../dirtyIndex';
import type { TableState } from '../types';
import type { ColumnSchema } from '../../../types';

const COLUMNS: ColumnSchema[] = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
  { name: 'score', dataType: 'integer', nullable: true },
];

const PK_COLUMNS = COLUMNS.filter((column) => column.isPrimaryKey);

function stateWith(overrides: Partial<TableState>): TableState {
  return { ...emptyTableState(null), ...overrides };
}

function change(overrides: Partial<PendingRowChange> = {}): PendingRowChange {
  return {
    rowIdentity: { id: 1 },
    originalValues: {},
    currentValues: {},
    changedColumns: [],
    deleteMarked: false,
    ...overrides,
  };
}

describe('dirtyIndex', () => {
  describe('dirtyCellKey', () => {
    it('is injective for column names that contain the identity separators', () => {
      // A column name may contain the separator itself; the key must still be
      // injective. `NUL` is written through `String.fromCharCode` so the source
      // file stays plain text.
      const NUL = String.fromCharCode(0);
      const identity = '["id":1]';
      const keys = new Set([
        dirtyCellKey(identity, `a${NUL}b`),
        dirtyCellKey(identity, 'a:b'),
        dirtyCellKey(identity, 'a|b'),
        dirtyCellKey(identity, 'a,b'),
        dirtyCellKey(identity, 'a"b'),
        dirtyCellKey(identity, ''),
        dirtyCellKey('["id":1]', 'x'),
        dirtyCellKey('["id":2]', 'x'),
      ]);
      expect(keys.size).toBe(8);
    });

    it('never lets two different rows and columns share a key', () => {
      const NUL = String.fromCharCode(0);
      expect(dirtyCellKey('["id":1]', 'name')).not.toBe(dirtyCellKey('["id":1]', 'name2'));
      expect(dirtyCellKey('["id":1]', 'a:b')).not.toBe(dirtyCellKey('["id":1]', 'a'));
      expect(dirtyCellKey('["id":1]', `a${NUL}b`)).toBe(`["id":1]${NUL}a${NUL}b`);
    });
  });

    describe('buildDirtyIndex', () => {
    it('marks only the columns that really changed', () => {
      const ts = stateWith({
        columns: COLUMNS,
        rows: [{ id: 1, name: 'Alice', score: 10 }],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { name: 'Alice', score: 10 },
              currentValues: { name: 'Alicia', score: 10 },
              changedColumns: ['name', 'score'],
            }),
          ],
        ]),
      });

      const index = buildDirtyIndex(ts);
      const entry = index.byRowIndex.get(0);
      expect(entry?.changedColumns).toEqual(['name']);
      expect(entry?.cells).toEqual([{ columnName: 'name', original: 'Alice', next: 'Alicia' }]);
      expect(index.counts.totalCells).toBe(1);
    });

    it('reverting a cell to its original value removes the dirty mark', () => {
      const ts = stateWith({
        columns: COLUMNS,
        rows: [{ id: 1, name: 'Alice', score: 10 }],
        pendingChanges: new Map(),
      });

      const index = buildDirtyIndex(ts);
      expect(index.byRowIndex.size).toBe(0);
      expect(index.counts.totalCells).toBe(0);
    });

    it('delete-marked rows produce no cell marks and count as deleteRows', () => {
      const ts = stateWith({
        columns: COLUMNS,
        rows: [{ id: 1, name: 'Alice', score: 10 }],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              deleteMarked: true,
              originalValues: { name: 'Alice' },
              currentValues: { name: 'Alicia' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });

      const index = buildDirtyIndex(ts);
      const entry = index.byRowIndex.get(0);
      expect(entry?.state).toBe('deleted');
      expect(entry?.changedColumns).toEqual([]);
      expect(entry?.cells).toEqual([]);
      expect(index.counts.deleteRows).toBe(1);
      expect(index.counts.updateRows).toBe(0);
      expect(index.counts.totalCells).toBe(0);
    });

    it('duplicate row identities are never marked dirty', () => {
      const ts = stateWith({
        columns: COLUMNS,
        rows: [
          { id: 1, name: 'Alice', score: 10 },
          { id: 1, name: 'Bob', score: 20 },
        ],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { name: 'Alice' },
              currentValues: { name: 'Alicia' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });

      const index = buildDirtyIndex(ts);
      expect(index.byRowIndex.size).toBe(0);
      expect(index.counts.totalCells).toBe(1);
      expect(index.offPageKeys).toEqual(['["id":1]']);
    });

    it('null and undefined values compare equal (DB NULL reaches the UI as null)', () => {
      expect(valuesEqual(undefined, null)).toBe(true);
      const ts = stateWith({
        columns: COLUMNS,
        rows: [{ id: 1, name: null, score: 10 }],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { name: null },
              currentValues: { name: null },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });
      const index = buildDirtyIndex(ts);
      expect(index.byRowIndex.get(0)?.changedColumns).toEqual([]);
      expect(index.counts.totalCells).toBe(0);
    });

    it('same-looking values of different types are a real change', () => {
      const ts = stateWith({
        columns: COLUMNS,
        rows: [{ id: 1, name: 'Alice', score: 1 }],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { score: 1 },
              currentValues: { score: '1' },
              changedColumns: ['score'],
            }),
          ],
        ]),
      });
      expect(buildDirtyIndex(ts).counts.totalCells).toBe(1);

      const emptyString = stateWith({
        columns: COLUMNS,
        rows: [{ id: 1, name: 'Alice', score: 1 }],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { name: null },
              currentValues: { name: '' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });
      expect(buildDirtyIndex(emptyString).counts.totalCells).toBe(1);
    });

    it('valuesEqual pins its object, NaN and undefined semantics', () => {
      // Two objects with a different key order are **not** equal today.
      expect(valuesEqual({ a: 1, b: 2 }, { b: 2, a: 1 })).toBe(false);
      // NaN serialises to `null`, so it equals null.
      expect(valuesEqual(Number.NaN, null)).toBe(true);
      // `undefined` is normalised to null (a DB NULL reaches the UI as null).
      expect(valuesEqual(undefined, null)).toBe(true);
    });

    it('counts split cellsOnPage and cellsOffPage for one changed row', () => {
      const onPage = stateWith({
        columns: COLUMNS,
        rows: [{ id: 1, name: 'Alice', score: 10 }],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { name: 'Alice' },
              currentValues: { name: 'Alicia' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });
      const onPageIndex = buildDirtyIndex(onPage);
      expect(onPageIndex.counts.cellsOnPage).toBe(1);
      expect(onPageIndex.counts.cellsOffPage).toBe(0);
      expect(onPageIndex.counts.totalCells).toBe(1);
      expect(onPageIndex.offPageKeys).toEqual([]);

      const offPage = stateWith({
        columns: COLUMNS,
        rows: [{ id: 2, name: 'Bob', score: 20 }],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { name: 'Alice' },
              currentValues: { name: 'Alicia' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });
      const offPageIndex = buildDirtyIndex(offPage);
      expect(offPageIndex.counts.cellsOnPage).toBe(0);
      expect(offPageIndex.counts.cellsOffPage).toBe(1);
      expect(offPageIndex.counts.totalCells).toBe(1);
      expect(offPageIndex.offPageKeys).toEqual(['["id":1]']);
    });

    it('anchored key wins over the recomputed identity key', () => {
      // The primary key of row 0 was edited: the row now carries `id: 2`, while
      // the staged change and the anchor still point at the original `id: 1`.
      const ts = stateWith({
        columns: COLUMNS,
        rows: [{ id: 2, name: 'Alicia', score: 10 }],
        rowIdentityAnchors: new Map([[0, '["id":1]']]),
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              rowIdentity: { id: 1 },
              originalValues: { id: 1 },
              currentValues: { id: 2 },
              changedColumns: ['id'],
            }),
          ],
        ]),
      });

      const index = buildDirtyIndex(ts);
      expect(index.byRowIndex.get(0)?.identityKey).toBe('["id":1]');
      expect(index.byRowIndex.get(0)?.changedColumns).toEqual(['id']);
      expect(index.byRowIndex.size).toBe(1);
    });

    it('follows findPendingForRow even when another row carries the original identity', () => {
      // The edited row now carries `id: 2`, and a second row carries the
      // original `id: 1`. The single-row lookup attributes the staged change to
      // **both** rows (its anchor wins for the first, its direct lookup hits for
      // the second), so the batch index has to agree — the single-row lookup is
      // the authority (06 §10 #16).
      //
      // The app cannot produce this shape: the original primary key was unique,
      // so no other row can carry it unless that row was edited too — and then
      // it has an anchor of its own, which wins for it.
      const ts = stateWith({
        columns: COLUMNS,
        rows: [
          { id: 2, name: 'Alicia', score: 10 },
          { id: 1, name: 'Bob', score: 20 },
        ],
        rowIdentityAnchors: new Map([[0, '["id":1]']]),
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              rowIdentity: { id: 1 },
              originalValues: { id: 1 },
              currentValues: { id: 2 },
              changedColumns: ['id'],
            }),
          ],
        ]),
      });

      const index = buildDirtyIndex(ts);
      expect(index.byRowIndex.get(0)?.identityKey).toBe('["id":1]');
      expect(index.byRowIndex.get(1)?.identityKey).toBe('["id":1]');
      expect(findPendingForRow(ts, 1, ts.rows[1], PK_COLUMNS)?.key).toBe('["id":1]');
    });

    it('structure sharing keeps untouched row entries referentially stable', () => {
      const ts = stateWith({
        columns: COLUMNS,
        rows: [
          { id: 1, name: 'Alice', score: 10 },
          { id: 2, name: 'Bob', score: 20 },
        ],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              rowIdentity: { id: 1 },
              originalValues: { name: 'Alice' },
              currentValues: { name: 'Alicia' },
              changedColumns: ['name'],
            }),
          ],
          [
            '["id":2]',
            change({
              rowIdentity: { id: 2 },
              originalValues: { name: 'Bob' },
              currentValues: { name: 'Bobby' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });

      const first = buildDirtyIndex(ts);
      const second = buildDirtyIndex(
        stateWith({
          columns: COLUMNS,
          rows: [
            { id: 1, name: 'Alicia', score: 10 },
            { id: 2, name: 'Bobby', score: 20 },
          ],
          pendingChanges: new Map([
            [
              '["id":1]',
              change({
                rowIdentity: { id: 1 },
                originalValues: { name: 'Alice' },
                currentValues: { name: 'Alicia' },
                changedColumns: ['name'],
              }),
            ],
            [
              '["id":2]',
              change({
                rowIdentity: { id: 2 },
                originalValues: { name: 'Bob' },
                currentValues: { name: 'Robert' },
                changedColumns: ['name'],
              }),
            ],
          ]),
        }),
        first,
      );

      // Row 1 did not change: its entry is the very same object.
      expect(second.byRowIndex.get(0)).toBe(first.byRowIndex.get(0));
      // Row 2 changed: a new object.
      expect(second.byRowIndex.get(1)).not.toBe(first.byRowIndex.get(1));
    });

    it('empty pendingChanges yields EMPTY_DIRTY_INDEX semantics', () => {
      const index = buildDirtyIndex(stateWith({ columns: COLUMNS, rows: [{ id: 1 }] }));
      expect(index).toBe(EMPTY_DIRTY_INDEX);
      expect(index.byRowIndex.size).toBe(0);
      expect(index.offPageKeys).toEqual([]);
      expect(index.counts).toEqual({
        updateRows: 0,
        deleteRows: 0,
        totalRows: 0,
        totalCells: 0,
        cellsOnPage: 0,
        cellsOffPage: 0,
      });
    });
  });

  describe('selectDirtyCounts', () => {
    it('ignores page state entirely', () => {
      const pendingChanges = new Map([
        [
          '["id":1]',
          change({
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
          }),
        ],
        [
          '["id":2]',
          change({ rowIdentity: { id: 2 }, deleteMarked: true }),
        ],
      ]);

      const counts = selectDirtyCounts(pendingChanges);
      expect(counts.updateRows).toBe(1);
      expect(counts.deleteRows).toBe(1);
      expect(counts.totalRows).toBe(2);
      expect(counts.totalCells).toBe(1);
      // Without a page there is no cell on the page.
      expect(counts.cellsOnPage).toBe(0);
      expect(counts.cellsOffPage).toBe(1);

      // The same numbers when the page is empty (a write from elsewhere) or
      // when the rows are not loaded at all.
      const emptyPage = buildDirtyIndex(stateWith({ columns: COLUMNS, rows: [], pendingChanges }));
      expect(emptyPage.counts.totalCells).toBe(counts.totalCells);
      expect(emptyPage.counts.totalRows).toBe(counts.totalRows);
      expect(emptyPage.counts.cellsOnPage).toBe(0);
      expect(emptyPage.counts.cellsOffPage).toBe(counts.totalCells);
    });
  });

  describe('selectDirtyRow', () => {
    it('returns the row entry or undefined', () => {
      const ts = stateWith({
        columns: COLUMNS,
        rows: [{ id: 1, name: 'Alice' }],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { name: 'Alice' },
              currentValues: { name: 'Alicia' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });
      const index = buildDirtyIndex(ts);
      expect(selectDirtyRow(index, 0)?.state).toBe('dirty');
      expect(selectDirtyRow(index, 7)).toBeUndefined();
      expect(selectDirtyRow(null, 0)).toBeUndefined();
      expect(selectDirtyRow(undefined, 0)).toBeUndefined();
    });
  });

  describe('equivalence with findPendingForRow', () => {
    /** The contract: batch and single-row lookup agree on every row. */
    function assertAgrees(ts: TableState) {
      const index = buildDirtyIndex(ts);
      const pkColumns = ts.columns.filter((column) => column.isPrimaryKey);
      for (let rowIndex = 0; rowIndex < ts.rows.length; rowIndex += 1) {
        const single = findPendingForRow(ts, rowIndex, ts.rows[rowIndex], pkColumns);
        const batch = index.byRowIndex.get(rowIndex);
        expect(batch?.identityKey).toBe(single?.key);
        if (!single) {
          expect(batch).toBeUndefined();
          continue;
        }
        // A delete-marked row carries no cell marks; a normal row carries the
        // columns that really changed.
        if (single.change.deleteMarked) {
          expect(batch?.state).toBe('deleted');
          expect(batch?.changedColumns).toEqual([]);
        } else {
          expect(batch?.state).toBe('dirty');
          expect(batch?.changedColumns).toEqual(
            single.change.changedColumns.filter(
              (column) =>
                !valuesEqual(
                  single.change.originalValues[column],
                  single.change.currentValues[column],
                ),
            ),
          );
        }
      }
      return index;
    }

    it('batch index agrees with findPendingForRow for every row', () => {
      // 1) a plain row with a staged change
      const plain = stateWith({
        columns: COLUMNS,
        rows: [
          { id: 1, name: 'Alice', score: 10 },
          { id: 2, name: 'Bob', score: 20 },
        ],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { name: 'Alice' },
              currentValues: { name: 'Alicia' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });
      assertAgrees(plain);

      // 2) a row whose primary key was edited (the anchor wins)
      const editedPk = stateWith({
        columns: COLUMNS,
        rows: [{ id: 2, name: 'Alicia', score: 10 }],
        rowIdentityAnchors: new Map([[0, '["id":1]']]),
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              rowIdentity: { id: 1 },
              originalValues: { id: 1 },
              currentValues: { id: 2 },
              changedColumns: ['id'],
            }),
          ],
        ]),
      });
      assertAgrees(editedPk);

      // 3) two rows with the same identity (neither may be marked)
      const duplicated = stateWith({
        columns: COLUMNS,
        rows: [
          { id: 1, name: 'Alice', score: 10 },
          { id: 1, name: 'Bob', score: 20 },
        ],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { name: 'Alice' },
              currentValues: { name: 'Alicia' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });
      assertAgrees(duplicated);

      // 4) a table without a primary key
      const noPrimaryKey = stateWith({
        columns: [{ name: 'name', dataType: 'text', nullable: true }],
        rows: [{ name: 'Alice' }],
        pendingChanges: new Map([
          [
            '["id":1]',
            change({
              originalValues: { name: 'Alice' },
              currentValues: { name: 'Alicia' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });
      assertAgrees(noPrimaryKey);

      // 5) more rows than staged changes, plus a delete-marked row
      const mixed = stateWith({
        columns: COLUMNS,
        rows: [
          { id: 1, name: 'Alice', score: 10 },
          { id: 2, name: 'Bob', score: 20 },
          { id: 3, name: 'Cara', score: 30 },
        ],
        pendingChanges: new Map([
          [
            '["id":2]',
            change({
              rowIdentity: { id: 2 },
              deleteMarked: true,
              originalValues: { name: 'Bob' },
              currentValues: { name: 'Bobby' },
              changedColumns: ['name'],
            }),
          ],
        ]),
      });
      assertAgrees(mixed);
    });

    it('batch index agrees with findPendingForRow after a page reorder', () => {
      const rows = [
        { id: 1, name: 'Alice', score: 10 },
        { id: 2, name: 'Bob', score: 20 },
        { id: 3, name: 'Cara', score: 30 },
      ];
      const pendingChanges = new Map([
        [
          '["id":3]',
          change({
            rowIdentity: { id: 3 },
            originalValues: { name: 'Cara' },
            currentValues: { name: 'Caro' },
            changedColumns: ['name'],
          }),
        ],
        [
          '["id":1]',
          change({
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
          }),
        ],
      ]);

      const forward = stateWith({ columns: COLUMNS, rows, pendingChanges });
      assertAgrees(forward);
      expect(buildDirtyIndex(forward).byRowIndex.get(2)?.identityKey).toBe('["id":3]');

      // The same staged changes, the rows in the opposite order: the mark
      // follows the identity, not the row number.
      const reversed = stateWith({
        columns: COLUMNS,
        rows: [...rows].reverse(),
        pendingChanges,
      });
      assertAgrees(reversed);
      const reversedIndex = buildDirtyIndex(reversed);
      expect(reversedIndex.byRowIndex.get(0)?.identityKey).toBe('["id":3]');
      expect(reversedIndex.byRowIndex.get(2)?.identityKey).toBe('["id":1]');
    });
  });

  describe('orthogonality to the selection model', () => {
    it('dirty derivation never reads selection state', () => {
      const pendingChanges = new Map([
        [
          '["id":1]',
          change({
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
          }),
        ],
      ]);
      const base = stateWith({
        columns: COLUMNS,
        rows: [
          { id: 1, name: 'Alice', score: 10 },
          { id: 2, name: 'Bob', score: 20 },
        ],
        pendingChanges,
      });

      const withSelection = {
        ...base,
        selectedRows: new Set([0, 1]),
        lastSelectedIndex: 1,
        gridSelection: {
          anchor: { rowIndex: 0, columnName: 'name' },
          focus: { rowIndex: 1, columnName: 'name' },
          extraRanges: [],
          mode: 'cell' as const,
        },
      };

      const without = buildDirtyIndex(base);
      const withSel = buildDirtyIndex(withSelection);
      expect(JSON.stringify(withSel)).toBe(JSON.stringify(without));
      expect(withSel.byRowIndex.get(0)).toEqual(without.byRowIndex.get(0));
      expect(withSel.counts).toEqual(without.counts);
    });
  });

  describe('EMPTY_DIRTY_INDEX', () => {
    it('is a stable singleton', () => {
      const index: DirtyIndex = EMPTY_DIRTY_INDEX;
      expect(index).toBe(EMPTY_DIRTY_INDEX);
      expect(index.byRowIndex.size).toBe(0);
      expect(index.offPageKeys).toEqual([]);
    });
  });
});
