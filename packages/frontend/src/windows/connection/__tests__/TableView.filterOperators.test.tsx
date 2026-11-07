/**
 * The end-to-end proof that the **shipped** path and the **tested** path
 * agree on which filter operators a driver offers (08 §3.5 / FF-D1).
 *
 * The defect this guards against is subtle and expensive: the rule "a driver
 * that declares no operators gets the shared nineteen" lived in
 * `supportedOperatorsForMeta`, while the panel was fed the driver's raw
 * array. A driver with `dataGrid: { filterOperators: [] }` therefore got the
 * shared set from the helper and **one** option in the editor — the user
 * could not change the operator any more.
 *
 * So this test goes through the **production wiring**: a driver meta in the
 * registry → `TableView` → `DataTable` → `FilterEditor` → the operator
 * dropdown. A unit test of the helper could not catch the panel bypassing it.
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, screen } from '@testing-library/react';
import { TableView } from '../TableView';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { TableState } from '../../../stores/tableData/types';
import type { TableChangeContext } from '../../../lib/tableChanges';
import { SHARED_FILTER_OPERATORS } from '../../../types';
import { operatorsForColumn } from '../../../lib/filterOperators';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(false), null],
}));

const settingsState = vi.hoisted(() => ({ confirmOnDelete: false, safeMode: false }));
const connectionsState = vi.hoisted(() => ({
  connections: [] as { id: string; name: string; readOnly?: boolean }[],
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { settings: typeof settingsState }) => unknown) =>
    sel({ settings: settingsState }),
}));

vi.mock('../../../stores/connectionStore', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../stores/connectionStore')>();
  return {
    ...actual,
    useConnectionStore: Object.assign(
      (selector?: (s: typeof connectionsState) => unknown) =>
        selector ? selector(connectionsState) : connectionsState,
      {
        getState: () => connectionsState,
        setState: (partial: Partial<typeof connectionsState>) =>
          Object.assign(connectionsState, partial),
      },
    ),
  };
});

/**
 * The registry the panel reads its capabilities from.
 *
 * `postgresql` declares an **empty** operator list — a driver that says
 * nothing must still get the shared set, or the operator dropdown collapses
 * to the one option that happens to be selected.
 */
const registryState = vi.hoisted(() => ({
  registry: {
    postgresql: { dataGrid: { filterOperators: [] } },
  } as Record<string, { dataGrid?: { filterOperators?: string[] } }>,
}));

vi.mock('../../../lib/databaseTypes', () => ({
  get DB_REGISTRY() {
    return registryState.registry;
  },
  escapeIdent: (ident: string) => ident,
}));

const tableController = vi.hoisted(() => ({
  snapshot: { state: 'ready' },
  refresh: vi.fn().mockResolvedValue({ state: 'ready', observedContext: { transactionState: 'none' } }),
  execute: vi.fn().mockResolvedValue({ executionId: 'table-tx' }),
  getExecution: vi.fn().mockResolvedValue({ state: 'succeeded' }),
}));
vi.mock('../../../lib/session/tableSession', () => ({
  tableSessionController: vi.fn().mockResolvedValue(tableController),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    useDatabase: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../components/ai/NlFilterInput', () => ({
  NlFilterInput: () => <div data-testid="mock-nl-filter" />,
}));

// The grid's own helpers do not matter here; the panel does.
vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({ columnWidths: [160], onResizeStart: vi.fn() }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

/** A native select, so the offered options are readable from the DOM. */
vi.mock('../../../components/ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
  }: {
    value: string;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
  }) => (
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  ),
}));

const tableStore = vi.hoisted(() => ({
  byPanel: new Map<string, TableState>(),
  loadTableData: vi.fn(),
  reloadPanel: vi.fn(),
  setSort: vi.fn(),
  removeFilter: vi.fn(),
  clearFilters: vi.fn(),
  addFilter: vi.fn(),
  setFilters: vi.fn(),
  updateFilter: vi.fn(),
  setFilterLogic: vi.fn(),
  applyFilters: vi.fn(),
  setFilterPanelOpen: vi.fn(),
  setVisibleColumns: vi.fn(),
  setPage: vi.fn(),
  setPageSize: vi.fn(),
  startEdit: vi.fn(),
  stageCellChange: vi.fn(),
  cancelEdit: vi.fn(),
  selectRow: vi.fn(),
  toggleSelectAll: vi.fn(),
  deleteRows: vi.fn(),
  previewPendingChanges: vi.fn().mockResolvedValue(null),
  commitPendingChanges: vi.fn().mockResolvedValue({}),
  rollbackPendingChanges: vi.fn(),
  setDetailRow: vi.fn(),
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: Object.assign((sel: (s: typeof tableStore) => unknown) => sel(tableStore), {
    getState: () => tableStore,
  }),
}));

const PANEL = 'panel-users';

function contextOf(): TableChangeContext {
  return {
    connectionId: null,
    dbSessionId: 'c1',
    driverType: 'postgresql',
    database: 'app',
    schema: null,
    table: 'users',
  };
}

/** Seed the panel with an open filter panel and one unfinished condition. */
function seedPanelWithOpenFilter() {
  tableStore.byPanel.set(PANEL, {
    ...emptyTableState(contextOf()),
    columns: [{ name: 'name', dataType: 'text', nullable: true }],
    rows: [{ name: 'alice' }],
    filterPanelOpen: true,
    draftFilters: [{ column: 'name', operator: 'eq', value: '' }],
  });
}

function renderTable() {
  return render(
    <TableView
      panelId={PANEL}
      dbSessionId="c1"
      database="app"
      tableName="users"
      databaseType="postgresql"
    />,
  );
}

/** The operator dropdown of the expanded condition. */
function operatorOptions(): string[] {
  const selects = screen.getAllByRole('combobox');
  // The condition editor renders the column select first, the operator
  // select second.
  const operatorSelect = selects[1] as HTMLSelectElement;
  return Array.from(operatorSelect.options).map((option) => option.value);
}

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  settingsState.confirmOnDelete = false;
  settingsState.safeMode = false;
  connectionsState.connections = [];
  tableStore.byPanel = new Map();
});

describe('the filter operators the panel offers (shipped path)', () => {
  it('offers the shared set when the driver declares no operators', () => {
    seedPanelWithOpenFilter();
    renderTable();

    // The driver declared `filterOperators: []`. That means "said nothing",
    // not "serves nothing": the panel must still offer the shared set —
    // narrowed by the column type, which is the second rule and always was.
    const offered = operatorOptions();
    expect(offered).toEqual(operatorsForColumn('text', SHARED_FILTER_OPERATORS));
    expect(offered.length).toBeGreaterThan(1);
    // The operators the shared set has and the column type takes.
    expect(offered).toContain('contains');
    expect(offered).toContain('startsWith');
    expect(offered).toContain('isNull');
  });

  it('offers only what the driver declared when it declared a subset', () => {
    registryState.registry.postgresql = { dataGrid: { filterOperators: ['eq', 'ne'] } };
    seedPanelWithOpenFilter();
    renderTable();

    expect(operatorOptions()).toEqual(['eq', 'ne']);
  });
});
