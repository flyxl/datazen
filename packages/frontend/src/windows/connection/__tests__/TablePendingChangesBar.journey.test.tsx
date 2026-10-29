/**
 * Component-level journey (AGENTS.md "连续旅程测试"): selection → edit → commit →
 * the re-fetched page clears the selection.
 *
 * This is the wiring contract DB-01 Step 10 lands in `TableView`: `useGridSelection` reports
 * the region, the store owns it, and `TablePendingChangesBar` triggers the ChangeSet commit
 * whose follow-up `loadTableData` must drop every row-indexed selection. The intermediate
 * (in-flight commit) state is asserted too, because "clear the selection too early" and
 * "never clear it" both look green if only the final state is checked.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useCallback, useState } from 'react';
import {
  EMPTY_GRID_SELECTION,
  type GridSelection,
} from '../../../stores/tableData/gridSelection';
import { useGridSelection } from '../../../hooks/useGridSelection';
import { TablePendingChangesBar } from '../TablePendingChangesBar';
import { useTableDataStore } from '../../../stores/tableDataStore';
import type { TableState } from '../../../stores/tableData/types';
import type { PendingRowChange } from '../../../lib/tableChanges';

const mockDatabaseCommands = vi.hoisted(() => ({
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
}));

vi.mock('../../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));

const i18n = vi.hoisted(() => ({
  t: (key: string, params?: Record<string, unknown>) =>
    params && params.count !== undefined ? `${key}#${String(params.count)}` : key,
}));
vi.mock('../../../hooks/useI18n', () => ({ useI18n: () => i18n }));

const PANEL = 'panel-journey';
const COLUMNS = ['id', 'name'];

const context = {
  connectionId: 'conn-1',
  dbSessionId: 'conn-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'users',
} as const;

const columns = [
  { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
];

function pageResponse(nameOfFirstRow: string) {
  return {
    columns,
    rows: [
      [1, nameOfFirstRow],
      [2, 'Bob'],
    ] as (string | number | null)[][],
    totalRows: 2,
    page: 0,
    pageSize: 50,
  };
}

const plan = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  table: context,
  updates: [
    {
      rowIdentity: { id: 1 },
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Alicia' },
      changedColumns: ['name'],
      sqlTemplate: 'UPDATE "users" SET "name" = \'Alicia\' WHERE "id" = 1',
      parameterSummary: ['SET name="Alicia"', 'PK id=1'],
    },
  ],
  deletes: [],
  warnings: [],
};

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Mirrors the DB-01 Step 10 wiring: store-owned selection + the extracted pending bar. */
function Journey() {
  const ts: TableState | undefined = useTableDataStore((state) => state.byPanel.get(PANEL));
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(null);
  const selection = ts?.gridSelection ?? EMPTY_GRID_SELECTION;
  const onChange = useCallback((next: GridSelection) => {
    useTableDataStore.getState().setGridSelection(PANEL, next);
  }, []);
  const grid = useGridSelection({
    columnOrder: COLUMNS,
    rowCount: ts?.rows.length ?? 0,
    selection,
    onChange,
    scrollElement,
    onRowGutterSelect: (rowIndex, opts) =>
      useTableDataStore.getState().selectRow(PANEL, rowIndex, opts),
    editingCell: ts?.editingCell ?? null,
  });

  const pending: ReadonlyMap<string, PendingRowChange> = ts?.pendingChanges ?? new Map();
  const deleteCount = [...pending.values()].filter((change) => change.deleteMarked).length;
  const rows = ts?.rows ?? [];

  return (
    <div>
      <TablePendingChangesBar
        pendingCount={pending.size}
        updateCount={pending.size - deleteCount}
        deleteCount={deleteCount}
        busy={ts?.pendingStatus !== 'idle'}
        onPreview={() => undefined}
        onCommit={() => {
          void useTableDataStore.getState().commitPendingChanges(PANEL);
        }}
        onRollback={() => useTableDataStore.getState().rollbackPendingChanges(PANEL)}
      />
      <div data-testid="grid" ref={setScrollElement} {...grid.surfaceProps}>
        {rows.map((row, rowIndex) => (
          <div key={rowIndex} data-testid={`row-${rowIndex}`} data-dt-row={rowIndex}>
            <div role="rowheader">
              <button type="button" data-testid={`gutter-${rowIndex}`}>
                {rowIndex + 1}
              </button>
            </div>
            {COLUMNS.map((columnName) => (
              <div
                key={columnName}
                data-testid={`cell-${rowIndex}-${columnName}`}
                data-dt-row={rowIndex}
                data-dt-col={columnName}
                data-dt-selected={
                  grid.isCellSelected(rowIndex, columnName) ? 'true' : undefined
                }
              >
                {String(row[columnName] ?? '')}
              </div>
            ))}
          </div>
        ))}
      </div>
      <span data-testid="mode">{selection.mode}</span>
    </div>
  );
}

describe('selection → edit → commit → refetch journey', () => {
  const store = () => useTableDataStore.getState();
  const panel = (): TableState => {
    const ts = store().byPanel.get(PANEL);
    if (!ts) throw new Error('journey panel is not loaded');
    return ts;
  };

  beforeEach(async () => {
    cleanup();
    vi.clearAllMocks();
    store().reset();
    mockDatabaseCommands.getTableData
      .mockResolvedValueOnce(pageResponse('Alice'))
      .mockResolvedValue(pageResponse('Alicia'));
    mockDatabaseCommands.previewPendingChanges.mockResolvedValue(plan);
    mockDatabaseCommands.commitPendingChanges.mockResolvedValue({
      planId: plan.planId,
      fingerprint: plan.fingerprint,
      statements: [{ operation: 'update', rowIdentity: { id: 1 }, affectedRows: 1 }],
      affectedRows: 1,
    });

    await store().loadTableData({ panelId: PANEL, ...context });
  });

  afterEach(cleanup);

  it('keeps the region across edit, then clears it only when the refetched page lands', async () => {
    render(<Journey />);

    // 1. Establish the cell selection through the real pointer translation.
    fireEvent.pointerDown(screen.getByTestId('cell-0-name'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(screen.getByTestId('cell-0-name'), { pointerId: 1, button: 0 });

    expect(screen.getByTestId('mode').textContent).toBe('cell');
    expect(screen.getByTestId('cell-0-name')).toHaveAttribute('data-dt-selected', 'true');
    expect(screen.getByTestId('cell-1-name')).not.toHaveAttribute('data-dt-selected');
    expect(panel().gridSelection).toEqual({
      anchor: { rowIndex: 0, columnName: 'name' },
      focus: { rowIndex: 0, columnName: 'name' },
      extraRanges: [],
      mode: 'cell',
    });
    // Cell mode owns the row set: nothing is left to re-derive (contract §4.2 rule 2).
    expect(panel().selectedRows.size).toBe(0);
    expect(screen.queryByTestId('pending-changes-bar')).toBeNull();

    // 2. Edit the selected cell and stage the change.
    act(() => {
      store().startEdit(PANEL, 0, 'name');
      store().stageCellChange(PANEL, 0, 'name', 'Alicia');
    });

    expect(panel().pendingChanges.size).toBe(1);
    // Staging never freezes or clears the selection (01-selection §7 #10).
    expect(panel().gridSelection.mode).toBe('cell');
    expect(screen.getByTestId('pending-changes-bar')).toHaveTextContent(
      'tableData.pendingChanges#1',
    );
    expect(screen.getByTestId('pending-commit')).toBeEnabled();

    // 3. Commit while the ChangeSet IPC is still in flight: the bar must stay (busy) and
    //    the selection must not be cleared just because a commit started.
    const commit = deferred<{
      planId: string;
      fingerprint: string;
      statements: { operation: 'update'; rowIdentity: { id: number }; affectedRows: number }[];
      affectedRows: number;
    }>();
    mockDatabaseCommands.commitPendingChanges.mockReturnValueOnce(commit.promise);

    fireEvent.click(screen.getByTestId('pending-commit'));

    await waitFor(() => expect(panel().pendingStatus).toBe('committing'));
    expect(screen.getByTestId('pending-changes-bar')).toHaveAttribute('aria-busy', 'true');
    expect(panel().gridSelection.mode).toBe('cell');
    expect(screen.getByTestId('cell-0-name')).toHaveAttribute('data-dt-selected', 'true');

    await act(async () => {
      commit.resolve({
        planId: plan.planId,
        fingerprint: plan.fingerprint,
        statements: [{ operation: 'update', rowIdentity: { id: 1 }, affectedRows: 1 }],
        affectedRows: 1,
      });
    });

    // 4. The follow-up refetch clears all row-indexed state and shows the new value.
    await waitFor(() => expect(screen.queryByTestId('pending-changes-bar')).toBeNull());
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2);
    expect(panel().gridSelection).toEqual(EMPTY_GRID_SELECTION);
    expect(panel().selectedRows.size).toBe(0);
    expect(panel().lastSelectedIndex).toBeNull();
    expect(panel().rows[0].name).toBe('Alicia');
    expect(screen.getByTestId('mode').textContent).toBe('none');
    expect(screen.getByTestId('cell-0-name')).not.toHaveAttribute('data-dt-selected');
  });
});
