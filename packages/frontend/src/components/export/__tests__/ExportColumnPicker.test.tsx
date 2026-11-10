import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, screen, cleanup } from '@testing-library/react';
import { ExportColumnPicker } from '../ExportColumnPicker';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(() => cleanup());

const COLUMNS = [
  { name: 'id', type: 'integer', isPrimaryKey: true },
  { name: 'name', type: 'varchar' },
  { name: 'note', type: 'text' },
];

function renderPicker(selected = new Set(['id', 'name', 'note']), overrides = {}) {
  const handlers = {
    onToggle: vi.fn(),
    onSelectAll: vi.fn(),
    onClearAll: vi.fn(),
    onResetDefault: vi.fn(),
    ...overrides,
  };
  render(<ExportColumnPicker columns={COLUMNS} selected={selected} {...handlers} />);
  return handlers;
}

describe('ExportColumnPicker (DBR-06, restored)', () => {
  it('renders the columns in display order, with type labels and a PK badge', () => {
    renderPicker();
    const list = screen.getByTestId('export-columns-list');
    const labels = Array.from(list.querySelectorAll('label')).map(
      (label) => label.textContent,
    );
    // The spans sit adjacent in JSX, so the text runs together — the
    // order and the content are what this pins, not the spacing.
    expect(labels).toEqual(['idPKinteger', 'namevarchar', 'notetext']);
    expect(screen.getByText('PK')).toBeInTheDocument();
  });

  it('checks exactly the selected columns', () => {
    renderPicker(new Set(['name']));
    expect(screen.getByTestId('export-column-id')).not.toBeChecked();
    expect(screen.getByTestId('export-column-name')).toBeChecked();
    expect(screen.getByTestId('export-column-note')).not.toBeChecked();
  });

  it('toggles a column through its checkbox', () => {
    const { onToggle } = renderPicker();
    fireEvent.click(screen.getByTestId('export-column-name'));
    expect(onToggle).toHaveBeenCalledTimes(1);
    expect(onToggle).toHaveBeenCalledWith('name');
  });

  it('offers select-all, clear-all and reset-to-default', () => {
    const { onSelectAll, onClearAll, onResetDefault } = renderPicker(new Set(['id']));
    fireEvent.click(screen.getByTestId('export-columns-select-all'));
    expect(onSelectAll).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId('export-columns-clear-all'));
    expect(onClearAll).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId('export-columns-reset-default'));
    expect(onResetDefault).toHaveBeenCalledTimes(1);
  });

  it('labels the actions with the shared i18n keys', () => {
    renderPicker();
    expect(screen.getByTestId('export-columns-select-all')).toHaveTextContent(
      'common.selectAll',
    );
    expect(screen.getByTestId('export-columns-clear-all')).toHaveTextContent(
      'common.deselectAll',
    );
    expect(screen.getByTestId('export-columns-reset-default')).toHaveTextContent(
      'export.resetColumns',
    );
    expect(screen.getByText('export.selectColumns')).toBeInTheDocument();
  });
});
