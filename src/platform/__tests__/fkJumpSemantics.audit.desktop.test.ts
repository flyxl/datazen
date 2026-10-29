/**
 * Independent verification of the jump semantics (Wave 5 · track C).
 *
 * The five properties of 07 that a user lives through:
 *
 * 1. the request names **every** local column of the group, in pairing order,
 *    with the row's current values, and the constraint;
 * 2. the conditions of the new tab are positional `eq` pairs combined with
 *    `and` — and a group that does not line up is **refused**, never degraded
 *    into `IS NULL`;
 * 3. the first frame of the new tab already carries the conditions: the
 *    applied filters, the draft filters and an open filter panel, without
 *    bumping the request revision and without a fetch of its own;
 * 4. "back" activates the tab the jump came **from** and restores its
 *    position; "forward" activates the tab the jump opened;
 * 5. walking A → B → A reuses the tab that is already open: two tabs, one
 *    stack entry.
 *
 * The chain is the real one — the real desktop transport, the real backend
 * client, the real `openForeignKeyTarget` — so the wire names and the request
 * bodies are the ones the host receives.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
  },
}));

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

import { createBackendClient, selectBackend, setBackendClient } from '@datazen/backend-client';
import {
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from '../desktop/tauriBackendTransport';
import { openForeignKeyTarget } from '../../lib/fkNavigationActions';
import { buildFkFilters, scrollTopForAnchorRow } from '../../lib/fkNavigation';
import { usePanelStore } from '../../stores/panelStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useFkNavigationStore, resetFkNavigationEntryIds } from '../../stores/fkNavigationStore';
import type { TablePanel } from '../../stores/panelTypes';
import type { ForeignKeyInfo } from '../../types';

const SINGLE: ForeignKeyInfo = {
  name: 'orders_customer_fk',
  columns: ['customer_id'],
  referencedTable: 'customers',
  referencedColumns: ['id'],
};

const COMPOSITE: ForeignKeyInfo = {
  name: 'orders_parent_fk',
  columns: ['parent_a', 'parent_b'],
  referencedTable: 'parents',
  referencedColumns: ['x', 'y'],
};

function resolution(table: string, columns: string[]) {
  return {
    target: {
      database: 'app',
      schema: null,
      table,
      columns,
      dataTypes: columns.map(() => 'integer'),
      primaryKeys: columns,
      unique: true,
      candidateLimit: 50,
    },
    exists: true,
    candidates: [],
    truncated: false,
  };
}

function sourcePanel(): TablePanel {
  return {
    id: 'panel-source',
    type: 'table',
    tableName: 'orders',
    database: 'app',
    tableSchema: null,
    subTab: 'data',
    connectionId: 'conn-1',
    dbSessionId: 'session-1',
    connectionName: 'Local',
    databaseType: 'postgresql',
  };
}

function jump(overrides: Partial<Parameters<typeof openForeignKeyTarget>[0]> = {}) {
  return {
    sourcePanelId: 'panel-source',
    sourceTable: 'orders',
    dbSessionId: 'session-1',
    database: 'app',
    schema: null,
    connectionId: 'conn-1',
    driverType: 'postgresql',
    foreignKey: SINGLE,
    values: [7],
    ...overrides,
  };
}

/** The invocations the desktop transport made, by wire command name. */
function callsTo(command: string): { request: Record<string, unknown> }[] {
  return mockInvoke.mock.calls
    .filter((call) => call[0] === command)
    .map((call) => {
      const args = call[1] as { request?: Record<string, unknown> } | undefined;
      return { request: args?.request ?? {} };
    });
}

describe('audit: the request of a jump', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetFkNavigationEntryIds();
    usePanelStore.setState({ panels: [], activePanelId: null });
    useTableDataStore.getState().reset();
    useFkNavigationStore.getState().reset();
    usePanelStore.getState().addPanel(sourcePanel());
    const client = createBackendClient(DESKTOP_BACKEND_ID, createDesktopBackendTransport());
    setBackendClient(DESKTOP_BACKEND_ID, client);
    selectBackend(DESKTOP_BACKEND_ID);
  });

  it('names every local column of the group, the values and the constraint', async () => {
    mockInvoke.mockResolvedValue(resolution('parents', ['x', 'y']));

    await openForeignKeyTarget(jump({ foreignKey: COMPOSITE, values: [3, 4] }));

    expect(callsTo('resolve_foreign_key_targets')).toHaveLength(1);
    const request = callsTo('resolve_foreign_key_targets')[0].request;
    expect(request.columns).toEqual(['parent_a', 'parent_b']);
    expect(request.values).toEqual([3, 4]);
    expect(request.constraintName).toBe('orders_parent_fk');
    expect(request.table).toBe('orders');
  });

  it('builds positional eq conditions combined with and', async () => {
    mockInvoke.mockResolvedValue(resolution('parents', ['x', 'y']));

    const outcome = await openForeignKeyTarget(jump({ foreignKey: COMPOSITE, values: [3, 4] }));

    const slice = useTableDataStore.getState().byPanel.get(outcome.panelId as string);
    expect(slice?.filters).toEqual([
      { column: 'x', operator: 'eq', value: 3 },
      { column: 'y', operator: 'eq', value: 4 },
    ]);
    expect(slice?.filterLogic).toBe('and');
  });

  it('refuses a group with a missing member, and never asks for IS NULL', async () => {
    const outcome = await openForeignKeyTarget(jump({ foreignKey: COMPOSITE, values: [3, null] }));

    expect(outcome.ok).toBe(false);
    expect(outcome.error?.code).toBe('grid.fk.valueNull');
    // No request at all — the host is never asked for a half-written key.
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(usePanelStore.getState().panels).toHaveLength(1);
  });

  it('refuses a request whose shape does not line up', async () => {
    const outcome = await openForeignKeyTarget(jump({ values: [7, 8] }));

    expect(outcome.ok).toBe(false);
    expect(outcome.error?.code).toBe('grid.fk.shapeMismatch');
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(usePanelStore.getState().panels).toHaveLength(1);
  });

  it('buildFkFilters never degrades a missing value into IS NULL', () => {
    expect(buildFkFilters({ columns: ['x', 'y'] }, [3, null])).toBeNull();
    expect(buildFkFilters({ columns: ['x', 'y'] }, [3])).toBeNull();
    expect(buildFkFilters({ columns: ['x', 'y'] }, [3, 4])).toEqual([
      { column: 'x', operator: 'eq', value: 3 },
      { column: 'y', operator: 'eq', value: 4 },
    ]);
  });
});

describe('audit: the first frame of the new tab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetFkNavigationEntryIds();
    usePanelStore.setState({ panels: [], activePanelId: null });
    useTableDataStore.getState().reset();
    useFkNavigationStore.getState().reset();
    usePanelStore.getState().addPanel(sourcePanel());
    const client = createBackendClient(DESKTOP_BACKEND_ID, createDesktopBackendTransport());
    setBackendClient(DESKTOP_BACKEND_ID, client);
    selectBackend(DESKTOP_BACKEND_ID);
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === 'resolve_foreign_key_targets') return resolution('customers', ['id']);
      if (command === 'get_table_data') {
        return {
          columns: [{ name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false }],
          rows: [[7]],
          totalRows: 1,
          page: 0,
          pageSize: 50,
        };
      }
      throw new Error(`unexpected command ${command}`);
    });
  });

  it('carries the conditions in the applied filters, the draft and the open panel', async () => {
    const outcome = await openForeignKeyTarget(jump());
    const panelId = outcome.panelId as string;

    const slice = useTableDataStore.getState().byPanel.get(panelId);
    expect(slice?.filters).toEqual([{ column: 'id', operator: 'eq', value: 7 }]);
    expect(slice?.draftFilters).toEqual([{ column: 'id', operator: 'eq', value: 7 }]);
    expect(slice?.filterPanelOpen).toBe(true);
    expect(slice?.filterLogic).toBe('and');
    expect(slice?.draftFilterLogic).toBe('and');
  });

  it('does not bump the request revision and does not fetch on its own', async () => {
    const outcome = await openForeignKeyTarget(jump());
    const panelId = outcome.panelId as string;

    const slice = useTableDataStore.getState().byPanel.get(panelId);
    expect(slice?.requestRevision).toBe(0);
    expect(slice?.loadingRevision).toBeNull();
    // The seed never fetches.
    expect(callsTo('get_table_data')).toHaveLength(0);
  });

  it('fetches exactly once, and the one fetch carries the conditions', async () => {
    const outcome = await openForeignKeyTarget(jump());
    const panelId = outcome.panelId as string;

    // The tab's own first load — what `TableView` does when the panel mounts.
    const slice = useTableDataStore.getState().byPanel.get(panelId);
    await useTableDataStore.getState().loadTableData({
      panelId,
      connectionId: 'conn-1',
      dbSessionId: 'session-1',
      driverType: 'postgresql',
      database: 'app',
      schema: null,
      table: 'customers',
    });

    const fetches = callsTo('get_table_data');
    expect(fetches).toHaveLength(1);
    const request = fetches[0].request;
    expect(request.filters).toEqual([{ column: 'id', operator: 'eq', value: 7 }]);
    expect(request.filterLogic).toBe('and');
    // The seed is still what the panel carries.
    expect(slice?.filters).toHaveLength(1);
  });
});

describe('audit: the round trip A → B → A', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetFkNavigationEntryIds();
    usePanelStore.setState({ panels: [], activePanelId: null });
    useTableDataStore.getState().reset();
    useFkNavigationStore.getState().reset();
    usePanelStore.getState().addPanel(sourcePanel());
    const client = createBackendClient(DESKTOP_BACKEND_ID, createDesktopBackendTransport());
    setBackendClient(DESKTOP_BACKEND_ID, client);
    selectBackend(DESKTOP_BACKEND_ID);
  });

  it('keeps two tabs and one stack entry, and reuses the tab of the origin', async () => {
    mockInvoke.mockResolvedValue(resolution('customers', ['id']));
    const first = await openForeignKeyTarget(jump());
    expect(usePanelStore.getState().panels).toHaveLength(2);
    const targetId = first.panelId as string;

    // B → A: the target is the table the navigation came from.
    mockInvoke.mockResolvedValue(resolution('orders', ['id']));
    const second = await openForeignKeyTarget(
      jump({
        sourcePanelId: targetId,
        sourceTable: 'customers',
        foreignKey: { ...SINGLE, name: 'customers_order_fk' },
        values: [3],
      }),
    );

    expect(second.ok).toBe(true);
    expect(second.reused).toBe(true);
    expect(second.panelId).toBe('panel-source');
    // Two tabs, one stack entry — the round trip does not pile up tabs.
    expect(usePanelStore.getState().panels).toHaveLength(2);
    expect(useFkNavigationStore.getState().back).toHaveLength(1);
    expect(useFkNavigationStore.getState().forward).toHaveLength(0);
  });

  it('keeps the tabs and the stack bounded over ten round trips', async () => {
    mockInvoke.mockResolvedValue(resolution('customers', ['id']));
    const first = await openForeignKeyTarget(jump());
    const targetId = first.panelId as string;

    mockInvoke.mockResolvedValue(resolution('orders', ['id']));
    for (let index = 0; index < 10; index += 1) {
      await openForeignKeyTarget(
        jump({
          sourcePanelId: targetId,
          sourceTable: 'customers',
          foreignKey: { ...SINGLE, name: 'customers_order_fk' },
          values: [3],
        }),
      );
    }

    expect(usePanelStore.getState().panels).toHaveLength(2);
    expect(useFkNavigationStore.getState().back).toHaveLength(1);
  });
});

describe('audit: the viewport of a tab', () => {
  beforeEach(() => {
    resetFkNavigationEntryIds();
    useFkNavigationStore.getState().reset();
  });

  it('is stored per panel and taken once', () => {
    const store = useFkNavigationStore.getState();
    store.recordViewport('panel-a', { scrollTop: 140, anchorRow: 5, anchorColumn: 'id' });

    const taken = useFkNavigationStore.getState().takeViewport('panel-a');
    expect(taken).toEqual({ scrollTop: 140, anchorRow: 5, anchorColumn: 'id' });
    // Taking it consumes it: a tab entered twice does not restore twice.
    expect(useFkNavigationStore.getState().takeViewport('panel-a')).toBeNull();
  });

  it('keeps only the latest snapshot of a panel', () => {
    const store = useFkNavigationStore.getState();
    store.recordViewport('panel-a', { scrollTop: 10, anchorRow: 0, anchorColumn: null });
    store.recordViewport('panel-a', { scrollTop: 90, anchorRow: 3, anchorColumn: 'id' });

    expect(useFkNavigationStore.getState().takeViewport('panel-a')?.scrollTop).toBe(90);
  });

  it('turns an anchor row into a scroll offset', () => {
    expect(scrollTopForAnchorRow(5, 28)).toBe(140);
    expect(scrollTopForAnchorRow(0, 28)).toBe(0);
    expect(scrollTopForAnchorRow(-3, 28)).toBe(0);
  });
});

describe('audit: a refusal opens no tab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetFkNavigationEntryIds();
    usePanelStore.setState({ panels: [], activePanelId: null });
    useTableDataStore.getState().reset();
    useFkNavigationStore.getState().reset();
    usePanelStore.getState().addPanel(sourcePanel());
    const client = createBackendClient(DESKTOP_BACKEND_ID, createDesktopBackendTransport());
    setBackendClient(DESKTOP_BACKEND_ID, client);
    selectBackend(DESKTOP_BACKEND_ID);
  });

  /**
   * FKU-D2 (known defect, expected failure).
   *
   * `FkJumpOutcome.ok === false` promises "nothing happened: no tab was
   * opened" (`src/lib/fkNavigationActions.ts`), and 07 §3.8 step 3 says the
   * same. `openForeignKeyTarget` adds the tab **before** it checks that the
   * conditions can be written, so a target that does not line up leaves an
   * empty tab behind.
   *
   * The current Rust host cannot produce that answer (it rejects a mismatch
   * with `grid.fk.shapeMismatch` first), so the defect was latent — but the
   * promise of the outcome type was broken anyway.
   *
   * Fixed: the conditions are built **before** the tab is created, so a
   * refusal leaves no tab behind. The pin used to be `it.fails`; it is a plain
   * `it` now.
   */
  it('opens no tab when the resolved target does not line up with the values', async () => {
    // The host answers with a target that has **two** referenced columns while
    // the front end asked about a one-column key. The conditions cannot be
    // written, so the jump is refused.
    mockInvoke.mockResolvedValue(resolution('customers', ['id', 'tenant_id']));

    const outcome = await openForeignKeyTarget(jump());

    expect(outcome.ok).toBe(false);
    expect(outcome.error?.code).toBe('grid.fk.shapeMismatch');
    // A refusal opens **no** tab: the source tab is the only one.
    expect(usePanelStore.getState().panels).toHaveLength(1);
  });
});
