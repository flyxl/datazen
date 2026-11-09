import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, waitFor, cleanup, screen } from '@testing-library/react';
import { DataExportDialog } from '../DataExportDialog';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
  }: {
    value: string | number;
    options: { value: string; label: string; disabled?: boolean }[];
    onChange: (v: string) => void;
  }) => (
    <select value={String(value)} onChange={(e) => onChange(e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value} disabled={o.disabled}>
          {o.label}
        </option>
      ))}
    </select>
  ),
}));

const exportTablesStream = vi.fn();

import { configurePlatform, clearPlatform } from '../../../platform/contracts';

function bindFakeExporter(): void {
  configurePlatform({
    services: {} as never,
    windows: {} as never,
    events: {
      emitCrossWindow: async () => {},
      listenCrossWindow: async () => () => {},
    },
    tablesExporter: {
      exportTables: (...args: unknown[]) => exportTablesStream(...(args as [never])),
    },
  });
}

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

afterEach(() => {
  cleanup();
  clearPlatform();
});

beforeEach(() => {
  vi.clearAllMocks();
  bindFakeExporter();
  exportTablesStream.mockResolvedValue({ Saved: 2 });
});

describe('DataExportDialog', () => {
  it('renders format and range options', () => {
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[
          [1, 'a'],
          [2, 'b'],
        ]}
        tableName="users"
      />,
    );
    expect(getByText('common.exportData')).toBeInTheDocument();
    expect(getByText('export.format')).toBeInTheDocument();
    expect(getByText('export.range')).toBeInTheDocument();
  });

  it('offers entire-table scope when dbSessionId is set', () => {
    render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        totalRows={100}
      />,
    );
    const selects = document.querySelectorAll('select');
    const range = selects[1]!;
    expect(Array.from(range.options).map((o) => o.value)).toContain('entire_table');
  });

  it('shows success state and close button on successful export', async () => {
    const onClose = vi.fn();
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={onClose}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
      />,
    );
    fireEvent.click(getByText('export.export'));
    await waitFor(() => {
      expect(exportTablesStream).toHaveBeenCalled();
      expect(getByText('export.success')).toBeInTheDocument();
    });
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(getByText('common.close'));
    expect(onClose).toHaveBeenCalled();
  });

  it('exports current page as a provided source (item 3c)', async () => {
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[
          [1, 'a'],
          [2, 'b'],
        ]}
        tableName="users"
        dbSessionId="c1"
      />,
    );
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
    const request = exportTablesStream.mock.calls[0]![0] as {
      dataFormat: string;
      tables: { tableName: string; columns: string[]; source: unknown }[];
    };
    expect(request.dataFormat).toBe('csv');
    expect(request.tables).toEqual([
      {
        tableName: 'users',
        columns: ['id', 'name'],
        source: {
          type: 'provided',
          columns: [
            { name: 'id', dataType: 'integer', nullable: true },
            { name: 'name', dataType: 'varchar', nullable: true },
          ],
          rows: [
            [1, 'a'],
            [2, 'b'],
          ],
        },
      },
    ]);
    await waitFor(() => expect(getByText('export.success')).toBeInTheDocument());
  });

  it('exports selected rows only, as a provided source (item 3c)', async () => {
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[
          [1, 'a'],
          [2, 'b'],
          [3, 'c'],
        ]}
        selectedRows={new Set([2])}
        tableName="users"
        dbSessionId="c1"
        initialScope="selected"
      />,
    );
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
    const request = exportTablesStream.mock.calls[0]![0] as {
      tables: { source: { type: string; rows: unknown[][] } }[];
    };
    expect(request.tables[0]!.source.type).toBe('provided');
    expect(request.tables[0]!.source.rows).toEqual([[3, 'c']]);
  });

  it('offers sql_update in the format list (item 3d)', () => {
    render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
      />,
    );
    const selects = document.querySelectorAll('select');
    const formatOptions = Array.from(selects[0]!.options).map((o) => o.value);
    expect(formatOptions).toEqual([
      'csv',
      'tsv',
      'json',
      'markdown',
      'xlsx',
      'sql_insert',
      'sql_update',
    ]);
  });

  it('exports xlsx through the Rust exporter, not the old binary path', async () => {
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
      />,
    );
    const selects = document.querySelectorAll('select');
    fireEvent.change(selects[0], { target: { value: 'xlsx' } });
    fireEvent.click(getByText('export.export'));
    await waitFor(() => {
      expect(exportTablesStream).toHaveBeenCalledTimes(1);
    });
    const request = exportTablesStream.mock.calls[0]![0] as {
      dataFormat: string;
      tables: { source: { type: string } }[];
    };
    expect(request.dataFormat).toBe('xlsx');
    expect(request.tables[0]!.source.type).toBe('provided');
  });

  it('shows error when export fails', async () => {
    exportTablesStream.mockRejectedValue(new Error('disk full'));
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        dbSessionId="c1"
      />,
    );
    fireEvent.click(getByText('export.export'));
    await waitFor(() => {
      expect(getByText('disk full')).toBeInTheDocument();
    });
  });

  it('does not close when user cancels save dialog', async () => {
    exportTablesStream.mockResolvedValue({ Cancelled: null });
    const onClose = vi.fn();
    const { getByText } = render(
      <DataExportDialog open onClose={onClose} columns={COLS} rows={[[1, 'a']]} dbSessionId="c1" />,
    );
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(exportTablesStream).toHaveBeenCalled());
    expect(onClose).not.toHaveBeenCalled();
  });

  it('switches to selected scope', () => {
    const selected = new Set([0]);
    render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[
          [1, 'a'],
          [2, 'b'],
        ]}
        selectedRows={selected}
      />,
    );
    const selects = document.querySelectorAll('select');
    fireEvent.change(selects[1], { target: { value: 'selected' } });
    expect(selects[1]).toHaveValue('selected');
  });

  it('calls onClose from cancel button', () => {
    const onClose = vi.fn();
    const { getByText } = render(
      <DataExportDialog open onClose={onClose} columns={COLS} rows={[]} />,
    );
    fireEvent.click(getByText('common.cancel'));
    expect(onClose).toHaveBeenCalled();
  });
});

describe('DataExportDialog export capability', () => {
  it('hides entire-table scope when capability is loaded_only', () => {
    render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        totalRows={100}
        dataExportCapability="loaded_only"
      />,
    );
    const selects = document.querySelectorAll('select');
    const range = selects[1]!;
    const values = Array.from(range.options).map((o) => o.value);
    expect(values).not.toContain('entire_table');
    expect(values).toContain('current_page');
  });

  it('shows a warning and disables export button when capability is none', () => {
    const { getByText, getByRole } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        dataExportCapability="none"
      />,
    );
    expect(getByText('export.disabledByDriver')).toBeInTheDocument();
    expect(getByRole('button', { name: 'export.export' })).toBeDisabled();
  });

  it('does not attempt export when capability is none', async () => {
    render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dataExportCapability="none"
      />,
    );
    // Button is disabled, so firing click is a no-op for the handler guard.
    const btn = document.querySelectorAll('button')[1] as HTMLButtonElement;
    fireEvent.click(btn);
    expect(exportTablesStream).not.toHaveBeenCalled();
  });
});

describe('DataExportDialog filtered scope (DB-15)', () => {
  it('offers the filtered scope exactly when its statement builder is threaded', () => {
    const { rerender } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
      />,
    );
    let range = document.querySelectorAll('select')[1]!;
    expect(Array.from(range.options).map((o) => o.value)).not.toContain('filtered_table');

    rerender(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        onBuildFilteredExportSql={vi.fn().mockResolvedValue('SELECT 1;')}
      />,
    );
    range = document.querySelectorAll('select')[1]!;
    expect(Array.from(range.options).map((o) => o.value)).toContain('filtered_table');
  });

  it('opens with the filtered scope preselected and routes its statement to Rust', async () => {
    const filteredSql = 'SELECT "id" FROM "users" WHERE "status" = \'active\';';
    const onBuildFilteredExportSql = vi.fn().mockResolvedValue(filteredSql);
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        initialScope="filtered_table"
        onBuildFilteredExportSql={onBuildFilteredExportSql}
      />,
    );
    const range = document.querySelectorAll('select')[1]!;
    expect(range).toHaveValue('filtered_table');

    fireEvent.click(screen.getByText('export.export'));

    await waitFor(() => {
      expect(onBuildFilteredExportSql).toHaveBeenCalledOnce();
      expect(exportTablesStream).toHaveBeenCalledTimes(1);
    });
    // The streamed statement is the backend's own filtered read, not a
    // SELECT the exporter built on its own — carried as a SQL source.
    const request = exportTablesStream.mock.calls[0]![0] as {
      tables: { source: unknown }[];
    };
    expect(request.tables[0]!.source).toEqual({ type: 'sql', sql: filteredSql });
    await waitFor(() => expect(getByText('export.success')).toBeInTheDocument());
  });

  it('falls back to the page scope when the preferred filtered scope cannot be served', () => {
    render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        initialScope="filtered_table"
      />,
    );
    // No dbSessionId and no builder: the scope must not point at a missing option.
    const range = document.querySelectorAll('select')[1]!;
    expect(range).toHaveValue('current_page');
  });
});

describe('DataExportDialog entire-table scope (item 3b)', () => {
  it('offers xlsx for entire tables and routes them to Rust as a table source', async () => {
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        totalRows={100}
        initialScope="entire_table"
      />,
    );
    const selects = document.querySelectorAll('select');
    const formatOptions = Array.from(selects[0]!.options).map((o) => o.value);
    expect(formatOptions).toContain('xlsx');
    fireEvent.change(selects[0]!, { target: { value: 'xlsx' } });
    fireEvent.change(selects[1]!, { target: { value: 'entire_table' } });
    fireEvent.click(getByText('export.export'));

    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
    const request = exportTablesStream.mock.calls[0]![0] as {
      mode: string;
      dataFormat: string;
      outputMode: string;
      tables: { tableName: string; source: unknown }[];
    };
    expect(request.mode).toBe('data_only');
    expect(request.dataFormat).toBe('xlsx');
    expect(request.outputMode).toBe('single');
    expect(request.tables).toEqual([
      { tableName: 'users', columns: ['id', 'name'], source: { type: 'table' } },
    ]);
    await waitFor(() => expect(getByText('export.success')).toBeInTheDocument());
  });

  it('returns to the form when the Rust export is cancelled', async () => {
    exportTablesStream.mockResolvedValue({ Cancelled: null });
    const { getByText, queryByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        totalRows={100}
        initialScope="entire_table"
      />,
    );
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
    expect(queryByText('export.success')).not.toBeInTheDocument();
  });
});
