import { describe, expect, it, vi, afterEach } from 'vitest';
import { act, cleanup, render, screen, fireEvent } from '@testing-library/react';
import {
  clearRowCopy,
  peekRowCopy,
  recordRowCopy,
  rowCopyForClipboardText,
} from '../../lib/gridPastePlan';
import { rowsToDraftValues, useGridClipboard, type UseGridClipboardResult } from '../useGridClipboard';
import type { ColumnSchema, Value } from '../../types';
import type { CellPasteWrite } from '../../lib/tableChanges';
import type { StagePasteWritesResult } from '../../stores/tableData/pendingEditActions';

/**
 * DBR-02: copy existing rows → paste as new draft rows. The row payload keeps
 * the real values (TSV cannot tell NULL from ''), primary-key and
 * auto-increment values never travel, same-table pastes need no confirmation
 * and cross-table pastes show the mapping summary first.
 */

const readClipboard = vi.fn();
const writeClipboard = vi.fn();
// Only the platform *port* is replaced; the rest of the package stays real
// (same discipline as useGridClipboard.test.tsx).
vi.mock('@datazen/backend-client', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    requirePlatformServices: () => ({ readClipboard, writeClipboard }),
  };
});

const columns: ColumnSchema[] = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true, isAutoIncrement: true },
  { name: 'name', dataType: 'text', nullable: true },
];
const rows = [
  { id: 1, name: 'alpha' },
  { id: 2, name: null },
  { id: 3, name: 'gamma' },
];
const context = {
  connectionId: 'c',
  dbSessionId: 's1',
  driverType: 'postgresql',
  database: 'db',
  schema: null,
  table: 't',
};

const stagePasteWrites = vi
  .fn<(writes: readonly CellPasteWrite[]) => StagePasteWritesResult>()
  .mockReturnValue({ staged: 0, skipped: 0, problems: [] });
const addDraftRows = vi
  .fn<(rows: readonly Record<string, Value | null>[]) => string[]>()
  .mockReturnValue([]);

/** A host that renders the confirm dialog — without it, `confirm()` never resolves. */
let captured: UseGridClipboardResult | null = null;
function mountClipboard(selectedRows: ReadonlySet<number> = new Set([0, 1])) {
  function Host() {
    const result = useGridClipboard({
      panelId: 'p1',
      isEditable: true,
      columns,
      columnOrder: ['id', 'name'],
      rows,
      pendingChanges: new Map(),
      rowIdentityAnchors: new Map(),
      selectedRows,
      gridSelection: { mode: 'row', anchor: null, focus: null, extraRanges: [] },
      context,
      tableLabel: 't',
      stagePasteWrites,
      addDraftRows,
    });
    captured = result;
    return <>{result.confirmElement}</>;
  }
  render(<Host />);
  return {
    current: () => captured as UseGridClipboardResult,
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  clearRowCopy();
});

describe('row copy payload (gridPastePlan)', () => {
  it('round-trips a recorded row copy through its clipboard text', () => {
    recordRowCopy(
      { tableKey: '["c","db","","t"]', columnNames: ['id', 'name'], rows: [[1, 'a'], [2, null]] },
      '1\ta\n2\t',
    );
    expect(rowCopyForClipboardText('1\ta\n2\t')?.rows).toEqual([[1, 'a'], [2, null]]);
    expect(peekRowCopy()).not.toBeNull();
  });

  it('a different clipboard text invalidates the payload', () => {
    recordRowCopy({ tableKey: 'k', columnNames: ['a'], rows: [['x']] }, 'x');
    expect(rowCopyForClipboardText('something else from Excel')).toBeNull();
  });

  it('CRLF from the OS clipboard still verifies (Windows rewrites the final newline)', () => {
    recordRowCopy(
      { tableKey: 'k', columnNames: ['a', 'b'], rows: [['x', 'y']] },
      'x\ty',
    );
    // The OS rewrote the trailing newline of the copy; CRLF normalization on
    // both sides keeps the claim verifiable.
    recordRowCopy({ tableKey: 'k', columnNames: ['a', 'b'], rows: [['x', 'y']] }, 'x\ty\n');
    expect(rowCopyForClipboardText('x\ty\r\n')).not.toBeNull();
  });

  it('clearing kills the payload', () => {
    recordRowCopy({ tableKey: 'k', columnNames: ['a'], rows: [['x']] }, 'x');
    clearRowCopy();
    expect(rowCopyForClipboardText('x')).toBeNull();
  });
});

describe('rowsToDraftValues', () => {
  it('strips primary-key and auto-increment columns', () => {
    const drafts = rowsToDraftValues(['id', 'name'], [[5, 'kept']], columns);
    expect(drafts).toEqual([{ name: 'kept' }]);
  });

  it('preserves explicit NULLs and real values verbatim', () => {
    const drafts = rowsToDraftValues(['name', 'id'], [['a', 1], [null, 2]], columns);
    expect(drafts).toEqual([{ name: 'a' }, { name: null }]);
  });

  it('columns the target does not know are skipped', () => {
    const drafts = rowsToDraftValues(['unknown_col', 'name'], [['v', 'a']], columns);
    expect(drafts).toEqual([{ name: 'a' }]);
  });
});

describe('useGridClipboard row features (DBR-02)', () => {
  it('copyRowsAsDrafts copies the covered rows straight into drafts', () => {
    const hook = mountClipboard();
    let ok = false;
    act(() => {
      ok = hook.current().copyRowsAsDrafts();
    });
    expect(ok).toBe(true);
    // Two drafts, PK stripped, the explicit NULL in row 2 preserved.
    expect(addDraftRows).toHaveBeenCalledWith([{ name: 'alpha' }, { name: null }]);
  });

  it('copyRowsAsNewRows writes the row TSV and records the payload', async () => {
    const hook = mountClipboard();
    let ok = false;
    await act(async () => {
      ok = await hook.current().copyRowsAsNewRows();
    });
    expect(ok).toBe(true);
    const payload = peekRowCopy();
    expect(payload).not.toBeNull();
    expect(payload?.tableKey).toBe('["c","db","","t"]');
    // The payload keeps the full matrix — PK stripping happens when the rows
    // become drafts, not at copy time (the clipboard stays faithful).
    expect(payload?.columnNames).toEqual(['id', 'name']);
    expect(payload?.rows).toEqual([[1, 'alpha'], [2, null]]);
  });

  it('pasteRowsAsDrafts pastes a same-table copy without confirmation', async () => {
    // A same-table row copy (the hook's own context is c/db//t).
    recordRowCopy(
      { tableKey: '["c","db","","t"]', columnNames: ['id', 'name'], rows: [[7, 'pasted-name']] },
      '7\tpasted-name',
    );
    readClipboard.mockResolvedValue('7\tpasted-name');
    const hook = mountClipboard(new Set([2]));
    let consumed = false;
    await act(async () => {
      consumed = await hook.current().pasteRowsAsDrafts();
    });
    expect(consumed).toBe(true);
    // No confirm dialog ever rendered for a same-table paste.
    expect(screen.queryByTestId('confirm-dialog-ok')).toBeNull();
    expect(addDraftRows).toHaveBeenCalledWith([{ name: 'pasted-name' }]);
  });

  it('pasteRowsAsDrafts asks before a cross-table paste and refuses on cancel', async () => {
    recordRowCopy(
      {
        tableKey: '["c","other-db","","source-table"]',
        columnNames: ['name'],
        rows: [['cross-name']],
      },
      'cross-name',
    );
    readClipboard.mockResolvedValue('cross-name');
    const hook = mountClipboard(new Set([2]));
    let pending: Promise<boolean> = Promise.resolve(false);
    act(() => {
      pending = hook.current().pasteRowsAsDrafts();
    });
    // The mapping summary dialog is up; nothing is written before the answer.
    await act(async () => {
      await Promise.resolve();
    });
    expect(addDraftRows).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId('confirm-dialog-cancel'));
    await act(async () => {
      await pending;
    });
    expect(addDraftRows).not.toHaveBeenCalled();
  });

  it('pasteRowsAsDrafts confirms a cross-table paste on OK', async () => {
    recordRowCopy(
      {
        tableKey: '["c","other-db","","source-table"]',
        columnNames: ['name'],
        rows: [['cross-name']],
      },
      'cross-name',
    );
    readClipboard.mockResolvedValue('cross-name');
    const hook = mountClipboard(new Set([2]));
    let pending: Promise<boolean> = Promise.resolve(false);
    act(() => {
      pending = hook.current().pasteRowsAsDrafts();
    });
    await act(async () => {
      await Promise.resolve();
    });
    fireEvent.click(screen.getByTestId('confirm-dialog-ok'));
    await act(async () => {
      await pending;
    });
    expect(addDraftRows).toHaveBeenCalledWith([{ name: 'cross-name' }]);
  });

  it('pasteRowsAsDrafts refuses a clipboard that holds no DataZen rows', async () => {
    readClipboard.mockResolvedValue('from excel\tdata');
    const hook = mountClipboard(new Set([2]));
    let consumed = false;
    await act(async () => {
      consumed = await hook.current().pasteRowsAsDrafts();
    });
    expect(consumed).toBe(true);
    expect(addDraftRows).not.toHaveBeenCalled();
    expect(hook.current().problems.map((problem) => problem.code)).toContain(
      'grid.pasteRows.notRowData',
    );
  });

  it('pasteRowsAsDrafts reports when no copied column can be written', async () => {
    // Only the PK column was copied: nothing writable.
    recordRowCopy(
      { tableKey: '["c","db","","t"]', columnNames: ['id'], rows: [[7]] },
      '7',
    );
    readClipboard.mockResolvedValue('7');
    const hook = mountClipboard(new Set([2]));
    let consumed = false;
    await act(async () => {
      consumed = await hook.current().pasteRowsAsDrafts();
    });
    expect(consumed).toBe(true);
    expect(addDraftRows).not.toHaveBeenCalled();
    expect(hook.current().problems.map((problem) => problem.code)).toContain(
      'grid.pasteRows.noWritableColumn',
    );
  });
});
