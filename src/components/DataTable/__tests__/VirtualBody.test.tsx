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
    const rowDiv = container.querySelector('[data-dt-row-view]')!;
    fireEvent.click(rowDiv, { metaKey: true });
    expect(onRowSelect).toHaveBeenCalledWith(0, { multi: true, range: false });

    fireEvent.click(rowDiv, { shiftKey: true });
    expect(onRowSelect).toHaveBeenCalledWith(0, { multi: false, range: true });
  });

  it('handles the cell double-click', () => {
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
    const rowDiv = container.querySelector('[data-dt-row-view]')!;
    fireEvent.doubleClick(rowDiv);
    expect(onCellDoubleClick).toHaveBeenCalledWith(0, 'id');
  });

  it('keeps the row out of the tab order (the scroll container is the tab stop)', () => {
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
        onRowSelect={vi.fn()}
      />,
    );
    expect(container.querySelectorAll('[data-dt-row-view]')).toHaveLength(rows.length);
    expect(container.querySelectorAll('[data-dt-row-view][tabindex="0"]')).toHaveLength(0);
    expect(container.querySelectorAll('[data-dt-row-view][tabindex="-1"]')).toHaveLength(rows.length);
  });

  it('calls onRowSelect exactly once when the row number button is pressed with Enter', () => {
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
    // The browser turns `Enter` on a focused button into a click. The keydown
    // must not reach a row handler of its own: the row's `Enter`/`Space`
    // branch is retired (DB-02 §4.4), so the row would be selected twice.
    fireEvent.keyDown(rowBtn, { key: 'Enter' });
    expect(onRowSelect).not.toHaveBeenCalled();

    fireEvent.click(rowBtn);
    expect(onRowSelect).toHaveBeenCalledTimes(1);
    expect(onRowSelect).toHaveBeenCalledWith(0, { multi: false, range: false });

    fireEvent.keyDown(rowBtn, { key: ' ' });
    expect(onRowSelect).toHaveBeenCalledTimes(1);
  });

  it('marks the selected row with data-dt-selected', () => {
    const scrollEl = document.createElement('div');
    const { container } = render(
      <VirtualBody
        columns={COLS}
        rows={rows}
        rowHeight={40}
        editingCell={null}
        selectedRows={new Set([1])}
        scrollElement={scrollEl}
        onCellDoubleClick={vi.fn()}
        onCellEdit={vi.fn()}
        onCellEditCancel={vi.fn()}
        onRowSelect={vi.fn()}
      />,
    );
    const selected = container.querySelector('[data-dt-row-view="1"]');
    const other = container.querySelector('[data-dt-row-view="0"]');
    expect(selected?.getAttribute('data-dt-selected')).toBe('true');
    expect(other?.hasAttribute('data-dt-selected')).toBe(false);
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
