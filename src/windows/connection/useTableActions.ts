import { useMemo } from 'react';
import { useTableDataStore } from '../../stores/tableDataStore';
import { resolvePanelCountStrategy } from '../../stores/tableData/countStrategy';
import { resolveEditorBeforeNavigation } from './tableEditing';
import type { GridSelection } from '../../stores/tableData/gridSelection';
import type {
  CommitPendingChangesResult,
  RowChangePlan,
  TableChangeContext,
} from '../../lib/tableChanges';
import type { FilterCondition, SortCondition } from '../../types';
import type { DefaultOrderOverride } from '../../lib/defaultSort';

/**
 * Panel-scoped action binders: every store call is namespaced by this tab's id.
 *
 * This table is the single seam between the panel UI and the store. It was
 * inlined in `TableView` until the DB-06 track extracted it, because five more
 * chapters (DB-02 keyboard, DB-03 clipboard, DB-05 editors, DB-06 dirty cells,
 * DB-09 counts) all add entries to it — with the file at 776 lines there was
 * no room left for any of them.
 *
 * The bindings are **verbatim** what `TableView` used to build: same store
 * calls, same order, same memoisation keys.
 */
export interface TableActions {
  load: () => void;
  reload: () => void;
  /** Ask for the exact row count — the answer to an estimated one. */
  countExact: () => void;
  /** Change the panel's temporary override of the default order. */
  setDefaultOrderOverride: (override: DefaultOrderOverride) => void;
  setSort: (sort: SortCondition) => void;
  removeFilter: (index: number) => void;
  clearFilters: () => void;
  addFilter: (filter: FilterCondition) => void;
  setFilters: (filters: FilterCondition[], logic?: 'and' | 'or') => void;
  updateFilter: (index: number, filter: FilterCondition) => void;
  setFilterLogic: (logic: 'and' | 'or') => void;
  applyFilters: () => void;
  setFilterPanelOpen: (open: boolean) => void;
  setVisibleColumns: (columns: string[] | null) => void;
  setPage: (page: number) => void;
  setPageSize: (size: number) => void;
  startEdit: (row: number, col: string) => void;
  stageCellChange: (row: number, col: string, value: unknown) => void;
  cancelEdit: () => void;
  selectRow: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  toggleSelectAll: () => void;
  setGridSelection: (selection: GridSelection) => void;
  deleteRows: (rowIndices: number[]) => void;
  /** Undo the delete mark of one row (DB-06). */
  unstageRowDelete: (rowIndex: number) => void;
  previewPendingChanges: () => Promise<RowChangePlan | null>;
  commitPendingChanges: () => Promise<CommitPendingChangesResult>;
  rollbackPendingChanges: () => void;
  setDetailRow: (index: number | null) => void;
}

export interface UseTableActionsOptions {
  /** Panel (tab) every action is scoped to. */
  panelId: string;
  /** The panel's immutable load target. */
  context: TableChangeContext;
  /**
   * Drops the "the new row could not be located" notice.
   *
   * Every action that moves the page invalidates the notice: the user is
   * looking at a different set of rows than the one the notice talks about.
   */
  invalidateNotice: () => void;
}

/**
 * The panel's action table, memoised on the panel id and the load target.
 *
 * `invalidateNotice` has to be stable (the caller owns it), otherwise the
 * memoisation key would change on every render and the table would be rebuilt
 * on every keystroke.
 */
export function useTableActions({
  panelId,
  context,
  invalidateNotice,
}: UseTableActionsOptions): TableActions {
  return useMemo(() => {
    const store = () => useTableDataStore.getState();
    /**
     * Whether a navigation may go ahead (DB-05).
     *
     * An open editor holds a draft the user typed. Navigating would take it
     * with it, so the editor is asked to write it first — and a draft that
     * is not acceptable **stops** the navigation instead of being dropped.
     */
    const mayNavigate = () => resolveEditorBeforeNavigation(panelId).ok;
    return {
      load: () => {
        // A re-read drops the open editor: the loaded page has no editing
        // cell any more, and the editor's unmount never writes (05 §4.4).
        // The draft is therefore written first, or the navigation is
        // refused — never silently dropped.
        if (!mayNavigate()) return;
        // The "retry" of a failed load re-reads the page, so the notice of the
        // previous commit is stale — exactly like a plain reload.
        invalidateNotice();
        void store().loadTableData({
          panelId,
          ...context,
          countStrategy: resolvePanelCountStrategy(context),
        });
      },
      reload: () => {
        if (!mayNavigate()) return;
        invalidateNotice();
        store().reloadPanel(panelId);
      },
      countExact: () => {
        // The exact count is a **full** re-read, not a paged one: the estimate
        // is replaced by the real number, which means the count has to run
        // again. A re-read drops the open editor, so the draft goes first.
        if (!mayNavigate()) return;
        invalidateNotice();
        store().reloadPanel(panelId, { skipCount: false, countStrategy: 'exact' });
      },
      setDefaultOrderOverride: (override) => {
        if (!mayNavigate()) return;
        invalidateNotice();
        store().setDefaultOrderOverride(panelId, override);
      },
      setSort: (sort: SortCondition) => {
        if (!mayNavigate()) return;
        invalidateNotice();
        store().setSort(panelId, sort);
      },
      removeFilter: (index: number) => store().removeFilter(panelId, index),
      clearFilters: () => {
        if (!mayNavigate()) return;
        invalidateNotice();
        store().clearFilters(panelId);
      },
      addFilter: (filter: FilterCondition) => store().addFilter(panelId, filter),
      setFilters: (filters: FilterCondition[], logic?: 'and' | 'or') => {
        if (!mayNavigate()) return;
        invalidateNotice();
        store().setFilters(panelId, filters, logic);
      },
      updateFilter: (index: number, filter: FilterCondition) =>
        store().updateFilter(panelId, index, filter),
      setFilterLogic: (logic: 'and' | 'or') => store().setFilterLogic(panelId, logic),
      applyFilters: () => {
        if (!mayNavigate()) return;
        invalidateNotice();
        store().applyFilters(panelId);
      },
      setFilterPanelOpen: (open: boolean) => store().setFilterPanelOpen(panelId, open),
      setVisibleColumns: (columns: string[] | null) =>
        store().setVisibleColumns(panelId, columns),
      setPage: (page: number) => {
        if (!mayNavigate()) return;
        invalidateNotice();
        store().setPage(panelId, page);
      },
      setPageSize: (size: number) => {
        if (!mayNavigate()) return;
        invalidateNotice();
        store().setPageSize(panelId, size);
      },
      startEdit: (row: number, col: string) => store().startEdit(panelId, row, col),
      stageCellChange: (row: number, col: string, value: unknown) =>
        store().stageCellChange(panelId, row, col, value),
      cancelEdit: () => store().cancelEdit(panelId),
      // Row gutter (DB-01 §4.2): row-level selection must also switch the mode to 'row',
      // otherwise `selectedRows` paints a highlight the selection model cannot explain.
      selectRow: (index: number, opts?: { multi?: boolean; range?: boolean }) =>
        store().setRowModeSelection(panelId, index, opts),
      toggleSelectAll: () => store().toggleSelectAll(panelId),
      // Cell/region selection (DB-01): the store owns the mode, the grid only reports.
      setGridSelection: (selection: GridSelection) =>
        store().setGridSelection(panelId, selection),
      deleteRows: (rowIndices: number[]) => store().deleteRows(panelId, rowIndices),
      // Restoring a row is not a page move: the notice of the previous commit
      // still describes the rows the user is looking at.
      unstageRowDelete: (rowIndex: number) => store().unstageRowDelete(panelId, rowIndex),
      previewPendingChanges: () => store().previewPendingChanges(panelId),
      commitPendingChanges: () => store().commitPendingChanges(panelId),
      rollbackPendingChanges: () => {
        invalidateNotice();
        store().rollbackPendingChanges(panelId);
      },
      setDetailRow: (index: number | null) => store().setDetailRow(panelId, index),
    };
  }, [context, panelId, invalidateNotice]);
}
