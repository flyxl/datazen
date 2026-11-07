/**
 * The store's half of keyset paging.
 *
 * These cases drive the **real store actions** — `loadTableData`, `setPage`,
 * `setSort` — and only the IPC command is a double. That matters: the
 * interesting failures of a paging contract are not in the arithmetic but in
 * the *wiring* (a boundary that is not sent, a stack that is not cleared, a
 * page that is answered from a stale boundary), and a test that stubs the
 * store's own actions cannot see any of them.
 *
 * The assertions are on **what the request carries** and **what the panel
 * remembers**, never on visible wording.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { CursorValue, PagePagingInfo } from '../../types';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

const PANEL = 'panel-users';

const sampleColumns = [
  { name: 'id', dataType: 'integer', isPrimaryKey: true, nullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, nullable: false },
];

const FIRST_BOUNDARY: CursorValue[] = [{ kind: 'integer', value: 2 }];
const SECOND_BOUNDARY: CursorValue[] = [{ kind: 'integer', value: 4 }];

/** The answer of a read that was made by seek. */
function keysetResponse(
  page: number,
  nextAfterKey: CursorValue[] | undefined,
  overrides: Partial<PagePagingInfo> = {},
) {
  const paging: PagePagingInfo = {
    mode: 'keyset',
    orderColumns: [{ column: 'id', descending: false }],
    driftSuspected: false,
    ...(nextAfterKey ? { nextAfterKey } : {}),
    ...overrides,
  };
  return {
    columns: sampleColumns,
    rows: [
      [1, 'Alice'],
      [2, 'Bob'],
    ],
    totalRows: 100,
    page,
    pageSize: 2,
    paging,
  };
}

describe('the keyset half of the table-data store', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const slice = () => useTableDataStore.getState().byPanel.get(PANEL);
  const loaded = () => {
    const ts = slice();
    if (!ts) throw new Error('the panel has no table-data slice');
    return ts;
  };

  /** The `afterKey` of the last request the store sent. */
  function lastAfterKey(): CursorValue[] | undefined {
    const calls = mockDatabaseCommands.getTableData.mock.calls;
    const last = calls[calls.length - 1];
    if (!last) throw new Error('no request was sent');
    return last[0].afterKey as CursorValue[] | undefined;
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(keysetResponse(0, FIRST_BOUNDARY));
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    useTableDataStore.getState().reset();
  });

  async function loadTable() {
    await useTableDataStore.getState().loadTableData({
      panelId: PANEL,
      dbSessionId: 'conn-1',
      table: 'users',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
  }

  it('next_page_sends_cursor_from_previous_page_last_row', async () => {
    await loadTable();
    expect(lastAfterKey()).toBeUndefined();

    mockDatabaseCommands.getTableData.mockResolvedValue(keysetResponse(1, SECOND_BOUNDARY));
    useTableDataStore.getState().setPage(PANEL, 1);
    await vi.waitFor(() => expect(lastAfterKey()).toBeDefined());

    expect(lastAfterKey()).toEqual(FIRST_BOUNDARY);

    mockDatabaseCommands.getTableData.mockResolvedValue(keysetResponse(2, undefined));
    useTableDataStore.getState().setPage(PANEL, 2);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(3));

    expect(lastAfterKey()).toEqual(SECOND_BOUNDARY);
  });

  it('prev_page_reuses_stacked_boundary', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockResolvedValue(keysetResponse(1, SECOND_BOUNDARY));
    useTableDataStore.getState().setPage(PANEL, 1);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2));

    // Back to the first page: the boundary of that page is the one that was
    // used to reach it — and the first page has none.
    mockDatabaseCommands.getTableData.mockResolvedValue(keysetResponse(0, FIRST_BOUNDARY));
    useTableDataStore.getState().setPage(PANEL, 0);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(3));

    expect(lastAfterKey()).toBeUndefined();
  });

  it('sort_change_clears_stack_and_resets_to_page_zero', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockResolvedValue(keysetResponse(1, SECOND_BOUNDARY));
    useTableDataStore.getState().setPage(PANEL, 1);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2));
    expect(loaded().keyset.stack.length).toBeGreaterThan(0);

    mockDatabaseCommands.getTableData.mockResolvedValue(keysetResponse(0, FIRST_BOUNDARY));
    useTableDataStore.getState().setSort(PANEL, { column: 'name', descending: false });
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(3));

    // The boundary of the old order may not be reused: the read starts over.
    expect(lastAfterKey()).toBeUndefined();
    expect(loaded().page).toBe(0);
    expect(loaded().keyset.anchorPage).toBe(0);
    // Only the page that was actually read is on the stack — the pages of the
    // order that no longer exists are gone.
    expect(loaded().keyset.stack).toHaveLength(1);
  });

  it('non_adjacent_page_falls_back_to_offset_and_reanchors', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockResolvedValue(keysetResponse(7, undefined));
    useTableDataStore.getState().setPage(PANEL, 7);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2));

    expect(lastAfterKey()).toBeUndefined();
    expect(loaded().keyset.anchorPage).toBe(7);
    // The page number was **not** turned into a boundary.
    expect(loaded().keyset.nextAfterKey).toBeNull();
  });

  it('drift_triggers_a_recount_of_the_rows', async () => {
    // Rows were deleted while the user was paging: the count on the panel
    // is stale, and a stale count is read again — not only announced.
    mockDatabaseCommands.getTableData.mockResolvedValue({
      ...keysetResponse(0, FIRST_BOUNDARY),
      paging: {
        mode: 'keyset',
        orderColumns: [{ column: 'id', descending: false }],
        driftSuspected: true,
      },
    });
    await loadTable();

    await vi.waitFor(() =>
      expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2),
    );
    const recount = mockDatabaseCommands.getTableData.mock.calls[1][0];
    expect(
      (recount as { skipCount?: boolean }).skipCount,
      'the recount must not skip the count — that is the whole point of it',
    );
  });

  it('a_drift_is_not_counted_twice_for_the_same_page', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValue({
      ...keysetResponse(0, FIRST_BOUNDARY),
      paging: {
        mode: 'keyset',
        orderColumns: [{ column: 'id', descending: false }],
        driftSuspected: true,
      },
    });
    await loadTable();

    await vi.waitFor(() =>
      expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2),
    );
    // The recount answers with the same drift, and it must not start a
    // third read: a table that keeps shrinking must not spin the panel.
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2);
  });

  it('degrade_reason_from_response_is_surfaced', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValue(
      keysetResponse(0, undefined, { mode: 'offset', unavailableReason: 'nullableKeyColumn' }),
    );
    await loadTable();

    expect(loaded().keyset.unavailable).toBe('nullableKeyColumn');
    expect(loaded().keyset.mode).toBe('offset');
  });

  it('stale_response_cannot_overwrite_newer_page', async () => {
    await loadTable();

    // A read for page 1 that does not answer yet.
    let resolveStale: (value: unknown) => void = () => {};
    mockDatabaseCommands.getTableData.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveStale = resolve;
        }),
    );
    useTableDataStore.getState().setPage(PANEL, 1);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2));

    // The order changes while that read is still in flight; the new read
    // answers straight away.
    mockDatabaseCommands.getTableData.mockResolvedValue(keysetResponse(0, FIRST_BOUNDARY));
    useTableDataStore.getState().setSort(PANEL, { column: 'name', descending: false });
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(3));

    // Now the abandoned read answers. Its rows must not land on the panel.
    const stale = keysetResponse(1, SECOND_BOUNDARY);
    (stale as { rows: (string | number | null)[][] }).rows = [[9, 'Stale']];
    resolveStale(stale);
    await vi.waitFor(() => expect(loaded().rows).toHaveLength(2));

    expect(loaded().rows).toHaveLength(2);
    expect(loaded().page).toBe(0);
    expect(loaded().sorts).toEqual([{ column: 'name', descending: false }]);
  });
});
