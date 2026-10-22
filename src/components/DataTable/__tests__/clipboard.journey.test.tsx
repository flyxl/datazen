/**
 * DB-03 §9.3 (A3) — the continuous clipboard journey.
 *
 * One session, wired exactly like `TableView` wires it (`useTableViewClipboard` → `DataTable`),
 * driven against the **real store**: every keystroke is followed by an assertion on the store
 * *and* on the DOM, so a paste that is correct in state but invisible on screen (or the other
 * way round) fails here.
 *
 * The incomplete intermediate states are part of the journey, not noise:
 * - a refused block leaves the staged work untouched but must still say why;
 * - the second paste overwrites the first **in place** (one pending entry, not two);
 * - the copy text is taken from the rows as they are *now*, including the staged value.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen, act } from '@testing-library/react';
import { useMemo } from 'react';
import { DataTable } from '../DataTable';
import { TableClipboardStatus } from '../../../windows/connection/TableClipboardStatus';
import { useTableViewClipboard } from '../../../windows/connection/useTableViewClipboard';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { useDirtyIndex } from '../../../windows/connection/useDirtyIndex';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import { EMPTY_GRID_SELECTION } from '../../../stores/tableData/gridSelection';
import { clearClipboardSource } from '../../../lib/gridPastePlan';
import type { TableState } from '../../../stores/tableData/types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string, params?: Record<string, unknown>) => (params ? `${key}:${JSON.stringify(params)}` : key) }),
}));
vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({ columnWidths: [120, 200], onResizeStart: vi.fn() }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));
vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));
vi.mock('../../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getTableData: vi.fn(),
    previewPendingChanges: vi.fn(),
    commitPendingChanges: vi.fn(),
  },
}));
vi.mock('@datazen/backend-client', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    requirePlatformServices: () => ({ readClipboard: vi.fn(async () => '') }),
  };
});

const PANEL = 'panel-clipboard-journey';
const CONTEXT = {
  connectionId: 'conn-1',
  dbSessionId: 'session-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'users',
};

const COLUMNS = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
  { name: 'city', dataType: 'text', nullable: true },
  { name: 'score', dataType: 'integer', nullable: true },
  { name: 'note', dataType: 'text', nullable: true },
];

const RECORDS = [
  { id: 1, name: 'Alice', city: 'Paris', score: 10, note: 'x' },
  { id: 2, name: 'Bob', city: 'Rome', score: 20, note: 'y' },
  { id: 3, name: 'Cara', city: 'Oslo', score: 30, note: 'z' },
];

function dirtyCells(container: HTMLElement): number {
  return container.querySelectorAll('[data-dt-dirty="true"]').length;
}

/** The grid, wired the way `TableView` wires it. */
function StoreGrid({ editable = true }: { editable?: boolean }) {
  const ts = useTableDataStore((state) => state.byPanel.get(PANEL));
  const columns = useMemo(
    () => (ts?.columns ?? []).map((column) => ({ id: column.name, name: column.name, type: column.dataType })),
    [ts],
  );
  const rows = useMemo(
    () => (ts?.rows ?? []).map((record) => columns.map((column) => record[column.name] ?? null)),
    [columns, ts],
  );
  const dirtyIndex = useDirtyIndex(ts);
  const clipboard = useTableViewClipboard({
    panelId: PANEL,
    ts,
    actions: {
      stagePasteWrites: (writes) =>
        useTableDataStore.getState().stagePasteWrites(PANEL, writes),
    },
    isEditable: editable,
    columnOrder: columns.map((column) => column.name),
    tableLabel: 'users',
  });

  return (
    <>
      <TableClipboardStatus bridge={clipboard} />
      <DataTable
        {...clipboard.dataTableProps}
        columns={columns}
        rows={rows}
        selectedRows={ts?.selectedRows ?? new Set<number>()}
        gridSelection={ts?.gridSelection ?? EMPTY_GRID_SELECTION}
        onGridSelectionChange={(selection) =>
          useTableDataStore.getState().setGridSelection(PANEL, selection)
        }
        onRowSelect={(index, opts) =>
          useTableDataStore.getState().setRowModeSelection(PANEL, index, opts)
        }
        onSelectAll={() => useTableDataStore.getState().toggleSelectAll(PANEL)}
        primaryKeyColumns={['id']}
        dirtyByRow={dirtyIndex.byRowIndex}
        editingCell={ts?.editingCell ?? null}
        onCellDoubleClick={(row, col) => useTableDataStore.getState().startEdit(PANEL, row, col)}
        onCellEditCancel={() => useTableDataStore.getState().cancelEdit(PANEL)}
      />
    </>
  );
}

function slice(): TableState {
  const ts = useTableDataStore.getState().byPanel.get(PANEL);
  if (!ts) throw new Error('panel slice missing');
  return ts;
}

function surface(container: HTMLElement): HTMLElement {
  const found = container.querySelector('[data-dt-surface]');
  if (!(found instanceof HTMLElement)) throw new Error('grid surface not found');
  return found;
}

function cell(container: HTMLElement, rowIndex: number, columnName: string): HTMLElement {
  const found = container.querySelector(`[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`);
  if (!(found instanceof HTMLElement)) throw new Error(`cell ${rowIndex}/${columnName} not found`);
  return found;
}

function clickCell(
  container: HTMLElement,
  rowIndex: number,
  columnName: string,
  modifiers: { shiftKey?: boolean } = {},
) {
  const target = cell(container, rowIndex, columnName);
  fireEvent.pointerDown(target, { pointerId: 1, button: 0, ...modifiers });
  fireEvent.pointerUp(target, { pointerId: 1, button: 0, ...modifiers });
}

function clipboardData(initial = '') {
  const store: Record<string, string> = { 'text/plain': initial };
  return {
    getData: (type: string) => store[type] ?? '',
    setData: (type: string, value: string) => {
      store[type] = value;
    },
    read: (type: string) => store[type],
  };
}

function dispatch(
  element: Element,
  type: 'copy' | 'paste',
  data: ReturnType<typeof clipboardData>,
): Event {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, 'clipboardData', { value: data });
  fireEvent(element, event);
  return event;
}

/** Paste, then let the async staging pass finish. */
async function paste(container: HTMLElement, text: string) {
  const data = clipboardData(text);
  await act(async () => {
    dispatch(surface(container), 'paste', data);
    await Promise.resolve();
    await Promise.resolve();
  });
}

/**
 * Paste a block whose source is unknown and answer the confirmation.
 *
 * The first paste of a session is always this case: nothing has been copied yet, so the
 * chapter refuses to assume the block belongs to this table (L3).
 */
async function pasteConfirmed(container: HTMLElement, text: string) {
  const data = clipboardData(text);
  await act(async () => {
    dispatch(surface(container), 'paste', data);
    await Promise.resolve();
  });
  const ok = screen.getByTestId('confirm-dialog-ok');
  await act(async () => {
    fireEvent.click(ok);
    await Promise.resolve();
    await Promise.resolve();
  });
}

function copy(container: HTMLElement) {
  const data = clipboardData();
  const event = dispatch(surface(container), 'copy', data);
  expect(event.defaultPrevented).toBe(true);
  return data.read('text/plain');
}

beforeEach(() => {
  clearClipboardSource();
  vi.stubEnv('VITE_E2E', '1');
  const panel: TableState = {
    ...emptyTableState(),
    columns: COLUMNS,
    rows: RECORDS.map((record) => ({ ...record })),
    context: CONTEXT,
  };
  useTableDataStore.setState({ byPanel: new Map([[PANEL, panel]]) });
  Object.defineProperty(document, 'elementFromPoint', {
    configurable: true,
    writable: true,
    value: vi.fn(() => null),
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});

describe('DB-03 clipboard journey', () => {
  it('walks from one pasted cell to a copy of the region and back', async () => {
    const { container } = render(<StoreGrid />);

    // ① Focus a cell: cell mode, one pending nothing.
    clickCell(container, 0, 'name');
    expect(slice().gridSelection.mode).toBe('cell');
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 0, columnName: 'name' });
    expect(slice().pendingChanges.size).toBe(0);

    // ② Paste one value: the store stages it and the DOM marks the cell dirty. The source is
    // unknown on a cold session, so this one asks first.
    await pasteConfirmed(container, 'Zoe');
    expect(slice().rows[0].name).toBe('Zoe');
    expect(slice().pendingChanges.size).toBe(1);
    const [change] = [...slice().pendingChanges.values()];
    expect(change.changedColumns).toEqual(['name']);
    expect(change.currentValues).toEqual({ name: 'Zoe' });
    expect(cell(container, 0, 'name').getAttribute('data-dt-dirty')).toBe('true');
    expect(screen.getByTestId('table-clipboard-status').textContent).toContain('dataTable.pasteResult');

    // ③ Paste again over the same cell: still one pending entry, the newer value wins.
    // Nothing has been copied yet, so this block is still "unknown source" and asks again.
    await pasteConfirmed(container, 'Zoe2');
    expect(slice().rows[0].name).toBe('Zoe2');
    expect(slice().pendingChanges.size).toBe(1);
    expect([...slice().pendingChanges.values()][0].currentValues).toEqual({ name: 'Zoe2' });
    expect([...slice().pendingChanges.values()][0].originalValues).toEqual({ name: 'Alice' });

    // ④ Extend to two rows and copy: the text comes from the rows as they are now.
    clickCell(container, 1, 'name', { shiftKey: true });
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 0, columnName: 'name' });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 1, columnName: 'name' });
    expect(copy(container)).toBe('Zoe2\nBob');

    // ⑤ Paste the copied text back — same table, so no confirmation — and both cells move.
    clickCell(container, 0, 'name');
    await paste(container, 'Zoe2\nBob');
    expect(slice().rows.map((row) => row.name)).toEqual(['Zoe2', 'Bob', 'Cara']);
    // Row 1 received its **own** original value back, so pasting it reverts to "not written"
    // instead of staging a no-op change — the three-state rule survives the batch path too.
    expect(slice().pendingChanges.size).toBe(1);
    expect(cell(container, 1, 'name').getAttribute('data-dt-dirty')).toBeNull();
  });

  it('keeps the staged work intact when a paste is refused, and says why', async () => {
    const { container } = render(<StoreGrid />);
    clickCell(container, 0, 'name');
    await pasteConfirmed(container, 'Zoe');
    expect(slice().pendingChanges.size).toBe(1);

    // A ragged block is refused before anything is staged: the earlier work survives.
    await paste(container, 'a\tb\nc');
    expect(slice().rows[0].name).toBe('Zoe');
    expect(slice().pendingChanges.size).toBe(1);
    expect(screen.getByTestId('table-clipboard-status').textContent).toContain(
      'dataTable.pasteRaggedRow',
    );
    // No cell is marked as an error: nothing was attempted.
    expect(container.querySelectorAll('[data-dt-cell-error]')).toHaveLength(0);

    // An empty clipboard is refused too, with its own wording.
    await paste(container, '   ');
    expect(screen.getByTestId('table-clipboard-status').textContent).toContain(
      'dataTable.pasteEmpty',
    );
    expect(slice().pendingChanges.size).toBe(1);
  });

  it('asks before writing a block whose source is unknown', async () => {
    const { container } = render(<StoreGrid />);
    clickCell(container, 0, 'name');

    // Nothing was copied in this session ⇒ the source is unknown ⇒ the dialog stands between
    // the gesture and the write.
    const data = clipboardData('Zoe');
    await act(async () => {
      dispatch(surface(container), 'paste', data);
      await Promise.resolve();
    });
    expect(slice().pendingChanges.size).toBe(0);

    await act(async () => {
      fireEvent.click(screen.getByTestId('confirm-dialog-ok'));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(slice().rows[0].name).toBe('Zoe');
    expect(slice().pendingChanges.size).toBe(1);
  });

  it('cancelling the confirmation writes nothing and claims nothing', async () => {
    const { container } = render(<StoreGrid />);
    clickCell(container, 0, 'name');

    const data = clipboardData('Zoe');
    await act(async () => {
      dispatch(surface(container), 'paste', data);
      await Promise.resolve();
    });
    await act(async () => {
      fireEvent.click(screen.getByTestId('confirm-dialog-cancel'));
      await Promise.resolve();
    });

    expect(slice().rows[0].name).toBe('Alice');
    expect(slice().pendingChanges.size).toBe(0);
    // Nothing was written — and nothing claims otherwise. The cross-table note reads "Pasted
    // from another table into {table}": after a cancellation that sentence would be the only
    // thing on screen and it would be false. The dialog already gave the reason; a declined
    // paste needs no summary line.
    expect(screen.queryByTestId('table-clipboard-status')).not.toBeInTheDocument();
  });
  it('journey: an external 3x4 block lands, is overwritten in place, and copies back', async () => {
    const { container } = render(<StoreGrid />);

    // ① The landing point.
    clickCell(container, 0, 'name');
    expect(slice().gridSelection.mode).toBe('cell');

    // ② An Excel-shaped 3x4 block: a quoted comma, a newline inside quotes, and a plain value.
    const block = 'a1\ta2\ta3\ta4\n"x,y"\tb2\tb3\tb4\n"line1\nline2"\tc2\tc3\tc4';
    await pasteConfirmed(container, block);

    // ③ Twelve cells, twelve dirty marks.
    expect(slice().pendingChanges.size).toBe(3);
    expect(dirtyCells(container)).toBe(12);
    const staged = slicedValues();
    expect(staged[0]).toEqual({ name: 'a1', city: 'a2', score: 'a3', note: 'a4' });
    expect(staged[1].name).toBe('x,y');
    expect(staged[2].name).toBe('line1\nline2');

    // ④ A second 2x2 paste over the same landing point overwrites four cells in place.
    await pasteConfirmed(container, 'z1\tz2\nz3\tz4');
    expect(slice().pendingChanges.size).toBe(3);
    expect(dirtyCells(container)).toBe(12);
    expect(slicedValues()[0]).toEqual({ name: 'z1', city: 'z2', score: 'a3', note: 'a4' });

    // ⑤ Copy the same rectangle: the text is the rows as they are now, character for character.
    clickCell(container, 0, 'name');
    clickCell(container, 2, 'note', { shiftKey: true });
    // The embedded newline is quoted, so the three source rows stay three rows.
    expect(copy(container)).toBe(
      'z1\tz2\ta3\ta4\nz3\tz4\tb3\tb4\n"line1\nline2"\tc2\tc3\tc4',
    );
  });

  /** The staged value per row, restricted to the three writable columns. */
  function slicedValues(): Record<string, unknown>[] {
    return [...slice().pendingChanges.values()].map((change) => change.currentValues);
  }

  it('journey: a locked table refuses paste but still allows copy', async () => {
    const { container } = render(<StoreGrid editable={false} />);
    clickCell(container, 0, 'name');

    const data = clipboardData('Zoe');
    let prevented = false;
    await act(async () => {
      const event = new Event('paste', { bubbles: true, cancelable: true });
      Object.defineProperty(event, 'clipboardData', { value: data });
      fireEvent(surface(container), event);
      prevented = event.defaultPrevented;
      await Promise.resolve();
    });
    // Consumed, so the browser does not paste into the page — but nothing is written.
    expect(prevented).toBe(true);
    expect(slice().pendingChanges.size).toBe(0);
    expect(screen.getByTestId('table-clipboard-status').textContent).toContain(
      'dataTable.readOnlyPasteDisabled',
    );

    // Copy is not a write gesture, so a read-only connection keeps it.
    clickCell(container, 0, 'name');
    expect(copy(container)).toBe('Alice');
  });

  it('journey: quoted empty and the NULL literal mean different things', async () => {
    const { container } = render(<StoreGrid />);
    clickCell(container, 0, 'name');

    // ① `""` is an explicit empty string, `NULL` is SQL NULL.
    await pasteConfirmed(container, '"\tNULL"'.replace('"\tNULL"', '""\tNULL'));
    expect(slice().rows[0].name).toBe('');
    expect(slice().rows[0].city).toBeNull();
    expect(dirtyCells(container)).toBe(2);

    // ② Copy back: both render as empty, so the text is two adjacent tabs…
    clickCell(container, 0, 'name');
    clickCell(container, 0, 'city', { shiftKey: true });
    expect(copy(container)).toBe('\t');

    // ③ …and pasting *that* (unquoted empty fields) writes NULL in both, not an empty string.
    // No dialog this time: step ② recorded the copy source, so the block is known to be ours.
    await paste(container, '\t');
    expect(slice().rows[0].name).toBeNull();
    expect(slice().rows[0].city).toBeNull();
  });

  it('journey: editing swallows paste, then the grid resumes', async () => {
    const { container } = render(<StoreGrid />);

    await act(async () => {
      useTableDataStore.getState().startEdit(PANEL, 0, 'name');
    });
    expect(slice().editingCell).not.toBeNull();

    const swallowed = clipboardData('Zoe');
    let prevented = false;
    await act(async () => {
      const event = new Event('paste', { bubbles: true, cancelable: true });
      Object.defineProperty(event, 'clipboardData', { value: swallowed });
      fireEvent(surface(container), event);
      prevented = event.defaultPrevented;
      await Promise.resolve();
    });
    expect(prevented).toBe(false);
    expect(slice().editingCell).not.toBeNull();
    expect(slice().pendingChanges.size).toBe(0);

    await act(async () => {
      useTableDataStore.getState().cancelEdit(PANEL);
    });
    clickCell(container, 0, 'name');
    await pasteConfirmed(container, 'Zoe');
    expect(slice().rows[0].name).toBe('Zoe');
  });

  it('journey: a page change drops the paste landing point (DB-01 guard, B21)', async () => {
    const { container } = render(<StoreGrid />);
    clickCell(container, 0, 'name');
    expect(slice().gridSelection.mode).toBe('cell');

    await act(async () => {
      await useTableDataStore.getState().setPage(PANEL, 1);
    });

    // DB-01 owns the clearing; this chapter only guards it. A red line here is DB-01's defect,
    // not something this file may paper over.
    expect(slice().gridSelection.mode).toBe('none');
    expect(slice().selectedRows.size).toBe(0);
    expect(slice().lastSelectedIndex).toBeNull();

    // Without a landing point the paste must not be consumed.
    const data = clipboardData('Zoe');
    let prevented = true;
    await act(async () => {
      const event = new Event('paste', { bubbles: true, cancelable: true });
      Object.defineProperty(event, 'clipboardData', { value: data });
      fireEvent(surface(container), event);
      prevented = event.defaultPrevented;
      await Promise.resolve();
    });
    expect(prevented).toBe(false);
    expect(slice().pendingChanges.size).toBe(0);
  });
});
