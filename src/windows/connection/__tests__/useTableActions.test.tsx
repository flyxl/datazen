/**
 * Permanent guard for the extraction of the panel action table out of
 * `TableView` (DB-06 track, GC budget).
 *
 * The action table is the seam five more chapters (DB-02 keyboard, DB-03
 * clipboard, DB-05 editors, DB-06 dirty cells, DB-09 counts) add entries to.
 *
 * The guard has **two halves**, and the second one exists because the first
 * one was measured to be blind:
 *
 * 1. the exact set of bindings, and which of them drop the "the new row could
 *    not be located" notice;
 * 2. **what every binding actually calls**. The action table is a hook — it
 *    renders no DOM — so a golden-file comparison of the rendered markup
 *    cannot see a binding that calls the wrong store method. The only thing
 *    that catches it is calling each binding and looking at the store.
 *
 * The temporary probe rendered the pre-split and the post-split `TableView`
 * side by side, replayed the same interaction script on both and compared the
 * resulting store call log (`identical: true`); this file is what keeps that
 * conclusion alive.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useTableActions } from '../useTableActions';
import type { TableActions } from '../useTableActions';
import type { TableChangeContext } from '../../../lib/tableChanges';
import type { GridSelection } from '../../../stores/tableData/gridSelection';
import type { FilterCondition, SortCondition } from '../../../types';

/** One recorded store call: the method that was called and its arguments. */
interface StoreCall {
  method: string;
  args: unknown[];
}

const tableStore = vi.hoisted(() => {
  const calls: StoreCall[] = [];
  // The recorder is a spy as well, so the older `toHaveBeenCalledWith`
  // assertions keep working next to the recorded call log.
  const record = (method: string) =>
    vi.fn((...args: unknown[]) => {
      calls.push({ method, args });
    });
  const recordAsync = (method: string) =>
    vi.fn((...args: unknown[]) => {
      calls.push({ method, args });
      return Promise.resolve(null);
    });
  return {
    calls,
    loadTableData: record('loadTableData'),
    reloadPanel: record('reloadPanel'),
    setSort: record('setSort'),
    removeFilter: record('removeFilter'),
    clearFilters: record('clearFilters'),
    addFilter: record('addFilter'),
    setFilters: record('setFilters'),
    updateFilter: record('updateFilter'),
    setFilterLogic: record('setFilterLogic'),
    applyFilters: record('applyFilters'),
    setFilterPanelOpen: record('setFilterPanelOpen'),
    setVisibleColumns: record('setVisibleColumns'),
    setPage: record('setPage'),
    setPageSize: record('setPageSize'),
    startEdit: record('startEdit'),
    stageCellChange: record('stageCellChange'),
    cancelEdit: record('cancelEdit'),
    setRowModeSelection: record('setRowModeSelection'),
    toggleSelectAll: record('toggleSelectAll'),
    setGridSelection: record('setGridSelection'),
    deleteRows: record('deleteRows'),
    unstageRowDelete: record('unstageRowDelete'),
    previewPendingChanges: recordAsync('previewPendingChanges'),
    commitPendingChanges: recordAsync('commitPendingChanges'),
    rollbackPendingChanges: record('rollbackPendingChanges'),
    setDetailRow: record('setDetailRow'),
  };
});

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
  'unstageRowDelete',
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

const SORT: SortCondition = { column: 'name', descending: true };
const FILTER: FilterCondition = { column: 'name', operator: 'eq', value: 'Alice' };
const FILTERS: FilterCondition[] = [FILTER];
const SELECTION: GridSelection = {
  anchor: { rowIndex: 0, columnName: 'name' },
  focus: { rowIndex: 1, columnName: 'name' },
  extraRanges: [],
  mode: 'cell',
};

/**
 * One row of the contract: which store method a binding must call and with
 * which arguments.
 *
 * `args` are the arguments **after** the panel id — every binding is
 * panel-scoped, so the panel id is always the first argument. `load` is the
 * single exception: it passes one object (the panel id inside the load
 * parameters), and that is spelled out in its own row.
 */
interface BindingCase {
  /** The binding of the action table. */
  readonly action: keyof TableActions;
  /** The store method the binding must call — and only that one. */
  readonly method: string;
  /** The arguments the binding forwards, without the panel id. */
  readonly args: readonly unknown[];
  /** Trigger the binding. */
  readonly trigger: (actions: TableActions) => void;
}

const BINDING_CASES: readonly BindingCase[] = [
  {
    action: 'load',
    method: 'loadTableData',
    // The load takes one object; the panel id is inside it.
    args: [{ panelId: PANEL, ...CONTEXT }],
    trigger: (actions) => actions.load(),
  },
  {
    action: 'reload',
    method: 'reloadPanel',
    args: [],
    trigger: (actions) => actions.reload(),
  },
  {
    action: 'setSort',
    method: 'setSort',
    args: [SORT],
    trigger: (actions) => actions.setSort(SORT),
  },
  {
    action: 'removeFilter',
    method: 'removeFilter',
    args: [2],
    trigger: (actions) => actions.removeFilter(2),
  },
  {
    action: 'clearFilters',
    method: 'clearFilters',
    args: [],
    trigger: (actions) => actions.clearFilters(),
  },
  {
    action: 'addFilter',
    method: 'addFilter',
    args: [FILTER],
    trigger: (actions) => actions.addFilter(FILTER),
  },
  {
    action: 'setFilters',
    method: 'setFilters',
    args: [FILTERS, 'or'],
    trigger: (actions) => actions.setFilters(FILTERS, 'or'),
  },
  {
    action: 'updateFilter',
    method: 'updateFilter',
    args: [1, FILTER],
    trigger: (actions) => actions.updateFilter(1, FILTER),
  },
  {
    action: 'setFilterLogic',
    method: 'setFilterLogic',
    args: ['or'],
    trigger: (actions) => actions.setFilterLogic('or'),
  },
  {
    action: 'applyFilters',
    method: 'applyFilters',
    args: [],
    trigger: (actions) => actions.applyFilters(),
  },
  {
    action: 'setFilterPanelOpen',
    method: 'setFilterPanelOpen',
    args: [true],
    trigger: (actions) => actions.setFilterPanelOpen(true),
  },
  {
    action: 'setVisibleColumns',
    method: 'setVisibleColumns',
    args: [['id']],
    trigger: (actions) => actions.setVisibleColumns(['id']),
  },
  {
    action: 'setPage',
    method: 'setPage',
    args: [4],
    trigger: (actions) => actions.setPage(4),
  },
  {
    action: 'setPageSize',
    method: 'setPageSize',
    args: [100],
    trigger: (actions) => actions.setPageSize(100),
  },
  {
    action: 'startEdit',
    method: 'startEdit',
    args: [3, 'name'],
    trigger: (actions) => actions.startEdit(3, 'name'),
  },
  {
    action: 'stageCellChange',
    method: 'stageCellChange',
    args: [3, 'name', 'Bob'],
    trigger: (actions) => actions.stageCellChange(3, 'name', 'Bob'),
  },
  {
    action: 'cancelEdit',
    method: 'cancelEdit',
    args: [],
    trigger: (actions) => actions.cancelEdit(),
  },
  {
    action: 'selectRow',
    method: 'setRowModeSelection',
    args: [2, { multi: true, range: false }],
    trigger: (actions) => actions.selectRow(2, { multi: true, range: false }),
  },
  {
    action: 'toggleSelectAll',
    method: 'toggleSelectAll',
    args: [],
    trigger: (actions) => actions.toggleSelectAll(),
  },
  {
    action: 'setGridSelection',
    method: 'setGridSelection',
    args: [SELECTION],
    trigger: (actions) => actions.setGridSelection(SELECTION),
  },
  {
    action: 'deleteRows',
    method: 'deleteRows',
    args: [[1, 2]],
    trigger: (actions) => actions.deleteRows([1, 2]),
  },
  {
    action: 'unstageRowDelete',
    method: 'unstageRowDelete',
    args: [5],
    trigger: (actions) => actions.unstageRowDelete(5),
  },
  {
    action: 'previewPendingChanges',
    method: 'previewPendingChanges',
    args: [],
    trigger: (actions) => {
      void actions.previewPendingChanges();
    },
  },
  {
    action: 'commitPendingChanges',
    method: 'commitPendingChanges',
    args: [],
    trigger: (actions) => {
      void actions.commitPendingChanges();
    },
  },
  {
    action: 'rollbackPendingChanges',
    method: 'rollbackPendingChanges',
    args: [],
    trigger: (actions) => actions.rollbackPendingChanges(),
  },
  {
    action: 'setDetailRow',
    method: 'setDetailRow',
    args: [7],
    trigger: (actions) => actions.setDetailRow(7),
  },
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
  tableStore.calls.length = 0;
});

describe('useTableActions', () => {
  it('exposes exactly the known bindings', () => {
    const { actions } = setup();
    expect(Object.keys(actions).sort()).toEqual(ACTION_NAMES);
  });

  /**
   * The delegation contract: every binding calls **its own** store method with
   * the panel-scoped arguments — and nothing else.
   *
   * This is the half the golden files cannot cover: the action table renders
   * no DOM, so a binding that calls the wrong store method (or passes the
   * wrong arguments) is invisible to every other guard in this repository.
   */
  it('every binding calls its own store method with the panel-scoped arguments', () => {
    // The table below has to stay in step with the implementation: a binding
    // that is missing from it would simply not be checked.
    expect(BINDING_CASES.map((testCase) => testCase.action).sort()).toEqual(ACTION_NAMES);

    for (const testCase of BINDING_CASES) {
      tableStore.calls.length = 0;
      const { actions } = setup();

      testCase.trigger(actions);

      const expected: StoreCall = {
        method: testCase.method,
        args: testCase.action === 'load' ? [...testCase.args] : [PANEL, ...testCase.args],
      };
      expect(tableStore.calls, `binding "${testCase.action}"`).toEqual([expected]);
    }
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
