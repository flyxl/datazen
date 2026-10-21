/**
 * DB-07 continuous journey: the foreign-key navigation, from the arrow in the
 * grid to the tab it opens and back again.
 *
 * The journey runs against the **real** wiring — the real
 * `useTableForeignKeyNavigation`, the real panel store, the real table-data
 * store, the real navigation stack — and the real desktop transport, so the
 * command that reaches the host is asserted by its wire name.
 *
 * The steps that matter are the ones a user lives through:
 *
 * - the arrow appears on the cell of a foreign key, and **only** there;
 * - a group with a missing member has no arrow at all (a composite key is
 *   located by all of its columns or by none of them);
 * - clicking the arrow opens a **new** tab that already carries the
 *   conditions of the jump;
 * - "back" returns to the tab the jump started from;
 * - walking A → B → A does not pile up tabs;
 * - a key press during an IME composition or inside an editor changes nothing.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { useMemo } from 'react';
import { DataTable } from '../DataTable';
import { useTableForeignKeyNavigation } from '../../../windows/connection/useTableForeignKeyNavigation';
import { usePanelStore } from '../../../stores/panelStore';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { useFkNavigationStore, resetFkNavigationEntryIds } from '../../../stores/fkNavigationStore';
import { createBackendClient, setBackendClient } from '@datazen/backend-client';
import {
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from '../../../platform/tauriBackendTransport';
import type { ForeignKeyInfo } from '../../../types';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
  },
}));

const schemaState = vi.hoisted(() => ({ foreignKeys: [] as unknown[] }));

vi.mock('../../../lib/schemaCache', () => ({
  getCachedTableSchema: async () => ({
    tableName: 'orders',
    columns: [],
    primaryKeys: [],
    indexes: [],
    foreignKeys: schemaState.foreignKeys,
  }),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({ columnWidths: [160, 160, 160], onResizeStart: vi.fn() }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 28 })),
    totalHeight: rows.length * 28,
  }),
}));

vi.mock('../../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

const SOURCE_PANEL = 'panel-orders';

const SINGLE_FOREIGN_KEY: ForeignKeyInfo = {
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

const RESOLUTION = {
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
};

const COLUMNS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'customer_id', name: 'customer_id', type: 'integer' },
];

const COMPOSITE_COLUMNS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'customer_id', name: 'customer_id', type: 'integer' },
  { id: 'parent_a', name: 'parent_a', type: 'integer' },
  { id: 'parent_b', name: 'parent_b', type: 'integer' },
];

interface GridProps {
  rows: Record<string, unknown>[];
  /** The displayed column names, in render order. */
  visibleColumns?: string[];

  /** The columns of the grid. */
  columns?: { id: string; name: string; type: string }[];
}

/** The grid, wired the way `TableView` wires it. */
function FkGrid({
  rows,
  visibleColumns = ['id', 'customer_id'],
  columns = COLUMNS,
}: GridProps) {
  const navigation = useTableForeignKeyNavigation({
    panelId: SOURCE_PANEL,
    dbSessionId: 'session-1',
    database: 'app',
    tableName: 'orders',
    schema: null,
    connectionId: 'conn-1',
    databaseType: 'postgresql',
    rows,
    displayedColumns: visibleColumns.map((name) => ({ name })),
    rowHeight: 28,
    activeCell: null,
  });
  const rowArrays = useMemo(
    () => rows.map((row) => columns.map((column) => row[column.name] ?? null)),
    [rows, columns],
  );
  return (
    <div
      data-testid="fk-root"
      tabIndex={0}
      onKeyDown={navigation.onKeyDown}
    >
      {navigation.breadcrumb && (
        <div data-testid="fk-breadcrumb">{navigation.breadcrumb.origin.sourceTable}</div>
      )}
      {navigation.error && (
        <div data-testid="fk-error">{navigation.error.code}</div>
      )}
      <button type="button" data-testid="fk-back" onClick={navigation.onBack}>
        back
      </button>
      <button type="button" data-testid="fk-forward" onClick={navigation.onForward}>
        forward
      </button>
      <DataTable
        columns={columns}
        rows={rowArrays}
        fkIndex={navigation.fkIndex}
        fkVisibleColumns={navigation.fkVisibleColumns}
        fkResolving={navigation.fkResolving}
        onFkJump={navigation.onFkJump}
        onScrollElementChange={navigation.onScrollElementChange}
      />
    </div>
  );
}

function arrows(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll('[data-dt-fk-click="true"]'));
}

function arrowOf(container: HTMLElement, rowIndex: number, columnName: string): HTMLElement | null {
  const cell = container.querySelector(`[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`);
  if (!(cell instanceof HTMLElement)) return null;
  return cell.querySelector('[data-dt-fk-click="true"]');
}

function sourcePanel() {
  return {
    id: SOURCE_PANEL,
    type: 'table' as const,
    tableName: 'orders',
    database: 'app',
    tableSchema: null,
    subTab: 'data' as const,
    connectionId: 'conn-1',
    dbSessionId: 'session-1',
    connectionName: 'Local',
    databaseType: 'postgresql' as const,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  resetFkNavigationEntryIds();
  schemaState.foreignKeys = [SINGLE_FOREIGN_KEY];
  usePanelStore.setState({ panels: [], activePanelId: null });
  useTableDataStore.getState().reset();
  useFkNavigationStore.getState().reset();
  usePanelStore.getState().addPanel(sourcePanel());
  setBackendClient(
    DESKTOP_BACKEND_ID,
    createBackendClient(DESKTOP_BACKEND_ID, createDesktopBackendTransport()),
  );
  mockInvoke.mockResolvedValue(RESOLUTION);
});

afterEach(cleanup);

describe('journey: foreign-key navigation', () => {
  it('shows the arrow on the cell of the foreign key and nowhere else', async () => {
    const { container } = render(
      <FkGrid rows={[{ id: 1, customer_id: 7 }, { id: 2, customer_id: null }]} />,
    );

    await waitFor(() => expect(arrows(container)).toHaveLength(1));
    // The cell with a value carries the arrow; the NULL cell does not.
    expect(arrowOf(container, 0, 'customer_id')).not.toBeNull();
    expect(arrowOf(container, 1, 'customer_id')).toBeNull();
    // A column that is not a member of a key never carries one.
    expect(arrowOf(container, 0, 'id')).toBeNull();
  });

  it('opens a new tab with the conditions of the jump', async () => {
    const { container } = render(<FkGrid rows={[{ id: 1, customer_id: 7 }]} />);
    await waitFor(() => expect(arrows(container)).toHaveLength(1));

    fireEvent.click(arrows(container)[0]);

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledTimes(1));
    // The command that reaches the host, by its wire name.
    expect(mockInvoke).toHaveBeenCalledWith('resolve_foreign_key_targets', {
      request: expect.objectContaining({
        dbSessionId: 'session-1',
        table: 'orders',
        columns: ['customer_id'],
        constraintName: 'orders_customer_fk',
        values: [7],
      }),
    });

    await waitFor(() => expect(usePanelStore.getState().panels).toHaveLength(2));
    const target = usePanelStore.getState().panels[1];
    expect(target.type).toBe('table');
    if (target.type !== 'table') throw new Error('the target tab is not a table tab');
    expect(target.tableName).toBe('customers');
    expect(target.fkOrigin?.sourcePanelId).toBe(SOURCE_PANEL);
    expect(target.fkOrigin?.localColumns).toEqual(['customer_id']);

    // The conditions are in the first frame of the new tab.
    const slice = useTableDataStore.getState().byPanel.get(target.id);
    expect(slice?.filters).toEqual([{ column: 'id', operator: 'eq', value: 7 }]);
    expect(slice?.filterLogic).toBe('and');
    expect(slice?.filterPanelOpen).toBe(true);
  });

  it('goes back to the tab the jump started from', async () => {
    const { container } = render(<FkGrid rows={[{ id: 1, customer_id: 7 }]} />);
    await waitFor(() => expect(arrows(container)).toHaveLength(1));

    fireEvent.click(arrows(container)[0]);
    await waitFor(() => expect(usePanelStore.getState().panels).toHaveLength(2));
    const targetId = usePanelStore.getState().panels[1].id;

    fireEvent.click(container.querySelector('[data-testid="fk-back"]') as HTMLElement);

    expect(usePanelStore.getState().activePanelId).toBe(SOURCE_PANEL);
    expect(useFkNavigationStore.getState().back).toHaveLength(0);
    expect(useFkNavigationStore.getState().forward).toHaveLength(1);
    // The tab the jump opened is still open — going back does not close it.
    expect(usePanelStore.getState().panels.map((panel) => panel.id)).toContain(targetId);
  });

  it('goes forward again to the tab the jump opened', async () => {
    const { container } = render(<FkGrid rows={[{ id: 1, customer_id: 7 }]} />);
    await waitFor(() => expect(arrows(container)).toHaveLength(1));
    fireEvent.click(arrows(container)[0]);
    await waitFor(() => expect(usePanelStore.getState().panels).toHaveLength(2));
    const targetId = usePanelStore.getState().panels[1].id;

    fireEvent.click(container.querySelector('[data-testid="fk-back"]') as HTMLElement);
    fireEvent.click(container.querySelector('[data-testid="fk-forward"]') as HTMLElement);

    expect(usePanelStore.getState().activePanelId).toBe(targetId);
    expect(useFkNavigationStore.getState().back).toHaveLength(1);
    expect(useFkNavigationStore.getState().forward).toHaveLength(0);
  });

  it('keeps the stack untouched while an IME composition is active', async () => {
    const { container } = render(<FkGrid rows={[{ id: 1, customer_id: 7 }]} />);
    await waitFor(() => expect(arrows(container)).toHaveLength(1));
    fireEvent.click(arrows(container)[0]);
    await waitFor(() => expect(usePanelStore.getState().panels).toHaveLength(2));

    const root = container.querySelector('[data-testid="fk-root"]') as HTMLElement;
    const event = new KeyboardEvent('keydown', { key: 'ArrowLeft', metaKey: true, altKey: true });
    Object.defineProperty(event, 'isComposing', { value: true });
    fireEvent(root, event);

    expect(useFkNavigationStore.getState().back).toHaveLength(1);
    expect(usePanelStore.getState().activePanelId).not.toBe(SOURCE_PANEL);
  });

  it('keeps the stack untouched while a text field has the focus', async () => {
    const { container } = render(<FkGrid rows={[{ id: 1, customer_id: 7 }]} />);
    await waitFor(() => expect(arrows(container)).toHaveLength(1));
    fireEvent.click(arrows(container)[0]);
    await waitFor(() => expect(usePanelStore.getState().panels).toHaveLength(2));

    const root = container.querySelector('[data-testid="fk-root"]') as HTMLElement;
    const input = document.createElement('input');
    root.appendChild(input);
    fireEvent.keyDown(input, { key: 'ArrowLeft', metaKey: true, altKey: true });

    expect(useFkNavigationStore.getState().back).toHaveLength(1);
  });

  it('refuses a jump whose group has a missing member, without asking the host', async () => {
    const { container } = render(
      <FkGrid rows={[{ id: 1, customer_id: 7 }, { id: 2, customer_id: null }]} />,
    );
    await waitFor(() => expect(arrows(container)).toHaveLength(1));

    // The NULL row has no arrow at all, so there is nothing to click.
    expect(arrowOf(container, 1, 'customer_id')).toBeNull();
    fireEvent.click(arrows(container)[0]);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledTimes(1));

    // The request carries the value of the row it was clicked in — never a
    // "IS NULL" condition.
    const request = mockInvoke.mock.calls[0][1].request;
    expect(request.values).toEqual([7]);
  });

  it('shows a spinner and refuses a second click while the jump is in flight', async () => {
    mockInvoke.mockReturnValue(new Promise(() => {}));
    const { container } = render(<FkGrid rows={[{ id: 1, customer_id: 7 }]} />);
    await waitFor(() => expect(arrows(container)).toHaveLength(1));

    fireEvent.click(arrows(container)[0]);

    await waitFor(() => {
      const arrow = arrows(container)[0];
      expect((arrow as HTMLButtonElement).disabled).toBe(true);
    });
    fireEvent.click(arrows(container)[0]);
    expect(mockInvoke).toHaveBeenCalledTimes(1);
  });

  it('shows a classified error and opens no tab when the host refuses', async () => {
    mockInvoke.mockRejectedValue('grid.fk.targetUnresolved: the referenced table is gone');
    const { container } = render(<FkGrid rows={[{ id: 1, customer_id: 7 }]} />);
    await waitFor(() => expect(arrows(container)).toHaveLength(1));

    fireEvent.click(arrows(container)[0]);

    await waitFor(() => expect(container.querySelector('[data-testid="fk-error"]')).not.toBeNull());
    expect(container.querySelector('[data-testid="fk-error"]')?.textContent).toBe(
      'grid.fk.targetUnresolved',
    );
    expect(usePanelStore.getState().panels).toHaveLength(1);
  });

  it('uses both columns of a composite key and lands the arrow on the first one', async () => {
    schemaState.foreignKeys = [COMPOSITE_FOREIGN_KEY];
    mockInvoke.mockResolvedValue({
      ...RESOLUTION,
      target: {
        ...RESOLUTION.target,
        table: 'parents',
        columns: ['x', 'y'],
        dataTypes: ['integer', 'integer'],
        primaryKeys: ['x', 'y'],
      },
    });
    const { container } = render(
      <FkGrid
        rows={[{ id: 1, customer_id: 3, parent_a: 1, parent_b: 2 }]}
        visibleColumns={['id', 'customer_id', 'parent_a', 'parent_b']}
        columns={COMPOSITE_COLUMNS}
      />,
    );

    await waitFor(() => expect(arrows(container)).toHaveLength(1));
    // The arrow is on the first member of the group, not on both of them.
    expect(arrowOf(container, 0, 'parent_a')).not.toBeNull();
    expect(arrowOf(container, 0, 'parent_b')).toBeNull();

    fireEvent.click(arrows(container)[0]);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledTimes(1));
    const request = mockInvoke.mock.calls[0][1].request;
    expect(request.columns).toEqual(['parent_a', 'parent_b']);
    expect(request.values).toEqual([1, 2]);

    await waitFor(() => expect(usePanelStore.getState().panels).toHaveLength(2));
    const target = usePanelStore.getState().panels[1];
    const slice = useTableDataStore.getState().byPanel.get(target.id);
    // Both conditions, combined with AND.
    expect(slice?.filters).toEqual([
      { column: 'x', operator: 'eq', value: 1 },
      { column: 'y', operator: 'eq', value: 2 },
    ]);
    expect(slice?.filterLogic).toBe('and');
  });

  it('moves the arrow to the next visible member when the first one is hidden', async () => {
    schemaState.foreignKeys = [COMPOSITE_FOREIGN_KEY];
    const { container } = render(
      <FkGrid
        rows={[{ id: 1, customer_id: 3, parent_a: 1, parent_b: 2 }]}
        visibleColumns={['id', 'customer_id', 'parent_b']}
        columns={COMPOSITE_COLUMNS}
      />,
    );

    await waitFor(() => expect(arrows(container)).toHaveLength(1));
    // "parent_a" is hidden: the arrow moves to the member the user can see.
    expect(arrowOf(container, 0, 'parent_b')).not.toBeNull();
    expect(arrowOf(container, 0, 'parent_a')).toBeNull();
  });

  it('has no arrow at all when the whole group is hidden', async () => {
    schemaState.foreignKeys = [COMPOSITE_FOREIGN_KEY];
    const { container } = render(
      <FkGrid
        rows={[{ id: 1, customer_id: 3, parent_a: 1, parent_b: 2 }]}
        visibleColumns={['id', 'customer_id']}
        columns={COMPOSITE_COLUMNS}
      />,
    );

    await waitFor(() => expect(container.querySelector('[data-dt-fk]')).not.toBeNull());
    expect(arrows(container)).toHaveLength(0);
  });

  it('does not pile up tabs when the user walks A → B → A', async () => {
    const { container } = render(<FkGrid rows={[{ id: 1, customer_id: 7 }]} />);
    await waitFor(() => expect(arrows(container)).toHaveLength(1));

    fireEvent.click(arrows(container)[0]);
    await waitFor(() => expect(usePanelStore.getState().panels).toHaveLength(2));
    const targetId = usePanelStore.getState().panels[1].id;

    // B → A: the target is the table the navigation came from.
    mockInvoke.mockResolvedValue({
      ...RESOLUTION,
      target: { ...RESOLUTION.target, table: 'orders' },
    });
    const second = await import('../../../lib/fkNavigationActions');
    await second.openForeignKeyTarget({
      sourcePanelId: targetId,
      sourceTable: 'customers',
      dbSessionId: 'session-1',
      database: 'app',
      schema: null,
      connectionId: 'conn-1',
      driverType: 'postgresql',
      foreignKey: { ...SINGLE_FOREIGN_KEY, name: 'customers_order_fk' },
      values: [3],
    });

    expect(usePanelStore.getState().panels).toHaveLength(2);
    expect(useFkNavigationStore.getState().back).toHaveLength(1);
  });
});
