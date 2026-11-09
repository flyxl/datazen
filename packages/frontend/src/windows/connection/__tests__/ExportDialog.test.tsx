import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, screen, fireEvent, waitFor } from '@testing-library/react';
import { ExportDialog } from '../ExportDialog';
import { getCachedTableSchema } from '../../../lib/schemaCache';
import type { ColumnSchema } from '../../../types';

// Item 3b: the entire-table scope rides the tables-exporter port.
const exportTablesStream = vi.fn();
const cancelExportTablesMock = vi.fn();

// Task 3: the loaded scopes ride ProvidedDataFetcher, which Blob-downloads
// on this (non-desktop) test build instead of touching the Rust exporter.
const downloadBlobMock = vi.hoisted(() => vi.fn());
vi.mock('../../../platform/blobTablesExporter', async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('../../../platform/blobTablesExporter')>();
  return { ...actual, downloadBlob: (...args: unknown[]) => downloadBlobMock(...args) };
});

// Task 6: `IS_DESKTOP_BUILD` flips through the getter mock (the pattern in
// `lib/__tests__/dataFetcher.test.ts`). Web is the default here — the
// provided-scope tests pin the Blob download — and the desktop test below
// flips it to true for the one Rust-routed entire-table case.
const envState = vi.hoisted(() => ({ desktop: false }));
vi.mock('../../../platform/env', () => ({
  get IS_DESKTOP_BUILD() {
    return envState.desktop;
  },
}));

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
      cancelExportTables: (...args: unknown[]) =>
        cancelExportTablesMock(...(args as [never])),
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
  envState.desktop = false;
  bindFakeExporter();
  exportTablesStream.mockResolvedValue({ Saved: 2 });
  cancelExportTablesMock.mockResolvedValue(true);
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
  it('offers xlsx for entire tables on desktop and routes them to Rust as a table source', async () => {
    envState.desktop = true;
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

describe('ExportDialog cancellation (item 1b)', () => {
  it('cancels mid-flight and reports the partial file deleted', async () => {
    envState.desktop = true;
    let resolveExport!: (result: unknown) => void;
    exportTablesStream.mockImplementation(
      () => new Promise((resolve) => { resolveExport = resolve; }),
    );
    renderDialog({ defaultScope: 'entire_table' });
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));
    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));

    const cancelButton = screen.getByRole('button', { name: 'export.cancelExport' });
    expect(cancelButton).toBeEnabled();
    fireEvent.click(cancelButton);
    expect(screen.getByText('export.cancelling')).toBeInTheDocument();
    expect(screen.getByText('export.cancelling')).toBeDisabled();
    const request = exportTablesStream.mock.calls[0]![0] as {
      cancelToken?: string;
    };
    expect(cancelExportTablesMock).toHaveBeenCalledTimes(1);
    expect(cancelExportTablesMock).toHaveBeenCalledWith(request.cancelToken);

    resolveExport({ Aborted: { partialDeleted: true } });
    await waitFor(() =>
      expect(screen.getByTestId('export-aborted-panel')).toBeInTheDocument(),
    );
    expect(screen.getByText('export.cancelledPartialDeleted')).toBeInTheDocument();
    expect(screen.queryByText('export.success')).not.toBeInTheDocument();
    expect(screen.getByText('common.close')).toBeInTheDocument();
  });

  it('offers no cancel button for the provided (current-page) scope', () => {
    envState.desktop = true;
    renderDialog({ defaultScope: 'current_page' });
    expect(
      screen.queryByRole('button', { name: 'export.cancelExport' }),
    ).not.toBeInTheDocument();
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

  it('falls back to the current page instead of Missing connection for entire_table without a session', async () => {
    // Task 3: the scope correction drops an entire_table default the
    // driver cannot serve, so the dialog can no longer sit on a scope its
    // own option list does not offer — the export below is a provided
    // current-page export, not a Missing connection error.
    renderDialog({ dbSessionId: undefined, defaultScope: 'entire_table' });
    expect(scopeSelect()).toHaveValue('current_page');
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id\n1\n2',
      'users.csv',
      'text/csv',
    );
    expect(screen.queryByText('Missing connection')).not.toBeInTheDocument();
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

describe('ExportDialog web format supply (task 6, plan §2.8)', () => {
  /** Format is the first combobox, scope the last (mock renders both). */
  function formatSelect(): HTMLSelectElement {
    return screen.getAllByRole('combobox')[0] as HTMLSelectElement;
  }
  function formatValues(): string[] {
    return Array.from(formatSelect().options).map((o) => o.value);
  }

  it('offers every format for the provided current-page scope on web', () => {
    renderDialog();
    expect(formatValues()).toEqual([
      'csv',
      'json',
      'markdown',
      'xlsx',
      'sql_insert',
      'sql_update',
    ]);
  });

  it('disables every non-CSV format with its reason for the entire-table scope on web', () => {
    renderDialog({ defaultScope: 'entire_table' });
    expect(scopeSelect()).toHaveValue('entire_table');
    expect(formatValues()).toEqual([
      'csv',
      'json',
      'markdown',
      'xlsx',
      'sql_insert',
      'sql_update',
    ]);
    const disabled = new Map(
      Array.from(formatSelect().options).map((o) => [o.value, o.disabled]),
    );
    expect(disabled.get('csv')).toBe(false);
    for (const value of ['json', 'markdown', 'xlsx', 'sql_insert', 'sql_update']) {
      expect(disabled.get(value)).toBe(true);
    }
    // The fixture column carries a primary key, so only the web reason shows.
    expect(
      screen.getAllByTestId('export-format-disabled-reason').map((el) => el.textContent),
    ).toEqual(['export.formatDisabledWebSqlScope']);
  });

  it('resets a non-CSV format to CSV when switching to the entire-table scope on web', () => {
    renderDialog();
    fireEvent.change(formatSelect(), { target: { value: 'xlsx' } });
    expect(formatSelect()).toHaveValue('xlsx');
    fireEvent.change(scopeSelect(), { target: { value: 'entire_table' } });
    expect(scopeSelect()).toHaveValue('entire_table');
    expect(formatSelect()).toHaveValue('csv');
    // The disabled formats stay listed (greyed out) instead of vanishing.
    expect(formatValues()).toEqual([
      'csv',
      'json',
      'markdown',
      'xlsx',
      'sql_insert',
      'sql_update',
    ]);
    expect(
      Array.from(formatSelect().options).find((o) => o.value === 'xlsx')?.disabled,
    ).toBe(true);
  });

  it('exports the current page as JSON on web via the frontend formatter', async () => {
    renderDialog();
    fireEvent.change(formatSelect(), { target: { value: 'json' } });
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));

    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    const [contents, fileName, mimeType] = downloadBlobMock.mock.calls[0] as [
      string,
      string,
      string,
    ];
    expect(JSON.parse(contents)).toEqual([{ id: 1 }, { id: 2 }]);
    expect(fileName).toBe('users.json');
    expect(mimeType).toBe('application/json');
    expect(exportTablesStream).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText('export.success')).toBeInTheDocument());
  });
});

describe('ExportDialog shared column picker (item 1a)', () => {
  const COLS2: ColumnSchema[] = [
    { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true, isAutoIncrement: false },
    { name: 'name', dataType: 'text', nullable: true, isPrimaryKey: false, isAutoIncrement: false },
  ];

  it('renders the shared picker with a PK badge on the key column', () => {
    renderDialog({ columns: COLS2 });
    const list = screen.getByTestId('export-columns-list');
    const labels = Array.from(list.querySelectorAll('label')).map(
      (label) => label.textContent,
    );
    expect(labels).toEqual(['idPKint', 'nametext']);
    expect(screen.getByTestId('export-column-id')).toBeChecked();
    expect(screen.getByTestId('export-column-name')).toBeChecked();
  });

  it('reset-to-default restores every column after toggling one off', () => {
    renderDialog({ columns: COLS2 });
    fireEvent.click(screen.getByTestId('export-column-name'));
    expect(screen.getByTestId('export-column-name')).not.toBeChecked();
    expect(screen.getByRole('button', { name: 'export.export' })).not.toBeDisabled();
    fireEvent.click(screen.getByTestId('export-column-name'));
    fireEvent.click(screen.getByTestId('export-columns-clear-all'));
    expect(screen.getByRole('button', { name: 'export.export' })).toBeDisabled();
    fireEvent.click(screen.getByTestId('export-columns-reset-default'));
    expect(screen.getByRole('button', { name: 'export.export' })).not.toBeDisabled();
    expect(screen.getByTestId('export-column-id')).toBeChecked();
    expect(screen.getByTestId('export-column-name')).toBeChecked();
  });

  it('exports only the checked columns through the shared picker', async () => {
    renderDialog({ columns: COLS2 });
    fireEvent.click(screen.getByTestId('export-column-name'));
    fireEvent.click(screen.getByRole('button', { name: 'export.export' }));
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith('id\n1\n2', 'users.csv', 'text/csv');
    expect(exportTablesStream).not.toHaveBeenCalled();
  });
});

describe('ExportDialog format availability (item 1c)', () => {
  const COLS_NO_KEY: ColumnSchema[] = [
    { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: false, isAutoIncrement: false },
    { name: 'name', dataType: 'text', nullable: true, isPrimaryKey: false, isAutoIncrement: false },
  ];

  /** Format is the first combobox, scope the last (mock renders both). */
  function formatSelect(): HTMLSelectElement {
    return screen.getAllByRole('combobox')[0] as HTMLSelectElement;
  }
  function formatOption(value: string): HTMLOptionElement {
    return Array.from(formatSelect().options).find((o) => o.value === value)!;
  }

  it('disables SQL UPDATE with its reason when no key column exists', () => {
    renderDialog({ columns: COLS_NO_KEY });
    expect(formatOption('sql_update').disabled).toBe(true);
    expect(formatOption('csv').disabled).toBe(false);
    expect(formatOption('xlsx').disabled).toBe(false);
    expect(screen.getByTestId('export-format-disabled-reason')).toHaveTextContent(
      'export.formatDisabledNoKey',
    );
  });

  it('enables SQL UPDATE and shows no reason when a key column exists', () => {
    renderDialog();
    expect(formatOption('sql_update').disabled).toBe(false);
    expect(screen.queryByTestId('export-format-disabled-reason')).not.toBeInTheDocument();
  });

  it('ignores selecting a disabled format', () => {
    renderDialog({ columns: COLS_NO_KEY });
    fireEvent.change(formatSelect(), { target: { value: 'sql_update' } });
    expect(formatSelect()).toHaveValue('csv');
  });

  it('falls back to CSV when the key column disappears', () => {
    const { rerender } = renderDialog();
    fireEvent.change(formatSelect(), { target: { value: 'sql_update' } });
    expect(formatSelect()).toHaveValue('sql_update');
    rerender(
      <ExportDialog
        open
        onClose={vi.fn()}
        tableName="users"
        columns={COLS_NO_KEY}
        rows={[{ id: 1 }, { id: 2 }]}
        selectedRows={new Set()}
        dbSessionId="c1"
      />,
    );
    expect(formatSelect()).toHaveValue('csv');
    expect(screen.getByTestId('export-format-disabled-reason')).toHaveTextContent(
      'export.formatDisabledNoKey',
    );
  });

  it('never disables XLSX for the entire-table scope on desktop', () => {
    envState.desktop = true;
    renderDialog({ defaultScope: 'entire_table' });
    expect(formatOption('xlsx').disabled).toBe(false);
    expect(screen.queryByTestId('export-format-disabled-reason')).not.toBeInTheDocument();
  });
});
