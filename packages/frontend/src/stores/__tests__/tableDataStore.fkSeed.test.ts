/**
 * The first-frame seed of a panel (DB-07).
 *
 * A foreign-key jump opens a tab that has to show the referenced row right
 * away. The seed is what makes that possible: it writes the conditions into
 * the slice **before** the tab's first load, so the one fetch carries them.
 *
 * The two properties that matter: the seed never triggers a fetch of its own
 * (a second fetch would show the whole table first), and it never bumps the
 * request revision (an in-flight response of the old request must not be
 * allowed to commit over it).
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

const PANEL = 'panel-seeded';

const context = {
  connectionId: 'conn-1',
  dbSessionId: 'session-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'customers',
};

describe('tableDataStore.seedPanel', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  beforeEach(async () => {
    vi.clearAllMocks();
    useTableDataStore = (await import('../tableDataStore')).useTableDataStore;
    useTableDataStore.getState().reset();
  });

  it('creates the slice with the conditions of the jump', () => {
    useTableDataStore.getState().seedPanel(PANEL, context, {
      filters: [
        { column: 'id', operator: 'eq', value: 7 },
        { column: 'tenant', operator: 'eq', value: 'acme' },
      ],
    });

    const slice = useTableDataStore.getState().byPanel.get(PANEL);
    expect(slice).toBeDefined();
    expect(slice?.filters).toEqual([
      { column: 'id', operator: 'eq', value: 7 },
      { column: 'tenant', operator: 'eq', value: 'acme' },
    ]);
    // The conditions are visible: the draft carries them and the panel is open.
    expect(slice?.draftFilters).toHaveLength(2);
    expect(slice?.filterPanelOpen).toBe(true);
    expect(slice?.filterLogic).toBe('and');
    expect(slice?.draftFilterLogic).toBe('and');
  });

  it('never triggers a fetch of its own', () => {
    useTableDataStore.getState().seedPanel(PANEL, context, {
      filters: [{ column: 'id', operator: 'eq', value: 7 }],
    });

    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();
  });

  it('never bumps the request revision', () => {
    useTableDataStore.getState().seedPanel(PANEL, context, {
      filters: [{ column: 'id', operator: 'eq', value: 7 }],
    });

    const slice = useTableDataStore.getState().byPanel.get(PANEL);
    expect(slice?.requestRevision).toBe(0);
    expect(slice?.loadingRevision).toBeNull();
  });

  it('carries the conditions into the first load of the panel', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns: [{ name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false }],
      rows: [[7]],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    });
    useTableDataStore.getState().seedPanel(PANEL, context, {
      filters: [{ column: 'id', operator: 'eq', value: 7 }],
    });

    await useTableDataStore
      .getState()
      .loadTableData({ panelId: PANEL, ...context });

    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(1);
    const request = mockDatabaseCommands.getTableData.mock.calls[0][0];
    expect(request.filters).toEqual([{ column: 'id', operator: 'eq', value: 7 }]);
  });

  it('opens the filter panel only when it is asked to', () => {
    useTableDataStore.getState().seedPanel(PANEL, context, {
      filters: [{ column: 'id', operator: 'eq', value: 7 }],
      openFilterPanel: false,
    });

    const slice = useTableDataStore.getState().byPanel.get(PANEL);
    expect(slice?.filterPanelOpen).toBe(false);
    // The conditions stay in the draft: the user can still see them.
    expect(slice?.draftFilters).toHaveLength(1);
  });
});
