/**
 * DB-03 §9.2 — `useGridClipboard`, the orchestration.
 *
 * This is the layer where a paste can go wrong in ways the pure planner cannot see: the order
 * of "plan → confirm → stage", the chunking of a large block, the read-only refusal, and the
 * clipboard read that may throw. The host below is deliberately minimal (it only renders the
 * hook and its dialog) so every assertion is about the hook, not about the grid.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, cleanup, act, fireEvent } from '@testing-library/react';
import { GRID_PASTE_CONFIRM_CELLS } from '../../lib/gridClipboard';
import {
  peekClipboardSource,
  clearClipboardSource,
  recordClipboardSource,
  tableChangeKey,
} from '../../lib/gridPastePlan';
import { useGridClipboard, type UseGridClipboardResult } from '../useGridClipboard';
import type { CellPasteWrite } from '../../lib/tableChanges';
import type { StagePasteWritesResult } from '../../stores/tableData/pendingEditActions';
import type { ColumnSchema } from '../../types';

const readClipboard = vi.fn();
// Only the platform *port* is replaced: the rest of the package (which the hook chain also
// imports) stays real, so nothing else in the module graph is faked by accident.
vi.mock('@datazen/backend-client', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, requirePlatformServices: () => ({ readClipboard }) };
});

const columns: ColumnSchema[] = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
];
const columnOrder = ['id', 'name'];

const context = {
  connectionId: 'conn-1',
  dbSessionId: 'sess-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'users',
};

function makeRows(count: number): Record<string, unknown>[] {
  return Array.from({ length: count }, (_, index) => ({ id: index + 1, name: `n${index}` }));
}

interface HostProps {
  rows?: Record<string, unknown>[];
  isEditable?: boolean;
  stagePasteWrites?: (writes: readonly CellPasteWrite[]) => StagePasteWritesResult;
  onReadOnly?: () => void;
  selection?: { topRow: number; bottomRow: number; left: string; right: string } | null;
  capture: (result: UseGridClipboardResult) => void;
}

function Host({
  rows = makeRows(3),
  isEditable = true,
  stagePasteWrites = () => ({ staged: 0, skipped: 0, problems: [] }),
  onReadOnly,
  selection = null,
  capture,
}: HostProps) {
  const result = useGridClipboard({
    panelId: 'panel-clip',
    isEditable,
    columns,
    columnOrder,
    rows,
    pendingChanges: new Map(),
    rowIdentityAnchors: new Map(),
    selectedRows: new Set<number>(),
    gridSelection: selection
      ? {
          mode: 'cell',
          anchor: { rowIndex: selection.topRow, columnName: selection.left },
          focus: { rowIndex: selection.bottomRow, columnName: selection.right },
          extraRanges: [],
        }
      : { mode: 'cell', anchor: { rowIndex: 0, columnName: 'name' }, focus: { rowIndex: 0, columnName: 'name' }, extraRanges: [] },
    context,
    tableLabel: 'users',
    stagePasteWrites,
    showReadOnlyTip: onReadOnly,
  });
  capture(result);
  return <>{result.confirmElement}</>;
}

/** Render the host and hand back the newest hook result. */
function mount(props: Partial<HostProps> = {}) {
  let latest: UseGridClipboardResult | null = null;
  const utils = render(
    <Host
      {...props}
      capture={(result) => {
        latest = result;
        props.capture?.(result);
      }}
    />,
  );
  return {
    ...utils,
    current: () => {
      if (!latest) throw new Error('the hook never rendered');
      return latest;
    },
  };
}

/**
 * Copy one row so the module record knows "this block came from `users`".
 *
 * Without it the first paste of a session is a **cross-table** paste by definition (the
 * source is unknown), and the hook would — correctly — put a confirmation dialog in the way
 * of every staging test in this file.
 */
function recordSameTable(host: ReturnType<typeof mount>) {
  const text = host.current().handlers.buildCopyText({ mode: 'row', rowIndices: [0] });
  expect(text).not.toBeNull();
}

/**
 * Record that **this exact text** came from the table under test.
 *
 * The record is only usable while the clipboard still holds the text we copied
 * (`sourceForClipboardText`), so recording a source for *some other* text would describe a
 * paste that no longer matches — the hook then asks for a confirmation, which is the point
 * of the binding. A staging test has to say "the clipboard holds what we copied" explicitly.
 */
function pretendCopied(text: string) {
  recordClipboardSource(
    {
      tableKey: tableChangeKey(context),
      columnNames: columnOrder,
      columnTypes: ['number', 'text'],
    },
    text,
  );
}

const anchor = { rowIndex: 0, columnName: 'name' };

beforeEach(() => {
  clearClipboardSource();
  readClipboard.mockReset();
  cleanup();
});

describe('useGridClipboard', () => {
  it('stages a pasted block through the owner action and reports the summary', async () => {
    const staged: CellPasteWrite[][] = [];
    const host = mount({
      stagePasteWrites: (writes) => {
        staged.push([...writes]);
        return { staged: writes.length, skipped: 0, problems: [] };
      },
    });
    pretendCopied('7\tAlice');

    await act(async () => {
      // After a same-table row copy the block aligns **by name**: the first field is the
      // source's first column (`id`), whichever cell the user pasted into.
      await host.current().pasteText('7\tAlice', { rowIndex: 0, columnName: 'name' });
    });

    expect(staged).toHaveLength(1);
    expect(staged[0]).toEqual([
      { rowIndex: 0, columnName: 'id', value: '7' },
      { rowIndex: 0, columnName: 'name', value: 'Alice' },
    ]);
    expect(host.current().lastPaste).toMatchObject({ staged: 2, skipped: 0, cancelled: false });
  });

  it('splits a large block into chunks and yields between them', async () => {
    const calls: number[] = [];
    const rowCount = 2601;
    const host = mount({
      rows: makeRows(rowCount),
      stagePasteWrites: (writes) => {
        calls.push(writes.length);
        return { staged: writes.length, skipped: 0, problems: [] };
      },
    });
    // 2 601 rows × 2 columns = 5 202 cells → 5 000 + 202.
    const text = Array.from({ length: rowCount }, () => 'x\ty').join('\n');
    pretendCopied(text);

    let pending: Promise<boolean> = Promise.resolve(false);
    act(() => {
      pending = host.current().pasteText(text, anchor);
    });
    // A block this size is above the confirmation threshold by definition, so the dialog is
    // part of the journey — the chunking happens *after* the user says yes.
    await act(async () => {
      await Promise.resolve();
    });
    fireEvent.click(screen.getByTestId('confirm-dialog-ok'));
    await act(async () => {
      await pending;
    });

    expect(calls).toEqual([5000, 202]);
    expect(host.current().lastPaste?.staged).toBe(rowCount * 2);
  });

  it('asks first when the block is above the confirmation threshold', async () => {
    const stagePasteWrites = vi.fn((writes: readonly CellPasteWrite[]) => ({
      staged: writes.length,
      skipped: 0,
      problems: [],
    }));
    const rows = GRID_PASTE_CONFIRM_CELLS + 1;
    const host = mount({ rows: makeRows(rows + 1), stagePasteWrites });

    let pending: Promise<boolean> = Promise.resolve(false);
    act(() => {
      pending = host.current().pasteText(Array.from({ length: rows }, () => 'x').join('\n'), {
        rowIndex: 0,
        columnName: 'name',
      });
    });

    // Nothing is written before the user answers.
    await act(async () => {
      await Promise.resolve();
    });
    expect(stagePasteWrites).not.toHaveBeenCalled();
    const ok = screen.getByTestId('confirm-dialog-ok');
    fireEvent.click(ok);
    await act(async () => {
      await pending;
    });

    expect(stagePasteWrites).toHaveBeenCalledTimes(1);
    expect(host.current().lastPaste?.staged).toBeGreaterThan(0);
  });

  it('treats a clipboard that changed since our copy as an external paste', async () => {
    // The failure this pins: copy a row here, copy a block in Excel, paste here. The stale
    // record still says "same table", so trusting it would waive the confirmation **and**
    // align the Excel block by our column names — a silent write into the wrong columns.
    const stagePasteWrites = vi.fn((writes: readonly CellPasteWrite[]) => ({
      staged: writes.length,
      skipped: 0,
      problems: [],
    }));
    const host = mount({ stagePasteWrites });
    recordSameTable(host);

    let pending: Promise<boolean> = Promise.resolve(false);
    act(() => {
      pending = host.current().pasteText('x\ty', { rowIndex: 0, columnName: 'name' });
    });
    await act(async () => {
      await Promise.resolve();
    });

    // It has to ask, exactly as it would for a paste from Excel.
    expect(stagePasteWrites).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId('confirm-dialog-ok'));
    await act(async () => {
      await pending;
    });

    // And it lands **by position** from the cell the user aimed at, not by the stale record's
    // column names (which would have started at `id`).
    expect(stagePasteWrites).toHaveBeenCalledTimes(1);
    const writes = stagePasteWrites.mock.calls[0][0];
    expect(writes[0]).toEqual({ rowIndex: 0, columnName: 'name', value: 'x' });
  });

  it('writes nothing when the user cancels the confirmation', async () => {
    const stagePasteWrites = vi.fn(() => ({ staged: 0, skipped: 0, problems: [] }));
    const host = mount({ rows: makeRows(5), stagePasteWrites });

    let pending: Promise<boolean> = Promise.resolve(false);
    act(() => {
      // Nothing was copied in this session, so the source is unknown (`crossTable === null`),
      // which is exactly the case that must ask before writing (L3).
      pending = host.current().pasteText('x\ty', { rowIndex: 0, columnName: 'name' });
    });
    await act(async () => {
      await Promise.resolve();
    });
    fireEvent.click(screen.getByTestId('confirm-dialog-cancel'));
    await act(async () => {
      await pending;
    });

    expect(stagePasteWrites).not.toHaveBeenCalled();
    expect(host.current().lastPaste).toMatchObject({ staged: 0, cancelled: true });
  });

  it('refuses a read-only panel with the tip and consumes the event', async () => {
    const stagePasteWrites = vi.fn(() => ({ staged: 0, skipped: 0, problems: [] }));
    const onReadOnly = vi.fn();
    const host = mount({ isEditable: false, stagePasteWrites, onReadOnly });

    let consumed = false;
    await act(async () => {
      consumed = await host.current().pasteText('x', anchor);
    });

    expect(consumed).toBe(true);
    expect(onReadOnly).toHaveBeenCalledTimes(1);
    expect(stagePasteWrites).not.toHaveBeenCalled();
    expect(host.current().problems.map((problem) => problem.code)).toContain('grid.cell.readOnly');
  });

  it('reports a clipboard read failure instead of throwing', async () => {
    readClipboard.mockRejectedValue(new Error('denied'));
    const stagePasteWrites = vi.fn(() => ({ staged: 0, skipped: 0, problems: [] }));
    const host = mount({ stagePasteWrites });

    await act(async () => {
      await host.current().handlers.onPasteFromClipboard?.(anchor);
    });

    expect(stagePasteWrites).not.toHaveBeenCalled();
    expect(host.current().problems.map((problem) => problem.code)).toContain(
      'grid.clipboard.readFailed',
    );
  });

  it('marks the cells it could not write', async () => {
    const host = mount({
      stagePasteWrites: (writes) => ({
        staged: writes.length - 1,
        skipped: 1,
        problems: [
          { rowIndex: 0, columnName: 'id', code: 'grid.paste.identityChanged' },
        ],
      }),
    });
    pretendCopied('x\ty');

    await act(async () => {
      await host.current().pasteText('x\ty', anchor);
    });

    expect([...host.current().cellErrors]).toEqual(['0:id']);
    expect(host.current().problems.map((problem) => problem.code)).toContain(
      'grid.paste.identityChanged',
    );
  });

  it('builds the copy text from the selection and records where it came from', () => {
    const host = mount({ rows: makeRows(4) });
    const text = host
      .current()
      .handlers.buildCopyText({
        mode: 'cell',
        range: { topRow: 0, bottomRow: 1, leftColumn: 'name', rightColumn: 'name' },
      });

    expect(text).toBe('n0\nn1');
    // The record is what lets a later paste prove "same table" (L1)…
    expect(peekClipboardSource()?.columnNames).toEqual(['name']);
    // …and it is only written when text actually reached the clipboard.
    expect(peekClipboardSource()?.tableKey).toBe(
      JSON.stringify(['conn-1', 'app', '', 'users']).toLowerCase(),
    );
  });

  it('does not record a source for an empty selection', () => {
    const host = mount({ rows: makeRows(1) });
    const text = host.current().handlers.buildCopyText({ mode: 'row', rowIndices: [] });
    expect(text).toBeNull();
    expect(peekClipboardSource()).toBeNull();
  });

  it('clears the per-cell marks when a new paste starts', async () => {
    const host = mount({
      stagePasteWrites: (writes) => ({
        staged: 0,
        skipped: writes.length,
        problems: writes.map((write) => ({
          rowIndex: write.rowIndex,
          columnName: write.columnName,
          code: 'grid.paste.outOfBounds' as const,
        })),
      }),
    });
    pretendCopied('x\ty');

    await act(async () => {
      await host.current().pasteText('x\ty', anchor);
    });
    expect(host.current().cellErrors.size).toBe(2);

    act(() => {
      host.current().clearPasteResult();
    });
    expect(host.current().cellErrors.size).toBe(0);
    expect(host.current().lastPaste).toBeNull();
  });
});
