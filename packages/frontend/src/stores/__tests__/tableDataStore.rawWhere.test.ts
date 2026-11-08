/**
 * The DBR-04 filter channels: the mode drafts, the Any Column expansion and
 * the raw `WHERE` predicate.
 *
 * The pinned rules:
 *
 * * **at most one applied channel at a time** — applying structured filters
 *   clears the raw predicate and applying raw clears the structured list;
 * * **mode drafts survive switches and applies** — switching panes never
 *   discards unapplied work, and Clear is the only thing that starts fresh;
 * * **every apply resets the page to the first one** and carries the channel
 *   on the wire;
 * * an empty Any Column value or raw predicate **clears** the filter — it
 *   never asks a question that matches nothing.
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

const PANEL = 'panel-dbr04';

const context = {
  connectionId: 'conn-1',
  dbSessionId: 'session-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'orders',
};

import type { FilterCondition } from '../../types';

const conditions: FilterCondition[] = [
  { column: 'name', operator: 'contains', value: 'gold' },
  { column: 'notes', operator: 'contains', value: 'gold', connector: 'or' },
];

async function createPanel() {
  const { useTableDataStore } = await import('../tableDataStore');
  const store = useTableDataStore.getState();
  if (!useTableDataStore.getState().byPanel.has(PANEL)) {
    // Load the panel through the real entry so the slice and the fetch run.
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns: [],
      rows: [],
      totalRows: 0,
      page: 0,
      pageSize: 50,
    });
    await store.loadTableData({ panelId: PANEL, ...context });
  }
  return useTableDataStore;
}

/** The store handle `createPanel` resolves to. */
type StoreHandle = Awaited<ReturnType<typeof createPanel>>;

function panelOf(store: StoreHandle) {
  return store.getState().byPanel.get(PANEL)!;
}

describe('tableDataStore DBR-04 filter channels', () => {
  let useTableDataStore: StoreHandle;

  beforeEach(async () => {
    vi.clearAllMocks();
    const mod = await import('../tableDataStore');
    mod.useTableDataStore.getState().reset();
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns: [],
      rows: [],
      totalRows: 0,
      page: 0,
      pageSize: 50,
    });
    useTableDataStore = mod.useTableDataStore;
  });

  it('a fresh panel starts in the structured mode with both drafts empty', async () => {
    await createPanel();
    const panel = panelOf(useTableDataStore);
    expect(panel.filterMode).toBe('structured');
    expect(panel.anyColumnDraft).toBe('');
    expect(panel.rawWhereDraft).toBe('');
    expect(panel.rawWhere).toBeNull();
  });

  it('switching modes is render-only: no fetch, no draft loss', async () => {
    await createPanel();
    const before = mockDatabaseCommands.getTableData.mock.calls.length;
    const store = useTableDataStore.getState();
    store.setAnyColumnDraft(PANEL, 'gold');
    store.setRawWhereDraft(PANEL, "total > 100");
    store.setFilterMode(PANEL, 'rawSql');
    store.setFilterMode(PANEL, 'anyColumn');
    store.setFilterMode(PANEL, 'structured');
    // No reload happened for any of the switches.
    expect(mockDatabaseCommands.getTableData.mock.calls.length).toBe(before);
    // Both drafts survived every switch.
    const panel = panelOf(useTableDataStore);
    expect(panel.anyColumnDraft).toBe('gold');
    expect(panel.rawWhereDraft).toBe('total > 100');
    expect(panel.rawWhere).toBeNull();
  });

  it('applying the raw predicate replaces the structured channel and resets the page', async () => {
    await createPanel();
    const store = useTableDataStore.getState();
    store.setFilters(PANEL, [
      { column: 'status', operator: 'eq', value: 'paid' },
    ]);
    await vi.waitFor(() => {
      expect(panelOf(useTableDataStore).filters).toHaveLength(1);
    });
    // Move away from the first page so the reset is observable.
    useTableDataStore.getState().setPage(PANEL, 3);
    useTableDataStore.getState().setRawWhereDraft(PANEL, 'total > 100');
    useTableDataStore.getState().applyRawWhere(PANEL, '  total > 100  ');
    await vi.waitFor(() => {
      const panel = panelOf(useTableDataStore);
      expect(panel.rawWhere).toBe('total > 100');
    });
    const panel = panelOf(useTableDataStore);
    expect(panel.filters).toEqual([]);
    expect(panel.draftFilters).toEqual([]);
    expect(panel.page).toBe(0);
    // The read carried the predicate, trimmed.
    const lastCall = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0];
    expect(lastCall?.rawWhere).toBe('total > 100');
  });

  it('applying structured filters clears the raw channel', async () => {
    await createPanel();
    const store = useTableDataStore.getState();
    store.applyRawWhere(PANEL, 'total > 100');
    await vi.waitFor(() => {
      expect(panelOf(useTableDataStore).rawWhere).toBe('total > 100');
    });
    useTableDataStore.getState().setFilters(PANEL, [
      { column: 'status', operator: 'eq', value: 'paid' },
    ]);
    await vi.waitFor(() => {
      expect(panelOf(useTableDataStore).filters).toHaveLength(1);
    });
    expect(panelOf(useTableDataStore).rawWhere).toBeNull();
    // The read no longer carries a raw predicate.
    const lastCall = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0];
    expect(lastCall?.rawWhere ?? null).toBeNull();
  });

  it('an empty raw predicate clears the filter instead of sending one', async () => {
    await createPanel();
    const store = useTableDataStore.getState();
    store.applyRawWhere(PANEL, 'total > 100');
    await vi.waitFor(() => {
      expect(panelOf(useTableDataStore).rawWhere).toBe('total > 100');
    });
    useTableDataStore.getState().applyRawWhere(PANEL, '   ');
    await vi.waitFor(() => {
      expect(panelOf(useTableDataStore).rawWhere).toBeNull();
    });
    const lastCall = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0];
    expect(lastCall?.rawWhere ?? null).toBeNull();
  });

  it('the Any Column expansion applies as OR-joined contains conditions', async () => {
    await createPanel();
    const store = useTableDataStore.getState();
    store.applyAnyColumn(PANEL, { conditions });
    await vi.waitFor(() => {
      expect(panelOf(useTableDataStore).filters).toHaveLength(2);
    });
    const panel = panelOf(useTableDataStore);
    expect(panel.filterLogic).toBe('or');
    expect(panel.filters).toEqual(conditions);
    expect(panel.rawWhere).toBeNull();
    expect(panel.page).toBe(0);
  });

  it('an empty Any Column expansion clears the filter', async () => {
    await createPanel();
    const store = useTableDataStore.getState();
    store.setFilters(PANEL, [
      { column: 'status', operator: 'eq', value: 'paid' },
    ]);
    await vi.waitFor(() => {
      expect(panelOf(useTableDataStore).filters).toHaveLength(1);
    });
    useTableDataStore.getState().applyAnyColumn(PANEL, { conditions: [] });
    await vi.waitFor(() => {
      expect(panelOf(useTableDataStore).filters).toHaveLength(0);
    });
  });

  it('clearFilters clears every channel, every draft and the mode', async () => {
    await createPanel();
    const store = useTableDataStore.getState();
    store.setAnyColumnDraft(PANEL, 'gold');
    store.setFilterMode(PANEL, 'rawSql');
    store.applyRawWhere(PANEL, 'total > 100');
    await vi.waitFor(() => {
      expect(panelOf(useTableDataStore).rawWhere).toBe('total > 100');
    });
    useTableDataStore.getState().clearFilters(PANEL);
    await vi.waitFor(() => {
      expect(panelOf(useTableDataStore).rawWhere).toBeNull();
    });
    const panel = panelOf(useTableDataStore);
    expect(panel.anyColumnDraft).toBe('');
    expect(panel.rawWhereDraft).toBe('');
    expect(panel.filterMode).toBe('structured');
    expect(panel.page).toBe(0);
  });
});
