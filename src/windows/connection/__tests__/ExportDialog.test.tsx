import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { ExportDialog } from '../ExportDialog';
import type { ColumnSchema } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/schemaCache', () => ({
  getCachedTableSchema: vi.fn().mockResolvedValue({ columns: [] }),
}));

vi.mock('../../../components/ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
    disabled,
  }: {
    value: string | number;
    options: { value: string; label: string; disabled?: boolean }[];
    onChange: (v: string) => void;
    disabled?: boolean;
  }) => (
    <select
      data-testid="export-scope"
      value={String(value)}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value} disabled={o.disabled}>
          {o.label}
        </option>
      ))}
    </select>
  ),
}));

const COLS: ColumnSchema[] = [
  { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true, isAutoIncrement: false },
];

function renderDialog(overrides: Partial<Parameters<typeof ExportDialog>[0]> = {}) {
  return render(
    <ExportDialog
      open
      onClose={vi.fn()}
      tableName="users"
      columns={COLS}
      rows={[{ id: 1 }, { id: 2 }]}
      selectedRows={new Set()}
      dbSessionId="c1"
      {...overrides}
    />,
  );
}

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
});

/** The scope `<Select>` is the second combobox rendered (format first, scope second). */
function scopeSelect(): HTMLSelectElement {
  const combos = screen.getAllByRole('combobox');
  return combos[combos.length - 1] as HTMLSelectElement;
}

describe('ExportDialog export capability', () => {
  it('offers entire_table scope by default (full_table)', () => {
    renderDialog();
    const values = Array.from(scopeSelect().options).map((o) => o.value);
    expect(values).toContain('entire_table');
    expect(values).toContain('current_page');
  });

  it('hides entire_table scope when capability is loaded_only', () => {
    renderDialog({ dataExportCapability: 'loaded_only' });
    const values = Array.from(scopeSelect().options).map((o) => o.value);
    expect(values).not.toContain('entire_table');
  });

  it('shows a warning and disables export button when capability is none', () => {
    renderDialog({ dataExportCapability: 'none' });
    expect(screen.getByText('export.disabledByDriver')).toBeInTheDocument();
    const exportBtn = screen.getByRole('button', { name: 'export.export' });
    expect(exportBtn).toBeDisabled();
    const values = Array.from(scopeSelect().options).map((o) => o.value);
    expect(values).not.toContain('entire_table');
  });
});
