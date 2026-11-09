import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, screen, fireEvent } from '@testing-library/react';
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

describe('ExportDialog unified formats and column picker (DBR-06)', () => {
  /** The format `<Select>` is the first combobox rendered. */
  function formatSelect(): HTMLSelectElement {
    const combos = screen.getAllByRole('combobox');
    return combos[0] as HTMLSelectElement;
  }

  it('offers the same format list as the grid dialog, TSV included', () => {
    renderDialog();
    const values = Array.from(formatSelect().options).map((o) => o.value);
    // 格式统一: the table-level dialog previously lacked TSV.
    expect(values).toEqual(['csv', 'tsv', 'json', 'markdown', 'xlsx', 'sql_insert', 'sql_update']);
  });

  it('disables xlsx with its reason for the entire-table scope', () => {
    renderDialog();
    const xlsx = Array.from(formatSelect().options).find((o) => o.value === 'xlsx')!;
    expect(xlsx.disabled).toBe(false);
    fireEvent.change(scopeSelect(), { target: { value: 'entire_table' } });
    expect(Array.from(formatSelect().options).find((o) => o.value === 'xlsx')!.disabled).toBe(true);
    // 禁用并解释原因 — the reason is rendered next to the format list.
    expect(screen.getByText('export.formatDisabledStream')).toBeInTheDocument();
  });

  it('keeps sql_update enabled for a keyed table', () => {
    renderDialog();
    const sqlUpdate = Array.from(formatSelect().options).find((o) => o.value === 'sql_update')!;
    // COLS declares `id` as a primary key, so the WHERE has a real column.
    expect(sqlUpdate.disabled).toBe(false);
  });

  it('shows the shared picker with the PK badge and selects every column by default', () => {
    renderDialog();
    const idCheckbox = document.querySelector(
      '[data-testid="export-column-id"]',
    ) as HTMLInputElement;
    expect(idCheckbox.checked).toBe(true);
    // The key column is marked, so the SQL UPDATE choice explains itself.
    expect(screen.getByText('PK')).toBeInTheDocument();
  });

  it('clear-all disables the export button and reset restores it', () => {
    renderDialog();
    fireEvent.click(screen.getByTestId('export-columns-clear-all'));
    expect(screen.getByRole('button', { name: 'export.export' })).toBeDisabled();
    fireEvent.click(screen.getByTestId('export-columns-reset-default'));
    expect(screen.getByRole('button', { name: 'export.export' })).toBeEnabled();
  });
});
