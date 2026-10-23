/**
 * Independent re-verification of the per-condition connector journey (08 §4.7 /
 * Step 11) — the three inputs, plus the incomplete middle state.
 *
 * Every leg captures the **raw request body**, so the claim is checked against
 * the bytes that go on the wire and not against a re-derived summary.
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

/** The raw request body of the last `getTableData` call. */
function lastRequest(): { filters: FilterCondition[]; filterLogic: string } {
  const call = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0] as {
    filters: FilterCondition[];
    filterLogic: string;
  };
  return call;
}

describe('the connector journey', () => {
  it('leg 1 — the second panel condition carries its own connector', async () => {
    render(<PanelHarness />);

    // Two complete conditions.
    fireEvent.click(screen.getByTestId('filter-add'));
    fireEvent.change(screen.getByPlaceholderText('filter.value'), { target: { value: 'alice' } });
    await vi.waitFor(() =>
      expect(
        useTableDataStore.getState().byPanel.get(PANEL)?.draftFilters[0]?.value,
      ).toBe('alice'),
    );
    fireEvent.click(screen.getByTestId('filter-add'));
    // The first condition collapses into a chip, so the only open value field
    // is the second one.
    const valueFields = screen.getAllByPlaceholderText('filter.value');
    expect(valueFields).toHaveLength(1);
    fireEvent.change(valueFields[0], { target: { value: 'bob' } });
    await vi.waitFor(() =>
      expect(
        useTableDataStore.getState().byPanel.get(PANEL)?.draftFilters[1]?.value,
      ).toBe('bob'),
    );

    // The second condition gets `or` of its own.
    fireEvent.click(screen.getByTestId('filter-connector-or'));

    // The request wide logic is out of the picture while a row is explicit.
    expect(screen.getByTestId('filter-logic-and')).toBeDisabled();

    fireEvent.click(screen.getByTestId('filter-apply'));
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());

    const request = lastRequest();
    // The request body, verbatim.
    expect(JSON.stringify(request.filters)).toBe(
      JSON.stringify([
        { column: 'id', operator: 'eq', value: 'alice' },
        { column: 'id', operator: 'eq', value: 'bob', connector: 'or' },
      ]),
    );
    expect(request.filterLogic).toBe('and');
  });

  it('leg 2 — a quick filter that mixes AND and OR keeps its connectors', async () => {
    const { container } = render(<ToolbarHarness />);
    const quick = container.querySelector(
      '[data-testid="table-quick-filter"]',
    ) as HTMLInputElement;

    fireEvent.change(quick, { target: { value: "id = 1 OR name = 'a' AND id > 0" } });
    fireEvent.keyDown(quick, { key: 'Enter' });

    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    const request = lastRequest();

    // The connector sequence, verbatim.
    expect(JSON.stringify(request.filters.map((filter) => filter.connector))).toBe(
      JSON.stringify([undefined, 'or', 'and']),
    );
    expect(request.filters).toHaveLength(3);
    expect(request.filters[0]).toEqual({ column: 'id', operator: 'eq', value: 1 });
    expect(request.filters[1]).toEqual({
      column: 'name',
      operator: 'eq',
      value: 'a',
      connector: 'or',
    });
    expect(request.filters[2]).toEqual({ column: 'id', operator: 'gt', value: 0, connector: 'and' });
    // The whole body, verbatim.
    expect(JSON.stringify(request.filters)).toBe(
      JSON.stringify([
        { column: 'id', operator: 'eq', value: 1 },
        { column: 'name', operator: 'eq', value: 'a', connector: 'or' },
        { column: 'id', operator: 'gt', value: 0, connector: 'and' },
      ]),
    );
    expect(request.filterLogic).toBe('or');
  });

  it('leg 3 — a quick filter whose parentheses group a different operator is refused', async () => {
    const { container } = render(<ToolbarHarness />);
    const quick = container.querySelector(
      '[data-testid="table-quick-filter"]',
    ) as HTMLInputElement;

    fireEvent.change(quick, { target: { value: "id = 1 AND (name = 'a' OR id > 0)" } });
    fireEvent.keyDown(quick, { key: 'Enter' });

    await vi.waitFor(() => expect(container.textContent).toContain('filter.nestedLogic'));
    await new Promise((resolve) => setTimeout(resolve, 30));
    // Nothing was sent, and the applied filters stayed empty.
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();
    expect(useTableDataStore.getState().byPanel.get(PANEL)?.filters).toEqual([]);
  });

  it('the incomplete middle state blocks the whole journey', async () => {
    render(<PanelHarness />);

    fireEvent.click(screen.getByTestId('filter-add'));

    // The row is marked, the reason is shown, the button is disabled.
    expect(screen.getByTestId('filter-incomplete')).toBeInTheDocument();
    expect(screen.getByTestId('filter-incomplete-error')).toBeInTheDocument();
    expect(screen.getByTestId('filter-apply')).toBeDisabled();

    // A click on the disabled button sends nothing.
    fireEvent.click(screen.getByTestId('filter-apply'));
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();
    expect(useTableDataStore.getState().byPanel.get(PANEL)?.filters).toEqual([]);
  });
});
