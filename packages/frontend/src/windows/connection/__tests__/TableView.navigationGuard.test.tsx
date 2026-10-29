/**
 * The navigation guard of DB-05 (05 §4.4).
 *
 * The contract: "编辑中的行被卸载（翻页/筛选/排序/刷新）→ 导航前先
 * `resolveEditorBeforeNavigation()`：合法提交、非法**中止导航**并保留编辑器与行内错误
 * （禁止静默丢弃用户的键入）".
 *
 * `useTableActions` guards every binding that re-reads the page: `load`,
 * `reload`, `countExact`, `setPage`, `setPageSize`, `setSort`, `applyFilters`,
 * `clearFilters`, `setFilters` and `setDefaultOrderOverride`.
 *
 * The two paths that were **not** guarded — `load` and `countExact` — both
 * end in `loadTableData`, which sets `editingCell: null` (see
 * `src/stores/tableDataStore.ts`): the open editor unmounts and the draft
 * the user typed is dropped without ever being written.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useTableActions } from '../useTableActions';
import { registerActiveCellEditor } from '../../../components/DataTable/editors/activeEditor';
import type { TableChangeContext } from '../../../lib/tableChanges';

const tableStore = vi.hoisted(() => {
  const record = () => vi.fn((...args: unknown[]) => void args);
  return {
    loadTableData: record(),
    reloadPanel: record(),
    setSort: record(),
    removeFilter: record(),
    clearFilters: record(),
    addFilter: record(),
    setFilters: record(),
    updateFilter: record(),
    setFilterLogic: record(),
    applyFilters: record(),
    setFilterPanelOpen: record(),
    setVisibleColumns: record(),
    setPage: record(),
    setPageSize: record(),
    startEdit: record(),
    stageCellChange: record(),
    cancelEdit: record(),
    setRowModeSelection: record(),
    toggleSelectAll: record(),
    setGridSelection: record(),
    deleteRows: record(),
    unstageRowDelete: record(),
    previewPendingChanges: record(),
    commitPendingChanges: record(),
    rollbackPendingChanges: record(),
    setDetailRow: record(),
    setDefaultOrderOverride: record(),
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

function setup() {
  const invalidateNotice = vi.fn();
  const { result } = renderHook(() =>
    useTableActions({ panelId: PANEL, context: CONTEXT, invalidateNotice }),
  );
  return { actions: result.current, invalidateNotice };
}

/** An editor that holds a draft the editor refuses (an invalid value). */
function openEditorWithAnInvalidDraft(): () => void {
  return registerActiveCellEditor({
    panelId: PANEL,
    commit: () => 'invalidNumber',
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('verify: the navigation guard covers every page-moving binding', () => {
  it('refuses the exact-count re-read while an editor holds an invalid draft', () => {
    const release = openEditorWithAnInvalidDraft();
    const { actions } = setup();

    actions.countExact();

    // The contract: the navigation is aborted, the editor stays open.
    expect(tableStore.reloadPanel).not.toHaveBeenCalled();
    release();
  });

  it('refuses the retry load while an editor holds an invalid draft', () => {
    const release = openEditorWithAnInvalidDraft();
    const { actions } = setup();

    actions.load();

    expect(tableStore.loadTableData).not.toHaveBeenCalled();
    release();
  });

  it('refuses a page change while an editor holds an invalid draft', () => {
    const release = openEditorWithAnInvalidDraft();
    const { actions } = setup();

    actions.setPage(2);

    expect(tableStore.setPage).not.toHaveBeenCalled();
    release();
  });
});
