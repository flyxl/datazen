import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, act, waitFor } from '@testing-library/react';
import { useEffect } from 'react';

/**
 * Independent re-derivation (verifier, round 2) of the count-strategy contract
 * at the level of the **request**.
 *
 * The contract has three parts, and all three are checked here:
 *
 *  1. The **explicit** modes are sent verbatim (`exact` / `estimated` /
 *     `unsupported`), and they survive every re-read.
 *  2. The **automatic** mode weighs the size the catalog knows. That verdict
 *     must be the **same** on the first read and on every re-read — otherwise
 *     the panel's number changes shape on the first refresh (V-2).
 *  3. The automatic mode **without** a known size sends no strategy at all:
 *     the field is not merely undefined, it is absent from the serialised
 *     request body, so the driver's own declaration decides.
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

/** The keys the host actually receives (JSON is what the Tauri IPC sends). */
function wireKeys(): string[] {
  return Object.keys(JSON.parse(JSON.stringify(lastRequest())));
}

describe('round 2 · the count strategy on the wire', () => {
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
    // No prior: the catalog knows nothing about this table.
    useSchemaStore.setState((state) => ({ schemas: new Map(state.schemas) }));
  });

  afterEach(cleanup);

  async function loadTable() {
    await useTableDataStore.getState().loadTableData({ panelId: PANEL, ...CONTEXT });
  }

  /** Put a known row count into the catalog for the panel's table. */
  async function setCatalogRowCount(rowCount: number | null) {
    const { createEmptyConnectionSchema } = await import('../schemaStoreState');
    useSchemaStore.setState((state) => {
      const schemas = new Map(state.schemas);
      schemas.set(
        'conn-1',
        Object.assign(createEmptyConnectionSchema(), {
          tables: [
            { name: 'users', schema: null, tableType: 'table' as const, rowCount },
          ],
        }),
      );
      return { schemas };
    });
  }

  /** Every path that re-reads the table. */
  const reReadPaths: { name: string; run: () => void }[] = [
    { name: 'setPage', run: () => useTableDataStore.getState().setPage(PANEL, 1) },
    { name: 'setPageSize', run: () => useTableDataStore.getState().setPageSize(PANEL, 25) },
    {
      name: 'setSort',
      run: () =>
        useTableDataStore.getState().setSort(PANEL, { column: 'name', descending: false }),
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

  it('the automatic mode without a known size sends no strategy at all', async () => {
    await loadTable();
    expect(lastRequest().countStrategy).toBeUndefined();
    expect(lastRequest().countStrategy).not.toBe('auto');
    expect(lastRequest().countStrategy).not.toBe('exact');
    // The field is absent from the body the host receives.
    expect(wireKeys()).not.toContain('countStrategy');
  });

  it('the automatic mode below the threshold asks for an exact count — first read and every re-read', async () => {
    await setCatalogRowCount(100);
    await loadTable();
    expect(lastRequest().countStrategy).toBe('exact');
    for (const path of reReadPaths) {
      mockDatabaseCommands.getTableData.mockClear();
      act(() => {
        path.run();
      });
      await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
      expect(lastRequest().countStrategy, `re-read via ${path.name}`).toBe('exact');
    }
  });

  it('the automatic mode above the threshold asks for an estimate — first read and every re-read', async () => {
    await setCatalogRowCount(5_000_000);
    await loadTable();
    expect(lastRequest().countStrategy).toBe('estimated');
    for (const path of reReadPaths) {
      mockDatabaseCommands.getTableData.mockClear();
      act(() => {
        path.run();
      });
      await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
      expect(lastRequest().countStrategy, `re-read via ${path.name}`).toBe('estimated');
    }
  });

  it('the automatic mode is the same on the panel read and on a re-read', async () => {
    // The panel's first read goes through useTableActions; the re-reads go
    // through the store. Both must reach the same verdict.
    await setCatalogRowCount(100);
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
    expect(firstRead).toBe('exact');
    expect(lastRequest().countStrategy).toBe('exact');
  });

  it('the explicit modes are sent verbatim and survive every re-read', async () => {
    for (const [mode, expected] of [
      ['exact', 'exact'],
      ['estimated', 'estimated'],
      ['none', 'unsupported'],
    ] as const) {
      useSettingsStore.setState((state) => ({ settings: { ...state.settings, rowCountMode: mode } }));
      await loadTable();
      expect(lastRequest().countStrategy).toBe(expected);
      for (const path of reReadPaths) {
        mockDatabaseCommands.getTableData.mockClear();
        act(() => {
          path.run();
        });
        await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
        expect(lastRequest().countStrategy, `${mode} / ${path.name}`).toBe(expected);
      }
    }
  });

  it('an explicit strategy from the caller still wins over the automatic verdict', async () => {
    await setCatalogRowCount(100);
    await useTableDataStore.getState().loadTableData({
      panelId: PANEL,
      ...CONTEXT,
      countStrategy: 'estimated',
    });
    expect(lastRequest().countStrategy).toBe('estimated');
  });
});
