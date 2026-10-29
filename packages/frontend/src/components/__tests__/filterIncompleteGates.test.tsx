/**
 * Independent re-verification of the two gates in front of an incomplete
 * condition (08 §4.5 / Step 10).
 *
 * The claim under test:
 *
 * 1. **The request body carries the conditions as they are.** An incomplete
 *    condition is *not* stripped any more — the host refuses it with
 *    `grid.filter.incomplete`. The old behaviour stripped it, which made the
 *    result set wider than the filter list on screen.
 * 2. **There are two gates.** The editor's `Apply` button is the first one
 *    (disabled + a visible reason); the store's `applyFilters` is the second
 *    one (it refuses an incomplete draft for every other caller).
 * 3. **The programmatic path still sends it.** `setFilters` is the escape
 *    hatch: it puts the conditions on the wire and lets the host reject them.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { FilterEditor } from '../FilterEditor';
import type { ColumnSchema, FilterCondition } from '../../types';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../ui/Select', () => ({
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

const PANEL = 'panel-users';

const COLUMNS: ColumnSchema[] = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true, isPrimaryKey: false },
];

const EDITOR_COLUMNS = [
  { name: 'id', type: 'integer' },
  { name: 'name', type: 'text' },
];

const sampleResponse = {
  columns: COLUMNS,
  rows: [[1, 'Alice']] as (string | number | null)[][],
  totalRows: 1,
  page: 0,
  pageSize: 50,
};

let useTableDataStore: typeof import('../../stores/tableDataStore').useTableDataStore;

beforeEach(async () => {
  vi.resetModules();
  vi.clearAllMocks();
  mockDatabaseCommands.getTableData.mockResolvedValue(sampleResponse);
  const mod = await import('../../stores/tableDataStore');
  useTableDataStore = mod.useTableDataStore;
  useTableDataStore.getState().reset();
  await useTableDataStore.getState().loadTableData({
    panelId: PANEL,
    dbSessionId: 'conn-1',
    table: 'users',
    connectionId: 'conn-1',
    driverType: 'postgres',
    database: 'app',
    schema: null,
  });
  mockDatabaseCommands.getTableData.mockClear();
});

afterEach(cleanup);

/** The panel wired to the real store, the way `TableView` wires it. */
function PanelHarness() {
  const ts = useTableDataStore((state) => state.byPanel.get(PANEL));
  const store = () => useTableDataStore.getState();
  return (
    <FilterEditor
      columns={EDITOR_COLUMNS}
      appliedFilters={ts?.filters ?? []}
      appliedLogic={ts?.filterLogic ?? 'and'}
      draftFilters={ts?.draftFilters ?? []}
      draftLogic={ts?.draftFilterLogic ?? 'and'}
      open
      onOpenChange={(open) => store().setFilterPanelOpen(PANEL, open)}
      onLogicChange={(logic) => store().setFilterLogic(PANEL, logic)}
      onChange={(index, next) => store().updateFilter(PANEL, index, next)}
      onAdd={(filter) => store().addFilter(PANEL, filter)}
      onRemove={(index) => store().removeFilter(PANEL, index)}
      onApply={() => store().applyFilters(PANEL)}
      onClear={() => store().clearFilters(PANEL)}
    />
  );
}

function loaded() {
  const state = useTableDataStore.getState().byPanel.get(PANEL);
  if (!state) throw new Error('the panel has no slice');
  return state;
}

describe('an incomplete condition meets two gates', () => {
  it('gate 1 — the editor marks the condition, blocks Apply and sends nothing', async () => {
    render(<PanelHarness />);

    fireEvent.click(screen.getByTestId('filter-add'));
    // The drafted condition has no value yet.
    expect(loaded().draftFilters).toEqual([{ column: 'id', operator: 'eq', value: '' }]);

    // The visible marks of the incomplete middle state.
    expect(screen.getByTestId('filter-incomplete')).toBeInTheDocument();
    expect(screen.getByTestId('filter-incomplete-error')).toBeInTheDocument();
    expect(screen.getByTestId('filter-apply')).toBeDisabled();

    // Clicking a disabled button does nothing; nothing is sent either way.
    fireEvent.click(screen.getByTestId('filter-apply'));
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();
    expect(loaded().filters).toEqual([]);
  });

  it('gate 2 — the store refuses an incomplete draft for a programmatic caller', async () => {
    await useTableDataStore.getState().loadTableData({
      panelId: PANEL,
      dbSessionId: 'conn-1',
      table: 'users',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
    mockDatabaseCommands.getTableData.mockClear();

    // A caller that stages a draft by hand and then applies it.
    useTableDataStore.getState().addFilter(PANEL, { column: 'id', operator: 'eq', value: '' });
    useTableDataStore.getState().addFilter(PANEL, { column: 'name', operator: 'eq', value: 'Bob' });

    const before = loaded();
    const beforeFilters = JSON.stringify(before.filters);
    const beforeRevision = before.requestRevision;

    useTableDataStore.getState().applyFilters(PANEL);

    const after = loaded();
    expect(JSON.stringify(after.filters)).toBe(beforeFilters);
    expect(after.requestRevision).toBe(beforeRevision);
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();
  });

  it('the programmatic path still puts an incomplete condition on the wire', async () => {
    const conditions: FilterCondition[] = [
      { column: 'id', operator: 'eq', value: '' },
      { column: 'name', operator: 'eq', value: 'Bob' },
    ];
    useTableDataStore.getState().setFilters(PANEL, conditions);

    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    const request = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0] as {
      filters: FilterCondition[];
      filterLogic: string;
    };

    // The request body, verbatim: the incomplete condition is **not** stripped.
    expect(JSON.stringify(request.filters)).toBe(
      JSON.stringify([
        { column: 'id', operator: 'eq', value: '' },
        { column: 'name', operator: 'eq', value: 'Bob' },
      ]),
    );
    expect(request.filters).toHaveLength(2);
    expect(request.filters[0]).toEqual({ column: 'id', operator: 'eq', value: '' });
    expect(request.filters[1]).toEqual({ column: 'name', operator: 'eq', value: 'Bob' });
    expect(request.filterLogic).toBe('and');
  });

  it('a complete filter list still goes on the wire unchanged', async () => {
    useTableDataStore.getState().setFilters(PANEL, [
      { column: 'name', operator: 'eq', value: 'Bob' },
    ]);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    const request = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0] as {
      filters: FilterCondition[];
    };
    expect(JSON.stringify(request.filters)).toBe(
      JSON.stringify([{ column: 'name', operator: 'eq', value: 'Bob' }]),
    );
  });
});
