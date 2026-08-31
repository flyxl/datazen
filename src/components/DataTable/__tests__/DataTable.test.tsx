import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { DataTable } from '../DataTable';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({
    columnWidths: [160, 200],
    onResizeStart: vi.fn(),
  }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../FilterBar', () => ({
  FilterBar: ({ onClear, loading }: { onClear: () => void; loading?: boolean }) => (
    <div data-testid="filter-bar" data-loading={String(Boolean(loading))}>
      <button type="button" onClick={onClear}>
        clear-filters
      </button>
    </div>
  ),
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

const { showNativeContextMenu } = vi.hoisted(() => ({
  showNativeContextMenu: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: (...args: unknown[]) => showNativeContextMenu(...args),
}));

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

const saveTextWithDialog = vi.fn().mockResolvedValue(true);

vi.mock('../../../commands/file', () => ({
  fileCommands: {
    saveTextWithDialog: (...args: unknown[]) => saveTextWithDialog(...args),
    saveBase64WithDialog: vi.fn().mockResolvedValue(true),
  },
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
});

describe('DataTable', () => {
  const rows = [
    [1, 'Alice'],
    [2, 'Bob'],
  ];

  it('renders header and body rows', () => {
    const { getByText } = render(<DataTable columns={COLS} rows={rows} />);
    expect(getByText('id')).toBeInTheDocument();
    expect(getByText('Alice')).toBeInTheDocument();
  });

  it('shows filter bar and pagination when props provided', () => {
    const onPageChange = vi.fn();
    const onClearFilters = vi.fn();
    const { getByTestId, getByText } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        filters={[{ column: 'name', operator: 'eq', value: 'Alice' }]}
        onRemoveFilter={vi.fn()}
        onClearFilters={onClearFilters}
        page={0}
        pageSize={25}
        totalRows={100}
        onPageChange={onPageChange}
        onPageSizeChange={vi.fn()}
      />,
    );
    expect(getByTestId('filter-bar')).toBeInTheDocument();
    fireEvent.click(getByText('clear-filters'));
    expect(onClearFilters).toHaveBeenCalled();
    expect(getByText('1-25 / 100')).toBeInTheDocument();
  });

  it('threads loading to the filter bar and pagination', () => {
    const { getByTestId, container } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        filters={[{ column: 'name', operator: 'eq', value: 'Alice' }]}
        onRemoveFilter={vi.fn()}
        onClearFilters={vi.fn()}
        page={0}
        pageSize={25}
        totalRows={100}
        onPageChange={vi.fn()}
        onPageSizeChange={vi.fn()}
        loading
      />,
    );

    expect(getByTestId('filter-bar')).toHaveAttribute('data-loading', 'true');
    expect(container.querySelector('[aria-busy="true"]')).toBeInTheDocument();
  });

  it('threads loading to the expanded filter editor', () => {
    const filter = { column: 'name', operator: 'eq' as const, value: 'Alice' };
    const { getByTestId } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        filters={[filter]}
        draftFilters={[filter]}
        filterPanelOpen
        onFilterPanelOpenChange={vi.fn()}
        onAddFilter={vi.fn()}
        onUpdateFilter={vi.fn()}
        onRemoveFilter={vi.fn()}
        onClearFilters={vi.fn()}
        onFilterLogicChange={vi.fn()}
        onApplyFilters={vi.fn()}
        loading
      />,
    );

    expect(getByTestId('filter-editor')).toHaveAttribute('aria-busy', 'true');
  });

  it('shows selection bar and select-all checkbox', () => {
    const onSelectAll = vi.fn();
    const onRowSelect = vi.fn();
    const { container } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        selectedRows={new Set([0])}
        onSelectAll={onSelectAll}
        onRowSelect={onRowSelect}
        exportTableName="users"
      />,
    );
    const checkbox = container.querySelector('input[type="checkbox"]')!;
    fireEvent.click(checkbox);
    expect(onSelectAll).toHaveBeenCalled();
    expect(container.textContent).toContain('dataTable.selected');
  });

  it('opens export dialog from toolbar button', async () => {
    const { getByTitle, getByText } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        selectedRows={new Set()}
        onSelectAll={vi.fn()}
        onRowSelect={vi.fn()}
        exportTableName="users"
      />,
    );
    fireEvent.click(getByTitle('export.export'));
    await waitFor(() => {
      expect(getByText('common.exportData')).toBeInTheDocument();
    });
  });

  it('shows bottom export bar when no selection handlers', async () => {
    const { getAllByTitle, getByText, getAllByText } = render(
      <DataTable columns={COLS} rows={rows} exportTableName="users" />,
    );
    const exportBtns = getAllByTitle('export.export');
    expect(exportBtns.length).toBeGreaterThan(0);
    fireEvent.click(exportBtns[0]);
    await waitFor(() => expect(getByText('common.exportData')).toBeInTheDocument());
    const dialogExportBtns = getAllByText('export.export');
    fireEvent.click(dialogExportBtns[dialogExportBtns.length - 1]);
    await waitFor(() => expect(saveTextWithDialog).toHaveBeenCalled());
  });

  it('opens native context menu with TablePlus-style items when a cell is hit', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    const { container, getByText, getAllByText } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        selectedRows={new Set([0, 1])}
        onSelectAll={vi.fn()}
        onRowSelect={vi.fn()}
        onAddFilter={vi.fn()}
        onUpdateFilter={vi.fn()}
        onRemoveFilter={vi.fn()}
        onClearFilters={vi.fn()}
        onFilterLogicChange={vi.fn()}
        onApplyFilters={vi.fn()}
        onFilterPanelOpenChange={vi.fn()}
        exportTableName="users"
      />,
    );
    const cell = container.querySelector('[data-dt-row="0"][data-dt-col="name"]');
    expect(cell).toBeTruthy();
    fireEvent.contextMenu(cell!, { clientX: 10, clientY: 10 });

    await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalled());
    const menuItems = showNativeContextMenu.mock.calls[0]![0] as Array<{
      kind: string;
      id?: string;
      action?: () => void;
    }>;
    const itemIds = menuItems.filter((i) => i.kind === 'item').map((i) => i.id);
    expect(itemIds).toEqual([
      'copy',
      'copy-row',
      'copy-as-json',
      'copy-as-sql-insert',
      'copy-as-update',
      'copy-as-csv',
      'copy-column-name',
      'filter-by-value',
      'copy-selected-rows',
      'export',
    ]);

    menuItems.find((i) => i.id === 'copy')!.action?.();
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('Alice'));

    menuItems.find((i) => i.id === 'export')!.action?.();
    await waitFor(() => expect(getByText('common.exportData')).toBeInTheDocument());
    const dialogExportBtns = getAllByText('export.export');
    fireEvent.click(dialogExportBtns[dialogExportBtns.length - 1]);
    await waitFor(() => expect(saveTextWithDialog).toHaveBeenCalled());
  });

  it('copies selected rows without cell hit', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    const { container } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        selectedRows={new Set([0])}
        onSelectAll={vi.fn()}
        onRowSelect={vi.fn()}
        exportTableName="users"
      />,
    );
    fireEvent.contextMenu(container.querySelector('.overflow-auto')!);

    await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalled());
    const menuItems = showNativeContextMenu.mock.calls[0]![0] as Array<{
      kind: string;
      id?: string;
      action?: () => void;
    }>;
    expect(menuItems.filter((i) => i.kind === 'item').map((i) => i.id)).toEqual([
      'copy-selected-rows',
      'copy-as-csv',
      'export',
    ]);
    menuItems.find((i) => i.id === 'copy-selected-rows')!.action?.();
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('1\tAlice'));
  });

  it('calls row click handlers', () => {
    const onRowClick = vi.fn();
    const onRowSelect = vi.fn();
    const { container } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        onRowClick={onRowClick}
        onRowSelect={onRowSelect}
        highlightedRow={0}
      />,
    );
    const row = container.querySelector('[tabindex="0"]')!;
    fireEvent.click(row);
    expect(onRowClick).toHaveBeenCalledWith(0);
    expect(onRowSelect).toHaveBeenCalledWith(0, { multi: false, range: false });
  });

  it('deletes selected rows from the toolbar and Delete key', () => {
    const onDeleteRows = vi.fn();
    const { getByTestId, container } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        selectedRows={new Set([0])}
        onSelectAll={vi.fn()}
        onRowSelect={vi.fn()}
        primaryKeyColumns={['id']}
        onDeleteRows={onDeleteRows}
      />,
    );
    getByTestId('data-table-delete-rows').click();
    expect(onDeleteRows).toHaveBeenCalledWith([0]);
    onDeleteRows.mockClear();
    fireEvent.keyDown(container.firstChild as HTMLElement, { key: 'Delete' });
    expect(onDeleteRows).toHaveBeenCalledWith([0]);
  });

  it('shows loading indicator in selection bar', () => {
    const { getByText } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        loading
        selectedRows={new Set()}
        onSelectAll={vi.fn()}
        onRowSelect={vi.fn()}
      />,
    );
    expect(getByText('common.loading')).toBeInTheDocument();
  });
});
