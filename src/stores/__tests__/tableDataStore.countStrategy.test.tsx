import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, act, waitFor } from '@testing-library/react';
import { useEffect } from 'react';

/**
 * Independent re-derivation of the count-strategy contract (PRD 09 §4.2 /
 * §7 B8 / §9.2), at the level of the **request** rather than of the pure
 * helper.
 *
 * The implementer's own test asserts `objectContaining({ countStrategy:
 * undefined })`, which passes whether the field is absent or present-and-
 * undefined. This file asserts the stronger thing the contract asks for: in
 * the automatic mode the request must not carry the field **at all** — the
 * driver decides, and the request must not pre-empt it.
 */
const { mockDatabaseCommands } = vi.hoisted(() => ({
  mockDatabaseCommands: { getTableData: vi.fn() },
}));

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

const PANEL = 'panel-users';
const CONTEXT = {
  connectionId: 'conn-1',
  dbSessionId: 'conn-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'users',
};

const columns = [
  { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
];

function pageRows(count: number): (string | number | null)[][] {
  return Array.from({ length: count }, (_, index) => [index + 1, `row-${index + 1}`]);
}

const invalidateNotice = () => {};

function lastRequest(): Record<string, unknown> {
  const call = mockDatabaseCommands.getTableData.mock.calls.at(-1);
  if (!call) throw new Error('no request was sent');
  return call[0] as Record<string, unknown>;
}

describe('the count strategy on the wire', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;
  let useSettingsStore: typeof import('../settingsStore').useSettingsStore;
  let useSchemaStore: typeof import('../schemaStore').useSchemaStore;
  let useTableActions: typeof import('../../windows/connection/useTableActions').useTableActions;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns,
      rows: pageRows(50),
      totalRows: 1234,
      totalRowsKind: 'exact',
      page: 0,
      pageSize: 50,
    });
    const store = await import('../tableDataStore');
    useTableDataStore = store.useTableDataStore;
    useTableDataStore.getState().reset();
    useSettingsStore = (await import('../settingsStore')).useSettingsStore;
    useSchemaStore = (await import('../schemaStore')).useSchemaStore;
    useTableActions = (await import('../../windows/connection/useTableActions')).useTableActions;
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, rowCountMode: 'auto', rowCountEstimateThreshold: 200_000 },
    }));
    // The catalog knows nothing about this table: no prior, no opinion.
    useSchemaStore.setState((state) => ({ schemas: new Map(state.schemas) }));
  });

  afterEach(cleanup);

  async function loadTable() {
    await useTableDataStore.getState().loadTableData({ panelId: PANEL, ...CONTEXT });
  }

  it('sends no countStrategy at all in the automatic mode without a known size', async () => {
    await loadTable();
    const request = lastRequest();
    // Not "auto", not "exact" — the driver decides.
    expect(request.countStrategy).toBeUndefined();
    expect(request.countStrategy).not.toBe('auto');
    expect(request.countStrategy).not.toBe('exact');
    // And the field does not survive serialisation, which is what the host
    // actually receives.
    expect(Object.keys(JSON.parse(JSON.stringify(request)))).not.toContain('countStrategy');
  });

  it('sends the explicit modes verbatim', async () => {
    for (const [mode, expected] of [
      ['exact', 'exact'],
      ['estimated', 'estimated'],
      ['none', 'unsupported'],
    ] as const) {
      useSettingsStore.setState((state) => ({ settings: { ...state.settings, rowCountMode: mode } }));
      mockDatabaseCommands.getTableData.mockClear();
      await loadTable();
      expect(lastRequest().countStrategy).toBe(expected);
    }
  });

  it('keeps the setting across every path that re-reads the table', async () => {
    // The commit's promise: "do not count" and "always exact" survive every
    // re-read. Each entry below is a path that triggers a read.
    const paths: { name: string; run: () => void }[] = [
      { name: 'setPage', run: () => useTableDataStore.getState().setPage(PANEL, 1) },
      { name: 'setPageSize', run: () => useTableDataStore.getState().setPageSize(PANEL, 25) },
      {
        name: 'setSort',
        run: () => useTableDataStore.getState().setSort(PANEL, { column: 'name', descending: false }),
      },
      {
        name: 'setFilters',
        run: () =>
          useTableDataStore.getState().setFilters(PANEL, [
            { column: 'name', operator: 'eq', value: 'Alice' },
          ]),
      },
      { name: 'clearFilters', run: () => useTableDataStore.getState().clearFilters(PANEL) },
      {
        name: 'setDefaultOrderOverride',
        run: () => useTableDataStore.getState().setDefaultOrderOverride(PANEL, 'none'),
      },
      { name: 'reloadPanel', run: () => useTableDataStore.getState().reloadPanel(PANEL) },
    ];

    for (const mode of ['none', 'exact', 'estimated'] as const) {
      const expected = mode === 'none' ? 'unsupported' : mode;
      useSettingsStore.setState((state) => ({ settings: { ...state.settings, rowCountMode: mode } }));
      // The panel has to exist first: reloading a panel that was never loaded
      // is a no-op, not a read.
      await loadTable();
      for (const path of paths) {
        mockDatabaseCommands.getTableData.mockClear();
        act(() => {
          path.run();
        });
        await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
        expect(lastRequest().countStrategy, `${mode} / ${path.name}`).toBe(expected);
      }
    }
  });

  it('lets the panel read use the size the catalog knows', async () => {
    // The panel's first read knows the table is huge, so the automatic mode
    // asks for an estimate instead of counting.
    const { createEmptyConnectionSchema } = await import('../schemaStoreState');
    useSchemaStore.setState((state) => {
      const schemas = new Map(state.schemas);
      schemas.set(
        'conn-1',
        Object.assign(createEmptyConnectionSchema(), {
          tables: [
            { name: 'users', schema: null, tableType: 'table' as const, rowCount: 5_000_000 },
          ],
        }),
      );
      return { schemas };
    });

    function Panel() {
      const actions = useTableActions({ panelId: PANEL, context: CONTEXT, invalidateNotice });
      useEffect(() => {
        actions.load();
      }, [actions]);
      return null;
    }
    render(<Panel />);
    await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(lastRequest().countStrategy).toBe('estimated');
  });

  it('asks for the same strategy on the first read and on a re-read', async () => {
    // The first read weighs the size the catalog knows, and so must every
    // re-read: a table **below** the threshold asks for an exact count at
    // first, and it has to keep asking for it. Otherwise "nothing" is sent
    // afterwards, which means the driver's own declaration — and for an
    // estimating driver that is an estimate, so the panel's number changes
    // shape on a refresh.
    const { createEmptyConnectionSchema } = await import('../schemaStoreState');
    useSchemaStore.setState((state) => {
      const schemas = new Map(state.schemas);
      schemas.set(
        'conn-1',
        Object.assign(createEmptyConnectionSchema(), {
          tables: [
            { name: 'users', schema: null, tableType: 'table' as const, rowCount: 100 },
          ],
        }),
      );
      return { schemas };
    });

    function Panel() {
      const actions = useTableActions({ panelId: PANEL, context: CONTEXT, invalidateNotice });
      useEffect(() => {
        actions.load();
      }, [actions]);
      return null;
    }
    render(<Panel />);
    await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    const firstRead = lastRequest().countStrategy;

    mockDatabaseCommands.getTableData.mockClear();
    act(() => {
      useTableDataStore.getState().reloadPanel(PANEL);
    });
    await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    const reRead = lastRequest().countStrategy;

    expect(firstRead).toBe('exact');
    expect(reRead).toBe('exact');
  });
});

describe('the exact count is a full re-read', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;
  let useSettingsStore: typeof import('../settingsStore').useSettingsStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns,
      rows: pageRows(50),
      totalRows: 1234,
      totalRowsKind: 'estimated',
      page: 0,
      pageSize: 50,
    });
    const store = await import('../tableDataStore');
    useTableDataStore = store.useTableDataStore;
    useTableDataStore.getState().reset();
    useSettingsStore = (await import('../settingsStore')).useSettingsStore;
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, rowCountMode: 'estimated' },
    }));
  });

  it('asks for the exact count with a full re-read, never with skipCount', async () => {
    await useTableDataStore.getState().loadTableData({ panelId: PANEL, ...CONTEXT });
    mockDatabaseCommands.getTableData.mockClear();
    act(() => {
      useTableDataStore.getState().reloadPanel(PANEL, { skipCount: false, countStrategy: 'exact' });
    });
    await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    const request = lastRequest();
    expect(request.countStrategy).toBe('exact');
    expect(request.skipCount).toBe(false);
  });

  it('keeps the estimated state and the error when the exact count fails', async () => {
    await useTableDataStore.getState().loadTableData({ panelId: PANEL, ...CONTEXT });
    expect(useTableDataStore.getState().byPanel.get(PANEL)?.totalRowsKind).toBe('estimated');

    // A **registered** code, so the message in the test is one the back end
    // can actually raise: `grid.count.*` is an empty domain (the count
    // notices are front-end facts, not error codes), and a made-up code in a
    // test sends the next reader looking for a producer that does not exist.
    mockDatabaseCommands.getTableData.mockRejectedValueOnce('grid.cell.readOnly: nope');
    await act(async () => {
      useTableDataStore.getState().reloadPanel(PANEL, { skipCount: false, countStrategy: 'exact' });
    });
    await waitFor(() => {
      expect(useTableDataStore.getState().byPanel.get(PANEL)?.error).toBeTruthy();
    });
    const slice = useTableDataStore.getState().byPanel.get(PANEL);
    // The estimate is still on screen — the failed count does not erase it.
    expect(slice?.totalRowsKind).toBe('estimated');
    expect(slice?.error).toContain('grid.cell.readOnly');
  });
});
