/**
 * DB-03 §9.2 + A2 — the batch write path.
 *
 * The whole point of this file is the **equivalence pin**: a paste is not allowed to be a
 * second write implementation. Whatever a single `stageCellChange` produces for one cell, one
 * `stagePasteWrites` with that same cell must produce too — same `rows`, same
 * `pendingChanges`, same `rowIdentityAnchors` — or the two paths have silently diverged.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TableState } from '../types';
import type { CellPasteWrite } from '../../../lib/tableChanges';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));

const sampleColumns = [
  { name: 'id', dataType: 'integer', isPrimaryKey: true, nullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, nullable: true },
  { name: 'score', dataType: 'integer', isPrimaryKey: false, nullable: true },
];

const sampleResponse = {
  columns: sampleColumns,
  rows: [
    [1, 'Alice', 10],
    [2, 'Bob', 20],
    [3, 'Cara', 30],
  ] as (string | number | null)[][],
  totalRows: 3,
  page: 0,
  pageSize: 50,
};

const WHO_PANEL = 'clip-a';
const PASTE_PANEL = 'clip-b';
const NO_PK_PANEL = 'clip-no-pk';

describe('stagePasteWrites (batch path of the clipboard)', () => {
  let useTableDataStore: typeof import('../../tableDataStore').useTableDataStore;

  const slice = (panelId: string): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(panelId);
    if (!ts) throw new Error(`panel ${panelId} has no slice`);
    return ts;
  };

  const load = async (panelId: string) => {
    await useTableDataStore.getState().loadTableData({
      panelId,
      dbSessionId: 'conn-1',
      table: 'scores',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
  };

  /** Everything the two paths must agree on, normalised for a deep-equal comparison. */
  const comparable = (ts: TableState) => ({
    rows: ts.rows,
    pendingChanges: [...ts.pendingChanges.entries()],
    rowIdentityAnchors: [...ts.rowIdentityAnchors.entries()],
  });

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(sampleResponse);
    mockDatabaseCommands.previewPendingChanges.mockResolvedValue(null);
    mockDatabaseCommands.commitPendingChanges.mockResolvedValue({});
    const mod = await import('../../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    await load(WHO_PANEL);
    await load(PASTE_PANEL);
  });

  it('produces exactly what a single stageCellChange produces', () => {
    useTableDataStore.getState().stageCellChange(WHO_PANEL, 1, 'name', 'Bobby');
    const single = comparable(slice(WHO_PANEL));

    const result = useTableDataStore
      .getState()
      .stagePasteWrites(PASTE_PANEL, [{ rowIndex: 1, columnName: 'name', value: 'Bobby' }]);
    const batch = comparable(slice(PASTE_PANEL));

    expect(result.staged).toBe(1);
    expect(result.problems).toEqual([]);
    expect(batch).toEqual(single);
  });

  it('agrees with stageCellChange on a revert (the pending entry disappears)', () => {
    useTableDataStore.getState().stageCellChange(WHO_PANEL, 0, 'name', 'Alicia');
    useTableDataStore.getState().stageCellChange(WHO_PANEL, 0, 'name', 'Alice');

    useTableDataStore
      .getState()
      .stagePasteWrites(PASTE_PANEL, [{ rowIndex: 0, columnName: 'name', value: 'Alicia' }]);
    const revert = useTableDataStore
      .getState()
      .stagePasteWrites(PASTE_PANEL, [{ rowIndex: 0, columnName: 'name', value: 'Alice' }]);

    expect(revert.staged).toBe(0);
    expect(comparable(slice(PASTE_PANEL))).toEqual(comparable(slice(WHO_PANEL)));
  });

  it('agrees with stageCellChange on writing an explicit NULL', () => {
    useTableDataStore.getState().stageCellChange(WHO_PANEL, 0, 'name', null);
    useTableDataStore
      .getState()
      .stagePasteWrites(PASTE_PANEL, [{ rowIndex: 0, columnName: 'name', value: null }]);
    expect(comparable(slice(PASTE_PANEL))).toEqual(comparable(slice(WHO_PANEL)));
  });

  it('agrees with stageCellChange when two columns of one row are written', () => {
    useTableDataStore.getState().stageCellChange(WHO_PANEL, 2, 'name', 'Cara!');
    useTableDataStore.getState().stageCellChange(WHO_PANEL, 2, 'score', 31);
    useTableDataStore.getState().stagePasteWrites(PASTE_PANEL, [
      { rowIndex: 2, columnName: 'name', value: 'Cara!' },
      { rowIndex: 2, columnName: 'score', value: 31 },
    ]);
    expect(comparable(slice(PASTE_PANEL))).toEqual(comparable(slice(WHO_PANEL)));
    expect(slice(PASTE_PANEL).pendingChanges.size).toBe(1);
  });

  /**
   * Differential test over generated write sequences.
   *
   * The four cases above are hand-picked, and hand-picked cases cannot defend an *equivalence
   * claim between two implementations*: they pin the branches someone thought of. The batch
   * path and the single-cell path keep separate copies of the same guard order, so the way
   * this actually breaks is "one copy was edited, the other was not" — and that shows up in
   * whichever combination nobody wrote down.
   *
   * Seeded on purpose: the sequence is reproducible from the seed alone, so a failure is a
   * bug report rather than a flake.
   */
  it('agrees with stageCellChange on generated write sequences (seeded differential)', async () => {
    // Park–Miller LCG. A fixed seed makes the run reproducible; the exact generator does not
    // matter as long as it is stable across runs and platforms.
    let state = 0x5eed_0300 >>> 0;
    const random = () => {
      state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
      return state / 0x1_0000_0000;
    };
    const pick = <T,>(items: readonly T[]): T => items[Math.floor(random() * items.length)] as T;

    const columnNames = ['id', 'name', 'score'];
    // The pool deliberately mixes: the original values, `null`, the empty string, values of
    // the wrong type for the column, and writes that move a primary key (which must be
    // refused by both paths, and refused *identically*).
    const values: CellPasteWrite['value'][] = [null, '', 'Alice', 'Bob', 'x', 10, 20, 30, 99, 0];

    for (let round = 0; round < 200; round += 1) {
      await load(WHO_PANEL);
      await load(PASTE_PANEL);

      const writes: CellPasteWrite[] = [];
      const opCount = 1 + Math.floor(random() * 6);
      for (let index = 0; index < opCount; index += 1) {
        writes.push({
          rowIndex: Math.floor(random() * 3),
          columnName: pick(columnNames),
          value: pick(values),
        });
      }

      for (const write of writes) {
        useTableDataStore
          .getState()
          .stageCellChange(WHO_PANEL, write.rowIndex, write.columnName, write.value);
      }
      useTableDataStore.getState().stagePasteWrites(PASTE_PANEL, writes);

      expect({ round, writes, state: comparable(slice(PASTE_PANEL)) }).toEqual({
        round,
        writes,
        state: comparable(slice(WHO_PANEL)),
      });
    }
  });

  it('returns the state by reference for an empty write list', () => {
    const before = slice(PASTE_PANEL);
    const result = useTableDataStore.getState().stagePasteWrites(PASTE_PANEL, []);
    expect(result.staged).toBe(0);
    expect(result.problems).toEqual([]);
    // No new state object ⇒ no re-render for an empty paste.
    expect(slice(PASTE_PANEL)).toBe(before);
  });

  it('writes 5 000 cells with a single state update', () => {
    const width = 2; // name + score
    const writes = [] as { rowIndex: number; columnName: string; value: string }[];
    for (let index = 0; index < 2500; index += 1) {
      writes.push({ rowIndex: index % 3, columnName: 'name', value: `n${index}` });
      writes.push({ rowIndex: index % 3, columnName: 'score', value: String(index) });
    }
    expect(writes.length).toBe(5000);

    let notifications = 0;
    const unsubscribe = useTableDataStore.subscribe(() => {
      notifications += 1;
    });
    const result = useTableDataStore.getState().stagePasteWrites(PASTE_PANEL, writes);
    unsubscribe();

    // Every one of the 5 000 writes really changed a value, and the whole batch cost **one**
    // store notification — that is what makes a large paste usable.
    expect(result.staged).toBe(5000);
    expect(notifications).toBe(1);
    expect(width).toBe(2);
  });

  it('skips one ambiguous row and still writes the rest', () => {
    // Two rows can be made ambiguous by giving them the same identity through a paste of the
    // primary key; the other 2 rows must not be lost.
    const result = useTableDataStore.getState().stagePasteWrites(PASTE_PANEL, [
      { rowIndex: 0, columnName: 'name', value: 'A' },
      { rowIndex: 1, columnName: 'id', value: 1 }, // collides with row 0's identity
      { rowIndex: 2, columnName: 'name', value: 'C' },
    ]);

    expect(result.staged).toBe(2);
    expect(result.problems.map((problem) => problem.code)).toEqual(['grid.paste.identityChanged']);
    const ts = slice(PASTE_PANEL);
    expect(ts.rows[0].name).toBe('A');
    expect(ts.rows[2].name).toBe('C');
    expect(ts.rows[1].id).toBe(2);
  });

  it('reports every cell when the table has no primary key', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValueOnce({
      columns: [
        { name: 'message', dataType: 'text', isPrimaryKey: false, nullable: true },
        { name: 'level', dataType: 'text', isPrimaryKey: false, nullable: true },
      ],
      rows: [['boot', 'info']],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    });
    await useTableDataStore.getState().loadTableData({
      panelId: NO_PK_PANEL,
      dbSessionId: 'conn-1',
      table: 'logs',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
    const writes = [
      { rowIndex: 0, columnName: 'message', value: 'A' },
      { rowIndex: 0, columnName: 'level', value: 'B' },
    ];
    const result = useTableDataStore.getState().stagePasteWrites(NO_PK_PANEL, writes);
    expect(result.staged).toBe(0);
    expect(result.problems.map((problem) => problem.code)).toEqual([
      'grid.cell.noRowIdentity',
      'grid.cell.noRowIdentity',
    ]);
  });

  it('reports an out-of-page row instead of writing it', () => {
    const result = useTableDataStore
      .getState()
      .stagePasteWrites(PASTE_PANEL, [{ rowIndex: 99, columnName: 'name', value: 'X' }]);
    expect(result.staged).toBe(0);
    expect(result.problems).toEqual([
      { rowIndex: 99, columnName: 'name', code: 'grid.paste.outOfBounds' },
    ]);
  });
});
