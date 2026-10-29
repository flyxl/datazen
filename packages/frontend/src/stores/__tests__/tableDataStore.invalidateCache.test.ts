/**
 * `invalidateCachedData` keeps a panel that has an open editor (DB-05).
 *
 * The invalidation drops the rows, the editor unmounts with them, and the
 * draft the user typed is lost without ever being written (05 §4.4).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useTableDataStore } from '../tableDataStore';
import { emptyTableState } from '../tableData/connectionState';
import type { TableChangeContext } from '../../lib/tableChanges';

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getTableData: vi.fn(),
    previewPendingChanges: vi.fn(),
    commitPendingChanges: vi.fn(),
  },
}));

const CONTEXT: TableChangeContext = {
  connectionId: 'conn-1',
  dbSessionId: 'sess-1',
  driverType: 'postgresql',
  database: 'app',
  schema: null,
  table: 'users',
};

const PANEL = 'panel-invalidate';

function seed(editing: boolean): void {
  const state = {
    ...emptyTableState(CONTEXT),
    columns: [{ name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true }],
    rows: [{ id: 1 }],
    totalRows: 1,
    editingCell: editing ? { row: 0, col: 'id' } : null,
  };
  useTableDataStore.setState({ byPanel: new Map([[PANEL, state]]) });
}

function slice() {
  const ts = useTableDataStore.getState().byPanel.get(PANEL);
  if (!ts) throw new Error('panel slice missing');
  return ts;
}

beforeEach(() => {
  useTableDataStore.getState().reset();
});

describe('verify: invalidateCachedData keeps a panel that has an open editor', () => {
  it('leaves the panel alone while an editor is open', () => {
    seed(true);
    useTableDataStore.getState().invalidateCachedData('sess-1');
    const ts = slice();
    expect(ts.rows).toHaveLength(1);
    expect(ts.columns).toHaveLength(1);
    expect(ts.editingCell).toEqual({ row: 0, col: 'id' });
  });

  it('still invalidates a panel without an open editor', () => {
    seed(false);
    useTableDataStore.getState().invalidateCachedData('sess-1');
    const ts = slice();
    expect(ts.rows).toHaveLength(0);
    expect(ts.columns).toHaveLength(0);
    expect(ts.editingCell).toBeNull();
  });
});
