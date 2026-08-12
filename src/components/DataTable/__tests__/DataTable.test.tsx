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
  FilterBar: ({ onClear }: { onClear: () => void }) => (
    <div data-testid="filter-bar">
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
      expect(getByText('export.title')).toBeInTheDocument();
    });
  });

  it('shows bottom export bar when no selection handlers', async () => {
    const { getAllByTitle, getByText, getAllByText } = render(
      <DataTable columns={COLS} rows={rows} exportTableName="users" />,
    );
    const exportBtns = getAllByTitle('export.export');
    expect(exportBtns.length).toBeGreaterThan(0);
    fireEvent.click(exportBtns[0]);
    await waitFor(() => expect(getByText('export.title')).toBeInTheDocument());
    const dialogExportBtns = getAllByText('export.export');
    fireEvent.click(dialogExportBtns[dialogExportBtns.length - 1]);
    await waitFor(() => expect(saveTextWithDialog).toHaveBeenCalled());
  });

  it('opens native context menu with export and copies selected rows', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    const { container, getByText, getAllByText } = render(
      <DataTable
        columns={COLS}
        rows={rows}
        selectedRows={new Set([0, 1])}
        onSelectAll={vi.fn()}
        onRowSelect={vi.fn()}
        exportTableName="users"
        getContextCellText={() => 'Alice'}
      />,
    );
    const scrollArea = container.querySelector('.overflow-auto')!;
    fireEvent.contextMenu(scrollArea, { clientX: 10, clientY: 10 });

    await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalled());
    const menuItems = showNativeContextMenu.mock.calls[0]![0] as Array<{
      kind: string;
      id?: string;
      label?: string;
      action?: () => void;
    }>;
    expect(menuItems.map((i) => i.id)).toEqual(['copy-cell', 'copy-selected-rows', 'export']);
    expect(menuItems[2]!.label).toMatch(/export.selectedRows/);

    menuItems[1]!.action?.();
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('1\tAlice\n2\tBob');
    });

    menuItems[2]!.action?.();
    await waitFor(() => expect(getByText('export.title')).toBeInTheDocument());
    const dialogExportBtns = getAllByText('export.export');
    fireEvent.click(dialogExportBtns[dialogExportBtns.length - 1]);
    await waitFor(() => expect(saveTextWithDialog).toHaveBeenCalled());
  });

  it('copies cell text from selection when getContextCellText is absent', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    const selection = { toString: () => '  selected-cell  ' };
    vi.spyOn(window, 'getSelection').mockReturnValue(selection as unknown as Selection);

    const { container } = render(<DataTable columns={COLS} rows={rows} exportTableName="users" />);
    fireEvent.contextMenu(container.querySelector('.overflow-auto')!);

    await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalled());
    const menuItems = showNativeContextMenu.mock.calls[0]![0] as Array<{
      kind: string;
      id?: string;
      action?: () => void;
    }>;
    expect(menuItems.map((i) => i.id)).toContain('copy-cell');
    expect(menuItems.map((i) => i.id)).toContain('export');
    menuItems.find((i) => i.id === 'copy-cell')!.action?.();
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('selected-cell'));
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
