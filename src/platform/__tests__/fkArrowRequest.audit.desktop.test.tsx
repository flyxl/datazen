/**
 * Independent verification of the arrow → request path (Wave 5 · track D).
 *
 * The coordinates of a click have to survive three hops: the arrow reports
 * `(rowIndex, columnName)`, the grid hands them to the navigation, and the
 * navigation turns them into the **values of that row** and the **columns of
 * the group**. A hop that swaps them produces a request that looks plausible
 * and addresses the wrong row — the worst kind of bug, because nothing fails.
 *
 * This file drives the whole path: the real arrow, the real grid, the real
 * navigation hook and the real desktop transport, and then asserts the request
 * the host receives.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { useMemo } from 'react';
import { DataTable } from '../../components/DataTable/DataTable';
import { useTableForeignKeyNavigation } from '../../windows/connection/useTableForeignKeyNavigation';
import { usePanelStore } from '../../stores/panelStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useFkNavigationStore, resetFkNavigationEntryIds } from '../../stores/fkNavigationStore';
import { createBackendClient, setBackendClient } from '@datazen/backend-client';
import {
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from '../desktop/tauriBackendTransport';
import type { TablePanel } from '../../stores/panelTypes';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
  },
}));

const schemaState = vi.hoisted(() => ({ foreignKeys: [] as unknown[] }));

vi.mock('../../lib/schemaCache', () => ({
  // `fkReferencedBy` 在模块作用域调用它注册失效回调；返回退订空函数即可。
  subscribeSchemaInvalidation: () => () => undefined,
  getCachedTableSchema: async () => ({
    tableName: 'orders',
    columns: [],
    primaryKeys: [],
    indexes: [],
    foreignKeys: schemaState.foreignKeys,
  }),
}));

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({ columnWidths: [160, 160, 160, 160], onResizeStart: vi.fn() }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 28 })),
    totalHeight: rows.length * 28,
  }),
}));

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

const SOURCE_PANEL = 'panel-source';

const SINGLE = {
  name: 'orders_customer_fk',
  columns: ['customer_id'],
  referencedTable: 'customers',
  referencedColumns: ['id'],
};

const COMPOSITE = {
  name: 'orders_parent_fk',
  columns: ['parent_a', 'parent_b'],
  referencedTable: 'parents',
  referencedColumns: ['x', 'y'],
};

const COLUMNS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'customer_id', name: 'customer_id', type: 'integer' },
  { id: 'parent_a', name: 'parent_a', type: 'integer' },
  { id: 'parent_b', name: 'parent_b', type: 'integer' },
];

function sourcePanel(): TablePanel {
  return {
    id: SOURCE_PANEL,
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

/** The grid, wired the way `TableView` wires it. */
function FkGrid({ rows }: { rows: Record<string, unknown>[] }) {
  const navigation = useTableForeignKeyNavigation({
    panelId: SOURCE_PANEL,
    dbSessionId: 'session-1',
    database: 'app',
    tableName: 'orders',
    schema: null,
    connectionId: 'conn-1',
    databaseType: 'postgresql',
    rows,
    displayedColumns: COLUMNS,
    rowHeight: 28,
    activeCell: null,
  });
  const rowArrays = useMemo(
    () => rows.map((row) => COLUMNS.map((column) => row[column.name] ?? null)),
    [rows],
  );
  return <DataTable columns={COLUMNS} rows={rowArrays} foreignKeys={navigation.foreignKeys} />;
}

function arrowOf(container: HTMLElement, rowIndex: number, columnName: string): HTMLElement | null {
  const cell = container.querySelector(`[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`);
  if (!(cell instanceof HTMLElement)) return null;
  const arrow = cell.querySelector('[data-dt-fk-click="true"]');
  return arrow instanceof HTMLElement ? arrow : null;
}

function request(): Record<string, unknown> {
  const call = mockInvoke.mock.calls.find((entry) => entry[0] === 'resolve_foreign_key_targets');
  return (call?.[1] as { request: Record<string, unknown> }).request;
}

beforeEach(() => {
  vi.clearAllMocks();
  resetFkNavigationEntryIds();
  schemaState.foreignKeys = [SINGLE];
  usePanelStore.setState({ panels: [], activePanelId: null });
  useTableDataStore.getState().reset();
  useFkNavigationStore.getState().reset();
  usePanelStore.getState().addPanel(sourcePanel());
  setBackendClient(
    DESKTOP_BACKEND_ID,
    createBackendClient(DESKTOP_BACKEND_ID, createDesktopBackendTransport()),
  );
  mockInvoke.mockResolvedValue({
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
    candidates: [],
    truncated: false,
  });
});

afterEach(cleanup);

describe('audit: the request carries the row that was clicked', () => {
  it('carries the columns and the values of the clicked row', async () => {
    const { container } = render(
      <FkGrid
        rows={[
          { id: 1, customer_id: 7 },
          { id: 2, customer_id: 9 },
        ]}
      />,
    );
    await waitFor(() => expect(arrowOf(container, 1, 'customer_id')).not.toBeNull());

    // The **second** row is the one that is clicked.
    fireEvent.click(arrowOf(container, 1, 'customer_id') as HTMLElement);

    await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
    const body = request();
    expect(body.columns).toEqual(['customer_id']);
    // The value of row 1 — not the value of row 0.
    expect(body.values).toEqual([9]);
    expect(body.constraintName).toBe('orders_customer_fk');
  });

  it('carries both members of a composite key, in pairing order', async () => {
    schemaState.foreignKeys = [COMPOSITE];
    const { container } = render(<FkGrid rows={[{ id: 1, parent_a: 3, parent_b: 4 }]} />);
    await waitFor(() => expect(arrowOf(container, 0, 'parent_a')).not.toBeNull());

    fireEvent.click(arrowOf(container, 0, 'parent_a') as HTMLElement);

    await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
    const body = request();
    expect(body.columns).toEqual(['parent_a', 'parent_b']);
    expect(body.values).toEqual([3, 4]);
  });

  it('never sends a NULL member, and never asks for IS NULL', async () => {
    schemaState.foreignKeys = [COMPOSITE];
    const { container } = render(<FkGrid rows={[{ id: 1, parent_a: 3, parent_b: null }]} />);
    // The group is incomplete: there is no arrow at all.
    await waitFor(() => expect(container.querySelector('[data-dt-fk]')).not.toBeNull());
    expect(container.querySelectorAll('[data-dt-fk-click="true"]')).toHaveLength(0);

    // And nothing was asked of the host.
    expect(mockInvoke).not.toHaveBeenCalled();
  });
});
