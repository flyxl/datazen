import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, screen, fireEvent, waitFor } from '@testing-library/react';
import { ExportDialog } from '../ExportDialog';
import { getCachedTableSchema } from '../../../lib/schemaCache';
import type { ColumnSchema } from '../../../types';

// Item 3b: the entire-table scope rides the tables-exporter port.
const exportTablesStream = vi.fn();

// Task 3: the loaded scopes ride ProvidedDataFetcher, which Blob-downloads
// on this (non-desktop) test build instead of touching the Rust exporter.
const downloadBlobMock = vi.hoisted(() => vi.fn());
vi.mock('../../../platform/blobTablesExporter', async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('../../../platform/blobTablesExporter')>();
  return { ...actual, downloadBlob: (...args: unknown[]) => downloadBlobMock(...args) };
});

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

afterEach(() => {
  cleanup();
  clearPlatform();
});

beforeEach(() => {
  vi.clearAllMocks();
  bindFakeExporter();
  exportTablesStream.mockResolvedValue({ Saved: 2 });
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

describe('ExportDialog loaded scope (task 3: ProvidedDataFetcher)', () => {
  const COLS2: ColumnSchema[] = [
    { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true, isAutoIncrement: false },
    { name: 'name', dataType: 'text', nullable: true, isPrimaryKey: false, isAutoIncrement: false },
  ];

  it('exports the current page via the frontend formatter, never the Rust exporter', async () => {
    renderDialog({
      columns: COLS2,
      rows: [
        { id: 1, name: 'a' },
        { id: 2, name: 'b' },
      ],
      defaultScope: 'current_page',
    });
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));

    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id,name\n1,a\n2,b',
      'users.csv',
      'text/csv',
    );
    expect(exportTablesStream).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText('export.success')).toBeInTheDocument());
  });

  it('exports only the selected rows, never a SQL translation', async () => {
    renderDialog({
      columns: COLS2,
      rows: [
        { id: 1, name: 'a' },
        { id: 2, name: 'b' },
        { id: 3, name: 'c' },
      ],
      selectedRows: new Set([2]),
      defaultScope: 'selected',
    });
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));

    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id,name\n3,c',
      'users.csv',
      'text/csv',
    );
    expect(exportTablesStream).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText('export.success')).toBeInTheDocument());
  });
});

describe('ExportDialog entire-table scope (item 3b)', () => {
  it('offers xlsx for entire tables and routes them to Rust as a table source', async () => {
    renderDialog({ defaultScope: 'entire_table' });
    const combos = screen.getAllByRole('combobox');
    const formatSelect = combos[0] as HTMLSelectElement;
    expect(Array.from(formatSelect.options).map((o) => o.value)).toContain('xlsx');
    fireEvent.change(formatSelect, { target: { value: 'xlsx' } });

    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));

    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
    const request = exportTablesStream.mock.calls[0]![0] as {
      mode: string;
      dataFormat: string;
      outputMode: string;
      tables: { tableName: string; columns: string[]; source: unknown; primaryKey: unknown }[];
    };
    expect(request.mode).toBe('data_only');
    expect(request.dataFormat).toBe('xlsx');
    expect(request.outputMode).toBe('single');
    expect(request.tables[0]!.tableName).toBe('users');
    expect(request.tables[0]!.columns).toEqual(['id']);
    expect(request.tables[0]!.source).toEqual({ type: 'table' });
    expect(request.tables[0]!.primaryKey).toBe('id');
    await waitFor(() => expect(screen.getByText('export.success')).toBeInTheDocument());
  });

  it('returns to the form when the Rust export is cancelled', async () => {
    exportTablesStream.mockResolvedValue({ Cancelled: null });
    renderDialog({ defaultScope: 'entire_table' });
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));
    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
    expect(screen.queryByText('export.success')).not.toBeInTheDocument();
  });
});

describe('ExportDialog column selection, schema prefetch and session guards', () => {
  const PREFETCHED: ColumnSchema[] = [
    { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true, isAutoIncrement: false },
    { name: 'name', dataType: 'text', nullable: true, isPrimaryKey: false, isAutoIncrement: false },
  ];

  it('toggles one column off and on again via its checkbox', async () => {
    renderDialog({
      columns: PREFETCHED,
      rows: [
        { id: 1, name: 'a' },
        { id: 2, name: 'b' },
      ],
    });
    const boxes = screen.getAllByRole('checkbox');
    expect(boxes).toHaveLength(2);
    fireEvent.click(boxes[1]!);
    expect(screen.getByRole('button', { name: 'common.selectAll' })).toBeInTheDocument();
    fireEvent.click(boxes[1]!);
    expect(screen.getByRole('button', { name: 'common.deselectAll' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id,name\n1,a\n2,b',
      'users.csv',
      'text/csv',
    );
  });

  it('exports only the checked columns', async () => {
    renderDialog({ columns: PREFETCHED });
    fireEvent.click(screen.getAllByRole('checkbox')[1]!);
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith('id\n1\n2', 'users.csv', 'text/csv');
    expect(exportTablesStream).not.toHaveBeenCalled();
  });

  it('deselect-all disables export, select-all re-enables it', () => {
    renderDialog({ columns: PREFETCHED });
    const exportBtn = screen.getByRole('button', { name: 'export.export' });
    expect(exportBtn).not.toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'common.deselectAll' }));
    expect(exportBtn).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'common.selectAll' }));
    expect(exportBtn).not.toBeDisabled();
  });

  it('prefetches the table schema when a database is bound', async () => {
    vi.mocked(getCachedTableSchema).mockResolvedValueOnce({
      tableName: 'users',
      columns: PREFETCHED,
      primaryKeys: ['id'],
      indexes: [],
      foreignKeys: [],
    });
    renderDialog({ columns: [PREFETCHED[0]!], database: 'appdb' });
    await waitFor(() => expect(screen.getByText('name')).toBeInTheDocument());
    expect(getCachedTableSchema).toHaveBeenCalledTimes(1);
    expect(vi.mocked(getCachedTableSchema).mock.calls[0]!.slice(0, 3)).toEqual([
      'c1',
      'users',
      'appdb',
    ]);
  });

  it('shows Missing connection for entire_table without a session', async () => {
    renderDialog({ dbSessionId: undefined, defaultScope: 'entire_table' });
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));
    await waitFor(() => expect(screen.getByText('Missing connection')).toBeInTheDocument());
    expect(exportTablesStream).not.toHaveBeenCalled();
  });

  it('exports loaded rows without a session (task 4: provided needs no session)', async () => {
    renderDialog({ dbSessionId: undefined });
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id\n1\n2',
      'users.csv',
      'text/csv',
    );
    expect(exportTablesStream).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText('export.success')).toBeInTheDocument());
  });

  it('exports selected rows without a session', async () => {
    renderDialog({
      dbSessionId: undefined,
      rows: [
        { id: 1 },
        { id: 2 },
        { id: 3 },
      ],
      selectedRows: new Set([2]),
      defaultScope: 'selected',
    });
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith('id\n3', 'users.csv', 'text/csv');
    expect(exportTablesStream).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText('export.success')).toBeInTheDocument());
  });

  it('hides the entire_table scope without a session', () => {
    renderDialog({ dbSessionId: undefined });
    const values = Array.from(scopeSelect().options).map((o) => o.value);
    expect(values).toContain('current_page');
    expect(values).not.toContain('entire_table');
  });

  it('changes scope through the range select', async () => {
    renderDialog();
    fireEvent.change(scopeSelect(), { target: { value: 'entire_table' } });
    expect(scopeSelect()).toHaveValue('entire_table');
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));
    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByText('export.success')).toBeInTheDocument());
  });

  it('does not prefetch the schema when closed', () => {
    render(
      <ExportDialog
        open={false}
        onClose={vi.fn()}
        tableName="users"
        columns={COLS}
        rows={[{ id: 1 }, { id: 2 }]}
        selectedRows={new Set()}
        dbSessionId="c1"
        database="appdb"
      />,
    );
    expect(getCachedTableSchema).not.toHaveBeenCalled();
  });
});
