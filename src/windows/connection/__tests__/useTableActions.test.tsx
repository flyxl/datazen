/**
 * Permanent guard for the extraction of the panel action table out of
 * `TableView` (DB-06 track, GC budget).
 *
 * The action table is the seam five more chapters (DB-02 keyboard, DB-03
 * clipboard, DB-05 editors, DB-06 dirty cells, DB-09 counts) add entries to,
 * so its surface is pinned here: the exact set of bindings, the store call
 * each one makes, and — the subtle one — which of them drop the "the new row
 * could not be located" notice.
 *
 * The temporary probe rendered the pre-split and the post-split `TableView`
 * side by side, replayed the same interaction script on both and compared the
 * resulting store call log (`identical: true`); this file is what keeps that
 * conclusion alive.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useTableActions } from '../useTableActions';
import type { TableChangeContext } from '../../../lib/tableChanges';

const tableStore = vi.hoisted(() => ({
  loadTableData: vi.fn(),
  reloadPanel: vi.fn(),
  setSort: vi.fn(),
  removeFilter: vi.fn(),
  clearFilters: vi.fn(),
  addFilter: vi.fn(),
  setFilters: vi.fn(),
  updateFilter: vi.fn(),
  setFilterLogic: vi.fn(),
  applyFilters: vi.fn(),
  setFilterPanelOpen: vi.fn(),
  setVisibleColumns: vi.fn(),
  setPage: vi.fn(),
  setPageSize: vi.fn(),
  startEdit: vi.fn(),
  stageCellChange: vi.fn(),
  cancelEdit: vi.fn(),
  setRowModeSelection: vi.fn(),
  toggleSelectAll: vi.fn(),
  setGridSelection: vi.fn(),
  deleteRows: vi.fn(),
  previewPendingChanges: vi.fn().mockResolvedValue(null),
  commitPendingChanges: vi.fn().mockResolvedValue({}),
  rollbackPendingChanges: vi.fn(),
  setDetailRow: vi.fn(),
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: Object.assign((sel: (s: typeof tableStore) => unknown) => sel(tableStore), {
    getState: () => tableStore,
  }),
}));

const PANEL = 'panel-users';
const CONTEXT: TableChangeContext = {
  connectionId: null,
  dbSessionId: 'c1',
  driverType: 'postgresql',
  database: 'app',
  schema: null,
  table: 'users',
};

/** Every binding the panel owns. A new chapter adds its entry here. */
const ACTION_NAMES = [
  'addFilter',
  'applyFilters',
  'cancelEdit',
  'clearFilters',
  'commitPendingChanges',
  'deleteRows',
  'load',
  'previewPendingChanges',
  'reload',
  'removeFilter',
  'rollbackPendingChanges',
  'selectRow',
  'setDetailRow',
  'setFilterLogic',
  'setFilterPanelOpen',
  'setFilters',
  'setGridSelection',
  'setPage',
  'setPageSize',
  'setSort',
  'setVisibleColumns',
  'stageCellChange',
  'startEdit',
  'toggleSelectAll',
  'updateFilter',
];

/**
 * The bindings that move the page: they drop the insert notice, because the
 * user is looking at a different set of rows than the notice talks about.
 */
const NOTICE_INVALIDATING = [
  'applyFilters',
  'clearFilters',
  'load',
  'reload',
  'rollbackPendingChanges',
  'setFilters',
  'setPage',
  'setPageSize',
  'setSort',
];

function setup() {
  const invalidateNotice = vi.fn();
  const { result } = renderHook(() =>
    useTableActions({ panelId: PANEL, context: CONTEXT, invalidateNotice }),
  );
  return { actions: result.current, invalidateNotice };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('useTableActions', () => {
  it('exposes exactly the known bindings', () => {
    const { actions } = setup();
    expect(Object.keys(actions).sort()).toEqual(ACTION_NAMES);
  });

  it('scopes every store call to the panel id', () => {
    const { actions } = setup();

    actions.reload();
    expect(tableStore.reloadPanel).toHaveBeenCalledWith(PANEL);

    actions.startEdit(3, 'name');
    expect(tableStore.startEdit).toHaveBeenCalledWith(PANEL, 3, 'name');

    actions.stageCellChange(3, 'name', 'Bob');
    expect(tableStore.stageCellChange).toHaveBeenCalledWith(PANEL, 3, 'name', 'Bob');

    actions.deleteRows([1, 2]);
    expect(tableStore.deleteRows).toHaveBeenCalledWith(PANEL, [1, 2]);

    actions.setPage(4);
    expect(tableStore.setPage).toHaveBeenCalledWith(PANEL, 4);

    actions.setPageSize(100);
    expect(tableStore.setPageSize).toHaveBeenCalledWith(PANEL, 100);

    actions.setVisibleColumns(['id']);
    expect(tableStore.setVisibleColumns).toHaveBeenCalledWith(PANEL, ['id']);

    actions.setDetailRow(2);
    expect(tableStore.setDetailRow).toHaveBeenCalledWith(PANEL, 2);

    actions.rollbackPendingChanges();
    expect(tableStore.rollbackPendingChanges).toHaveBeenCalledWith(PANEL);
  });

  it('loads the panel against its own target context', () => {
    const { actions } = setup();
    actions.load();
    expect(tableStore.loadTableData).toHaveBeenCalledWith({ panelId: PANEL, ...CONTEXT });
  });

  it('drops the insert notice exactly for the page-moving bindings', () => {
    const invalidating: string[] = [];
    for (const name of ACTION_NAMES) {
      const { actions, invalidateNotice } = setup();
      const action = (actions as unknown as Record<string, () => void>)[name];
      action();
      if (invalidateNotice.mock.calls.length > 0) invalidating.push(name);
    }
    expect(invalidating.sort()).toEqual([...NOTICE_INVALIDATING].sort());
  });

  it('keeps the notice when the binding does not move the page', () => {
    const { actions, invalidateNotice } = setup();
    actions.startEdit(0, 'id');
    actions.setFilterPanelOpen(true);
    actions.setVisibleColumns(null);
    actions.setDetailRow(null);
    expect(invalidateNotice).not.toHaveBeenCalled();
  });
});
