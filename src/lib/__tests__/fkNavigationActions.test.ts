/**
 * The jump itself (DB-07).
 *
 * `openForeignKeyTarget` is the only place a foreign-key jump is carried out,
 * so the tests here are the contract of the whole feature: a new tab with the
 * conditions of the jump, exactly one fetch, no tab at all when the host
 * refuses, and no third tab when the user walks back and forth.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const { mockResolveForeignKeyTargets } = vi.hoisted(() => ({
  mockResolveForeignKeyTargets: vi.fn(),
}));

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getTableData: vi.fn(),
    resolveForeignKeyTargets: mockResolveForeignKeyTargets,
    previewPendingChanges: vi.fn(),
    commitPendingChanges: vi.fn(),
  },
}));

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

import { openForeignKeyTarget } from '../fkNavigationActions';
import { usePanelStore } from '../../stores/panelStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useFkNavigationStore, resetFkNavigationEntryIds } from '../../stores/fkNavigationStore';
import type { TablePanel } from '../../stores/panelTypes';
import type { ForeignKeyInfo } from '../../types';

const FOREIGN_KEY: ForeignKeyInfo = {
  name: 'orders_customer_fk',
  columns: ['customer_id'],
  referencedTable: 'customers',
  referencedColumns: ['id'],
};

const COMPOSITE_FOREIGN_KEY: ForeignKeyInfo = {
  name: 'orders_parent_fk',
  columns: ['parent_a', 'parent_b'],
  referencedTable: 'parents',
  referencedColumns: ['x', 'y'],
};

function resolution(overrides: Record<string, unknown> = {}) {
  return {
    target: {
      database: 'app',
      schema: null,
      table: 'customers',
      columns: ['id'],
      dataTypes: ['integer'],
      primaryKeys: ['id'],
      unique: true,
      candidateLimit: 50,
    },
    exists: true,
    candidates: [[7]],
    truncated: false,
    ...overrides,
  };
}

function sourcePanel(overrides: Partial<TablePanel> = {}): TablePanel {
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
    ...overrides,
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
    foreignKey: FOREIGN_KEY,
    values: [7],
    ...overrides,
  };
}

describe('openForeignKeyTarget', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetFkNavigationEntryIds();
    usePanelStore.setState({ panels: [], activePanelId: null });
    useTableDataStore.getState().reset();
    useFkNavigationStore.getState().reset();
    // Every jump starts from a tab that exists.
    usePanelStore.getState().addPanel(sourcePanel());
  });

  it('opens a new tab with the conditions of the jump', async () => {
    mockResolveForeignKeyTargets.mockResolvedValue(resolution());

    const outcome = await openForeignKeyTarget(jump());

    expect(outcome.ok).toBe(true);
    expect(outcome.targetMissing).toBe(false);
    const panels = usePanelStore.getState().panels;
    expect(panels).toHaveLength(2);
    const target = panels[1] as TablePanel;
    expect(target.tableName).toBe('customers');
    expect(target.database).toBe('app');
    // The tab knows where it came from.
    expect(target.fkOrigin?.sourcePanelId).toBe('panel-source');
    expect(target.fkOrigin?.localColumns).toEqual(['customer_id']);
    expect(target.fkOrigin?.targetColumns).toEqual(['id']);
  });

  it('seeds the conditions into the first frame of the new tab', async () => {
    mockResolveForeignKeyTargets.mockResolvedValue(resolution());

    const outcome = await openForeignKeyTarget(jump());
    const panelId = outcome.panelId as string;

    const slice = useTableDataStore.getState().byPanel.get(panelId);
    expect(slice?.filters).toEqual([{ column: 'id', operator: 'eq', value: 7 }]);
    expect(slice?.draftFilters).toEqual([{ column: 'id', operator: 'eq', value: 7 }]);
    expect(slice?.filterLogic).toBe('and');
    // The seed never fetches: the tab's own first load is the one fetch.
    expect(slice?.requestRevision).toBe(0);
  });

  it('pairs a composite key positionally', async () => {
    mockResolveForeignKeyTargets.mockResolvedValue(
      resolution({
        target: {
          database: 'app',
          schema: null,
          table: 'parents',
          columns: ['x', 'y'],
          dataTypes: ['integer', 'integer'],
          primaryKeys: ['x', 'y'],
          unique: true,
          candidateLimit: 50,
        },
      }),
    );

    const outcome = await openForeignKeyTarget(
      jump({ foreignKey: COMPOSITE_FOREIGN_KEY, values: [3, 4] }),
    );

    const slice = useTableDataStore.getState().byPanel.get(outcome.panelId as string);
    expect(slice?.filters).toEqual([
      { column: 'x', operator: 'eq', value: 3 },
      { column: 'y', operator: 'eq', value: 4 },
    ]);
    // The request carries both columns and both values, in pairing order.
    expect(mockResolveForeignKeyTargets).toHaveBeenCalledTimes(1);
    const request = mockResolveForeignKeyTargets.mock.calls[0][0];
    expect(request.columns).toEqual(['parent_a', 'parent_b']);
    expect(request.values).toEqual([3, 4]);
    expect(request.constraintName).toBe('orders_parent_fk');
  });

  it('refuses a key with a missing member without asking the host', async () => {
    const outcome = await openForeignKeyTarget(
      jump({ foreignKey: COMPOSITE_FOREIGN_KEY, values: [3, null] }),
    );

    expect(outcome.ok).toBe(false);
    expect(outcome.error?.code).toBe('grid.fk.valueNull');
    expect(mockResolveForeignKeyTargets).not.toHaveBeenCalled();
    // No tab was opened: the source tab is the only one.
    expect(usePanelStore.getState().panels).toHaveLength(1);
  });

  it('refuses a request whose shape does not line up', async () => {
    const outcome = await openForeignKeyTarget(jump({ values: [7, 8] }));

    expect(outcome.ok).toBe(false);
    expect(outcome.error?.code).toBe('grid.fk.shapeMismatch');
    expect(mockResolveForeignKeyTargets).not.toHaveBeenCalled();
  });

  it('opens no tab when the host refuses', async () => {
    mockResolveForeignKeyTargets.mockRejectedValue(
      'grid.fk.targetUnresolved: the referenced table could not be resolved',
    );

    const outcome = await openForeignKeyTarget(jump());

    expect(outcome.ok).toBe(false);
    expect(outcome.error?.code).toBe('grid.fk.targetUnresolved');
    expect(outcome.error?.raw).toContain('grid.fk.targetUnresolved');
    expect(usePanelStore.getState().panels).toHaveLength(1);
  });

  it('keeps an unclassified error readable', async () => {
    mockResolveForeignKeyTargets.mockRejectedValue('permission denied for table customers');

    const outcome = await openForeignKeyTarget(jump());

    expect(outcome.error?.code).toBe('unknown');
    expect(outcome.error?.raw).toBe('permission denied for table customers');
  });

  it('reports a missing target row and still opens the tab', async () => {
    mockResolveForeignKeyTargets.mockResolvedValue(
      resolution({ exists: false, candidates: [] }),
    );

    const outcome = await openForeignKeyTarget(jump());

    expect(outcome.ok).toBe(true);
    expect(outcome.targetMissing).toBe(true);
    expect(usePanelStore.getState().panels).toHaveLength(2);
    const slice = useTableDataStore.getState().byPanel.get(outcome.panelId as string);
    expect(slice?.filters).toHaveLength(1);
  });

  it('records the jump in the navigation stack', async () => {
    mockResolveForeignKeyTargets.mockResolvedValue(resolution());
    usePanelStore.getState().addPanel(sourcePanel());

    const outcome = await openForeignKeyTarget(jump());

    const stack = useFkNavigationStore.getState().back;
    expect(stack).toHaveLength(1);
    expect(stack[0].panelId).toBe(outcome.panelId);
    expect(stack[0].origin.sourcePanelId).toBe('panel-source');
  });

  it('reuses the tab of a cycle instead of opening a third one', async () => {
    mockResolveForeignKeyTargets.mockResolvedValue(resolution());
    // A → B, and B is the tab the jump opened.
    const first = await openForeignKeyTarget(jump());
    const targetPanel = usePanelStore.getState().panels.find(
      (panel) => panel.id === first.panelId,
    ) as TablePanel;

    // B → A: the target is the table the navigation came from.
    mockResolveForeignKeyTargets.mockResolvedValue(
      resolution({
        target: {
          database: 'app',
          schema: null,
          table: 'orders',
          columns: ['id'],
          dataTypes: ['integer'],
          primaryKeys: ['id'],
          unique: true,
          candidateLimit: 50,
        },
      }),
    );
    const second = await openForeignKeyTarget(
      jump({
        sourcePanelId: targetPanel.id,
        sourceTable: 'customers',
        foreignKey: { ...FOREIGN_KEY, name: 'customers_order_fk' },
        values: [3],
      }),
    );

    expect(second.ok).toBe(true);
    expect(second.reused).toBe(true);
    expect(second.panelId).toBe('panel-source');
    // No third tab: the round trip reuses the tab that is already open.
    expect(usePanelStore.getState().panels).toHaveLength(2);
    expect(useFkNavigationStore.getState().back).toHaveLength(1);
  });
});
