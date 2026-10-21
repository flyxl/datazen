/**
 * The continuous journey of the filter editor (08 §9.3, AGENTS.md's hard rule
 * for interaction code).
 *
 * One journey, not a set of isolated legal states: open the panel → add a
 * condition → the incomplete middle state blocks Apply → fill the value →
 * give a condition its own connector → Apply → the request body carries the
 * per-condition connector. The second leg is the quick filter: an expression
 * that can be written down as a flat list goes on the wire unchanged, and one
 * that needs parentheses is refused instead of being flattened into a
 * different question.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { FilterEditor } from '../FilterEditor';
import { TableToolbar } from '../../windows/connection/TableToolbar';
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

vi.mock('../../components/ai/NlFilterInput', () => ({
  NlFilterInput: () => <div data-testid="mock-nl-filter" />,
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

/** The toolbar wired to the real store, the way `TableView` wires it. */
function ToolbarHarness() {
  const ts = useTableDataStore((state) => state.byPanel.get(PANEL));
  const store = () => useTableDataStore.getState();
  return (
    <TableToolbar
      panelId={PANEL}
      dbSessionId="conn-1"
      database="app"
      tableName="users"
      columns={COLUMNS}
      loading={false}
      pendingBusy={false}
      isEditable
      filterPanelOpen={false}
      appliedFilterCount={ts?.filters.length ?? 0}
      draftFilterCount={ts?.draftFilters.length ?? 0}
      onRefresh={vi.fn()}
      onSetFilterPanelOpen={(open) => store().setFilterPanelOpen(PANEL, open)}
      onAddFilter={(filter) => store().addFilter(PANEL, filter)}
      onSetFilters={(filters, logic) => store().setFilters(PANEL, filters, logic)}
      onClearFilters={() => store().clearFilters(PANEL)}
      onAddDraftRow={vi.fn()}
    />
  );
}

describe('the filter editor journey', () => {
  it('goes from an empty panel to a request that carries the per-condition connector', async () => {
    render(<PanelHarness />);

    // 1. An empty panel offers the "add" entry.
    expect(screen.getAllByText('filter.noActive').length).toBeGreaterThan(0);

    // 2. Adding a condition drafts an equality without a value.
    fireEvent.click(screen.getByTestId('filter-add'));
    const drafted = useTableDataStore.getState().byPanel.get(PANEL)?.draftFilters ?? [];
    expect(drafted).toEqual([{ column: 'id', operator: 'eq', value: '' }]);

    // 3. The incomplete middle state: the condition is marked and Apply is
    //    blocked, so nothing is sent.
    expect(screen.getByTestId('filter-incomplete')).toBeInTheDocument();
    expect(screen.getByTestId('filter-incomplete-error')).toBeInTheDocument();
    expect(screen.getByTestId('filter-apply')).toBeDisabled();
    fireEvent.click(screen.getByTestId('filter-apply'));
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();

    // 4. Typing the value completes the condition; Enter folds it into a chip.
    const valueInput = screen.getByPlaceholderText('filter.value');
    fireEvent.change(valueInput, { target: { value: 'alice' } });
    await vi.waitFor(() =>
      expect(useTableDataStore.getState().byPanel.get(PANEL)?.draftFilters[0]?.value).toBe('alice'),
    );
    fireEvent.keyDown(valueInput, { key: 'Enter' });
    await vi.waitFor(() => expect(screen.queryByPlaceholderText('filter.value')).toBeNull());

    // 5. A second condition, with a connector of its own.
    fireEvent.click(screen.getByTestId('filter-add'));
    fireEvent.click(screen.getByTestId('filter-connector-or'));
    const withConnector = useTableDataStore.getState().byPanel.get(PANEL)?.draftFilters ?? [];
    expect(withConnector[1]?.connector).toBe('or');
    // The request wide logic is out of the picture while a row is explicit.
    expect(screen.getByTestId('filter-logic-and')).toBeDisabled();

    // 6. The second condition needs a value too.
    fireEvent.change(screen.getByPlaceholderText('filter.value'), { target: { value: 'bob' } });
    await vi.waitFor(() =>
      expect(useTableDataStore.getState().byPanel.get(PANEL)?.draftFilters[1]?.value).toBe('bob'),
    );

    // 7. Apply sends the conditions as they are, the connector included.
    fireEvent.click(screen.getByTestId('filter-apply'));
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    const request = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0] as {
      filters: FilterCondition[];
      filterLogic: string;
    };
    expect(request.filters).toEqual([
      { column: 'id', operator: 'eq', value: 'alice' },
      { column: 'id', operator: 'eq', value: 'bob', connector: 'or' },
    ]);
    expect(request.filterLogic).toBe('and');
  });

  it('sends a quick filter that mixes AND and OR without changing its meaning', async () => {
    const { container } = render(<ToolbarHarness />);
    const quick = container.querySelector(
      '[data-testid="table-quick-filter"]',
    ) as HTMLInputElement;

    fireEvent.change(quick, { target: { value: "id = 1 OR name = 'a' AND id > 0" } });
    fireEvent.keyDown(quick, { key: 'Enter' });

    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    const request = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0] as {
      filters: FilterCondition[];
      filterLogic: string;
    };
    // The connectors travel with the conditions, so the host can render
    // `a OR (b AND c)` instead of reading the list from left to right.
    expect(request.filters.map((filter) => filter.connector)).toEqual([undefined, 'or', 'and']);
    expect(request.filterLogic).toBe('or');
  });

  it('refuses a quick filter whose parentheses group a different operator', async () => {
    const { container } = render(<ToolbarHarness />);
    const quick = container.querySelector(
      '[data-testid="table-quick-filter"]',
    ) as HTMLInputElement;

    fireEvent.change(quick, { target: { value: "id = 1 AND (name = 'a' OR id > 0)" } });
    fireEvent.keyDown(quick, { key: 'Enter' });

    await vi.waitFor(() => expect(container.textContent).toContain('filter.nestedLogic'));
    // Nothing was sent: the expression would have meant something else.
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();
    expect(useTableDataStore.getState().byPanel.get(PANEL)?.filters).toEqual([]);
  });
});
