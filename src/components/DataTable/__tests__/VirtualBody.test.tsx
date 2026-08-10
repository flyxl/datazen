import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { VirtualBody } from '../VirtualBody';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({
      index,
      key: String(index),
      start: index * 40,
    })),
    totalHeight: rows.length * 40,
  }),
}));

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

afterEach(cleanup);

describe('VirtualBody', () => {
  const rows = [
    [1, 'Alice'],
    [2, 'Bob'],
  ];

  it('renders virtual rows with row numbers', () => {
    const scrollEl = document.createElement('div');
    const { getByText, container } = render(
      <VirtualBody
        columns={COLS}
        rows={rows}
        rowHeight={40}
        editingCell={null}
        selectedRows={new Set()}
        scrollElement={scrollEl}
        onCellDoubleClick={vi.fn()}
        onCellEdit={vi.fn()}
        onCellEditCancel={vi.fn()}
        onRowSelect={vi.fn()}
      />,
    );
    expect(getByText('Alice')).toBeInTheDocument();
    expect(container.querySelector('button[title="dataTable.selectRow"]')).toHaveTextContent('1');
    expect(getByText('Alice')).toBeInTheDocument();
    expect(getByText('Bob')).toBeInTheDocument();
  });

  it('calls onRowSelect on row click with modifier keys', () => {
    const onRowSelect = vi.fn();
    const scrollEl = document.createElement('div');
    const { container } = render(
      <VirtualBody
        columns={COLS}
        rows={rows}
        rowHeight={40}
        editingCell={null}
        selectedRows={new Set()}
        scrollElement={scrollEl}
        onCellDoubleClick={vi.fn()}
        onCellEdit={vi.fn()}
        onCellEditCancel={vi.fn()}
        onRowSelect={onRowSelect}
      />,
    );
    const rowDiv = container.querySelector('[tabindex="0"]')!;
    fireEvent.click(rowDiv, { metaKey: true });
    expect(onRowSelect).toHaveBeenCalledWith(0, { multi: true, range: false });

    fireEvent.click(rowDiv, { shiftKey: true });
    expect(onRowSelect).toHaveBeenCalledWith(0, { multi: false, range: true });
  });

  it('handles keyboard row select and cell double-click', () => {
    const onRowSelect = vi.fn();
    const onCellDoubleClick = vi.fn();
    const scrollEl = document.createElement('div');
    const { container } = render(
      <VirtualBody
        columns={COLS}
        rows={rows}
        rowHeight={40}
        editingCell={null}
        selectedRows={new Set()}
        scrollElement={scrollEl}
        onCellDoubleClick={onCellDoubleClick}
        onCellEdit={vi.fn()}
        onCellEditCancel={vi.fn()}
        onRowSelect={onRowSelect}
      />,
    );
    const rowDiv = container.querySelector('[tabindex="0"]')!;
    fireEvent.keyDown(rowDiv, { key: 'Enter' });
    expect(onRowSelect).toHaveBeenCalledWith(0);

    fireEvent.doubleClick(rowDiv);
    expect(onCellDoubleClick).toHaveBeenCalledWith(0, 'id');
  });

  it('shows EditableCell when editing and commits edits', () => {
    const onCellEdit = vi.fn();
    const scrollEl = document.createElement('div');
    const { container } = render(
      <VirtualBody
        columns={COLS}
        rows={rows}
        rowHeight={40}
        editingCell={{ row: 0, col: 'name' }}
        selectedRows={new Set([0])}
        highlightedRow={0}
        scrollElement={scrollEl}
        onCellDoubleClick={vi.fn()}
        onCellEdit={onCellEdit}
        onCellEditCancel={vi.fn()}
        onRowSelect={vi.fn()}
      />,
    );
    const input = container.querySelector('input')!;
    expect(input).toBeInTheDocument();
    fireEvent.change(input, { target: { value: 'Carol' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCellEdit).toHaveBeenCalledWith(0, 'name', 'Carol');
  });

  it('row number button stops propagation', () => {
    const onRowSelect = vi.fn();
    const scrollEl = document.createElement('div');
    const { container } = render(
      <VirtualBody
        columns={COLS}
        rows={rows}
        rowHeight={40}
        editingCell={null}
        selectedRows={new Set()}
        scrollElement={scrollEl}
        onCellDoubleClick={vi.fn()}
        onCellEdit={vi.fn()}
        onCellEditCancel={vi.fn()}
        onRowSelect={onRowSelect}
      />,
    );
    const rowBtn = container.querySelector('button[type="button"]')!;
    fireEvent.click(rowBtn, { ctrlKey: true });
    expect(onRowSelect).toHaveBeenCalledWith(0, { multi: true, range: false });
  });
});
