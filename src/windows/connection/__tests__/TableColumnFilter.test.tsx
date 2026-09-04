import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TableColumnFilter } from '../TableColumnFilter';
import type { ColumnSchema } from '../../../types';

afterEach(cleanup);

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const mockColumns: ColumnSchema[] = [
  { name: 'id', dataType: 'int', isPrimaryKey: true },
  { name: 'username', dataType: 'varchar(50)', isPrimaryKey: false },
  { name: 'email', dataType: 'varchar(100)', isPrimaryKey: false },
  { name: 'created_at', dataType: 'timestamp', isPrimaryKey: false },
];

describe('TableColumnFilter', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders button with correct default state when all columns visible', () => {
    render(<TableColumnFilter columns={mockColumns} visibleColumns={null} onChange={vi.fn()} />);

    const toggleBtn = screen.getByTestId('table-column-filter-toggle');
    expect(toggleBtn).toBeInTheDocument();
    expect(screen.getByText('tableData.columnFilter')).toBeInTheDocument();
    expect(screen.queryByTestId('column-filter-count-badge')).not.toBeInTheDocument();
    expect(screen.queryByTestId('table-column-filter-popover')).not.toBeInTheDocument();
  });

  it('renders count badge when columns are filtered', () => {
    render(
      <TableColumnFilter
        columns={mockColumns}
        visibleColumns={['id', 'username']}
        onChange={vi.fn()}
      />,
    );

    const badge = screen.getByTestId('column-filter-count-badge');
    expect(badge).toHaveTextContent('2/4');
  });

  it('opens popover when clicking button and lists all columns', () => {
    render(<TableColumnFilter columns={mockColumns} visibleColumns={null} onChange={vi.fn()} />);

    fireEvent.click(screen.getByTestId('table-column-filter-toggle'));
    expect(screen.getByTestId('table-column-filter-popover')).toBeInTheDocument();
    expect(screen.getByText('tableData.visibleColumns')).toBeInTheDocument();

    // Check that all 4 column items are listed
    expect(screen.getByText('id')).toBeInTheDocument();
    expect(screen.getByText('username')).toBeInTheDocument();
    expect(screen.getByText('email')).toBeInTheDocument();
    expect(screen.getByText('created_at')).toBeInTheDocument();

    // PK badge for primary key
    expect(screen.getByText('PK')).toBeInTheDocument();
  });

  it('toggles column visibility when clicking checkbox', () => {
    const onChange = vi.fn();
    render(<TableColumnFilter columns={mockColumns} visibleColumns={null} onChange={onChange} />);

    fireEvent.click(screen.getByTestId('table-column-filter-toggle'));

    // Clicking 'email' checkbox to uncheck it (from all visible)
    const emailCheckbox = screen.getByTestId('column-filter-checkbox-email');
    expect(emailCheckbox).toBeChecked();

    fireEvent.click(emailCheckbox);
    expect(onChange).toHaveBeenCalledWith(['id', 'username', 'created_at']);
  });

  it('adds column back when checking an unchecked column', () => {
    const onChange = vi.fn();
    render(
      <TableColumnFilter
        columns={mockColumns}
        visibleColumns={['id', 'username']}
        onChange={onChange}
      />,
    );

    fireEvent.click(screen.getByTestId('table-column-filter-toggle'));

    const emailCheckbox = screen.getByTestId('column-filter-checkbox-email');
    expect(emailCheckbox).not.toBeChecked();

    fireEvent.click(emailCheckbox);
    expect(onChange).toHaveBeenCalledWith(['id', 'username', 'email']);
  });

  it('handles Select All action', () => {
    const onChange = vi.fn();
    render(<TableColumnFilter columns={mockColumns} visibleColumns={['id']} onChange={onChange} />);

    fireEvent.click(screen.getByTestId('table-column-filter-toggle'));
    fireEvent.click(screen.getByTestId('table-column-filter-select-all'));

    expect(onChange).toHaveBeenCalledWith(null);
  });

  it('handles Clear All action', () => {
    const onChange = vi.fn();
    render(<TableColumnFilter columns={mockColumns} visibleColumns={null} onChange={onChange} />);

    fireEvent.click(screen.getByTestId('table-column-filter-toggle'));
    fireEvent.click(screen.getByTestId('table-column-filter-clear-all'));

    expect(onChange).toHaveBeenCalledWith([]);
  });

  it('handles Reset action when filtered', () => {
    const onChange = vi.fn();
    render(<TableColumnFilter columns={mockColumns} visibleColumns={['id']} onChange={onChange} />);

    fireEvent.click(screen.getByTestId('table-column-filter-toggle'));
    const resetBtn = screen.getByTestId('table-column-filter-reset');
    fireEvent.click(resetBtn);

    expect(onChange).toHaveBeenCalledWith(null);
  });

  it('filters columns by search query', () => {
    render(<TableColumnFilter columns={mockColumns} visibleColumns={null} onChange={vi.fn()} />);

    fireEvent.click(screen.getByTestId('table-column-filter-toggle'));
    const searchInput = screen.getByTestId('table-column-filter-search');

    fireEvent.change(searchInput, { target: { value: 'user' } });
    expect(screen.getByText('username')).toBeInTheDocument();
    expect(screen.queryByText('email')).not.toBeInTheDocument();

    fireEvent.change(searchInput, { target: { value: 'nonexistent' } });
    expect(screen.getByText('tableData.noColumnsFound')).toBeInTheDocument();
  });

  it('closes popover on Escape key', () => {
    render(<TableColumnFilter columns={mockColumns} visibleColumns={null} onChange={vi.fn()} />);

    fireEvent.click(screen.getByTestId('table-column-filter-toggle'));
    expect(screen.getByTestId('table-column-filter-popover')).toBeInTheDocument();

    fireEvent.keyDown(screen.getByTestId('table-column-filter-popover'), { key: 'Escape' });
    expect(screen.queryByTestId('table-column-filter-popover')).not.toBeInTheDocument();
  });

  it('closes popover when clicking outside', () => {
    render(
      <div>
        <div data-testid="outside-element">Outside</div>
        <TableColumnFilter columns={mockColumns} visibleColumns={null} onChange={vi.fn()} />
      </div>,
    );

    fireEvent.click(screen.getByTestId('table-column-filter-toggle'));
    expect(screen.getByTestId('table-column-filter-popover')).toBeInTheDocument();

    fireEvent.mouseDown(screen.getByTestId('outside-element'));
    expect(screen.queryByTestId('table-column-filter-popover')).not.toBeInTheDocument();
  });
});
