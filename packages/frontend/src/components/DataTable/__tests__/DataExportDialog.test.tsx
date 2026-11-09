import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, waitFor, cleanup, screen } from '@testing-library/react';
import * as XLSX from 'xlsx';
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
const cancelExportTablesMock = vi.fn();

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
  envState.desktop = false;
  bindFakeExporter();
  exportTablesStream.mockResolvedValue({ Saved: 2 });
  cancelExportTablesMock.mockResolvedValue(true);
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
      // Current page is a provided export now: formatted in the frontend
      // and Blob-downloaded, never sent to the Rust exporter.
      expect(downloadBlobMock).toHaveBeenCalledWith(
        'id,name\n1,a',
        'users.csv',
        'text/csv',
      );
      expect(exportTablesStream).not.toHaveBeenCalled();
      expect(getByText('export.success')).toBeInTheDocument();
    });
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(getByText('common.close'));
    expect(onClose).toHaveBeenCalled();
  });

  it('exports current page via the frontend formatter, never the Rust exporter', async () => {
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
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id,name\n1,a\n2,b',
      'users.csv',
      'text/csv',
    );
    expect(exportTablesStream).not.toHaveBeenCalled();
    await waitFor(() => expect(getByText('export.success')).toBeInTheDocument());
  });

  it('exports selected rows only, via the frontend formatter', async () => {
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
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id,name\n3,c',
      'users.csv',
      'text/csv',
    );
    expect(exportTablesStream).not.toHaveBeenCalled();
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

  it('exports xlsx through the frontend formatter, not the Rust exporter', async () => {
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
      expect(downloadBlobMock).toHaveBeenCalledTimes(1);
    });
    const [bytes, filename, mimeType] = downloadBlobMock.mock.calls[0]!;
    expect(filename).toBe('users.xlsx');
    expect(mimeType).toBe(
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    );
    expect(bytes).toBeInstanceOf(Uint8Array);
    const wb = XLSX.read(bytes as Uint8Array, { type: 'array' });
    const matrix = XLSX.utils.sheet_to_json<unknown[]>(wb.Sheets['Sheet1']!, {
      header: 1,
      defval: null,
    });
    expect(matrix).toEqual([
      ['id', 'name'],
      [1, 'a'],
    ]);
    expect(exportTablesStream).not.toHaveBeenCalled();
  });

  it('shows error when export fails', async () => {
    exportTablesStream.mockRejectedValue(new Error('disk full'));
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
    fireEvent.click(getByText('export.export'));
    await waitFor(() => {
      expect(getByText('disk full')).toBeInTheDocument();
    });
  });

  it('does not close when user cancels save dialog', async () => {
    exportTablesStream.mockResolvedValue({ Cancelled: null });
    const onClose = vi.fn();
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={onClose}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        totalRows={100}
        initialScope="entire_table"
      />,
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

describe('DataExportDialog without a session (task 4)', () => {
  it('exports the current page without dbSessionId, via the frontend formatter', async () => {
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
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id,name\n1,a\n2,b',
      'users.csv',
      'text/csv',
    );
    expect(exportTablesStream).not.toHaveBeenCalled();
    await waitFor(() => expect(getByText('export.success')).toBeInTheDocument());
  });

  it('exports selected rows without dbSessionId', async () => {
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
        initialScope="selected"
      />,
    );
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id,name\n3,c',
      'users.csv',
      'text/csv',
    );
    expect(exportTablesStream).not.toHaveBeenCalled();
  });

  it('hides entire/filtered scopes without dbSessionId', () => {
    render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        totalRows={100}
        onBuildFilteredExportSql={vi.fn().mockResolvedValue('SELECT 1;')}
      />,
    );
    const range = document.querySelectorAll('select')[1]!;
    const values = Array.from(range.options).map((o) => o.value);
    expect(values).toContain('current_page');
    expect(values).not.toContain('entire_table');
    expect(values).not.toContain('filtered_table');
  });
});

describe('DataExportDialog entire-table scope (item 3b)', () => {
  it('offers xlsx for entire tables on desktop and routes them to Rust as a table source', async () => {
    envState.desktop = true;
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

describe('DataExportDialog cancellation (item 1b)', () => {
  function renderEntireTable() {
    envState.desktop = true;
    return render(
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
  }

  it('disables the cancel button before the export starts', () => {
    const { getByText } = renderEntireTable();
    expect(getByText('export.cancelExport')).toBeInTheDocument();
    expect(getByText('export.cancelExport')).toBeDisabled();
  });

  it('cancels mid-flight and reports the partial file deleted', async () => {
    let resolveExport!: (result: unknown) => void;
    exportTablesStream.mockImplementation(
      () => new Promise((resolve) => { resolveExport = resolve; }),
    );
    const { getByText, getByTestId, queryByText } = renderEntireTable();
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));

    const cancelButton = getByText('export.cancelExport');
    expect(cancelButton).toBeEnabled();
    fireEvent.click(cancelButton);
    // While the cancel is in flight the button shows the cancelling
    // label and cannot be pressed again.
    expect(getByText('export.cancelling')).toBeInTheDocument();
    expect(getByText('export.cancelling')).toBeDisabled();
    const request = exportTablesStream.mock.calls[0]![0] as {
      cancelToken?: string;
    };
    expect(cancelExportTablesMock).toHaveBeenCalledTimes(1);
    expect(cancelExportTablesMock).toHaveBeenCalledWith(request.cancelToken);

    resolveExport({ Aborted: { partialDeleted: true } });
    await waitFor(() =>
      expect(getByTestId('export-aborted-panel')).toBeInTheDocument(),
    );
    expect(getByText('export.cancelledPartialDeleted')).toBeInTheDocument();
    expect(queryByText('export.success')).not.toBeInTheDocument();
    // The aborted panel leaves only Close in the footer.
    expect(getByText('common.close')).toBeInTheDocument();
    expect(queryByText('export.export')).not.toBeInTheDocument();
  });

  it('reports the partial file kept when the backend could not delete it', async () => {
    let resolveExport!: (result: unknown) => void;
    exportTablesStream.mockImplementation(
      () => new Promise((resolve) => { resolveExport = resolve; }),
    );
    const { getByText, getByTestId } = renderEntireTable();
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
    fireEvent.click(getByText('export.cancelExport'));
    resolveExport({ Aborted: { partialDeleted: false } });
    await waitFor(() =>
      expect(getByTestId('export-aborted-panel')).toBeInTheDocument(),
    );
    expect(getByText('export.cancelledPartialKept')).toBeInTheDocument();
  });

  it('offers no cancel button for the provided (current-page) scope', () => {
    envState.desktop = true;
    const { queryByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
      />,
    );
    // Current page is a one-shot provided export — nothing to cancel.
    expect(queryByText('export.cancelExport')).not.toBeInTheDocument();
    expect(queryByText('export.cancelling')).not.toBeInTheDocument();
  });
});

describe('DataExportDialog web format supply (task 6, plan §2.8)', () => {
  /** Format is the first `<select>`, scope the second (mock renders both). */
  function formatSelect(): HTMLSelectElement {
    return document.querySelectorAll('select')[0] as HTMLSelectElement;
  }
  function scopeSelect(): HTMLSelectElement {
    return document.querySelectorAll('select')[1] as HTMLSelectElement;
  }
  function formatValues(): string[] {
    return Array.from(formatSelect().options).map((o) => o.value);
  }

  function renderWebDialog(extra: Partial<Parameters<typeof DataExportDialog>[0]> = {}) {
    return render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        totalRows={100}
        {...extra}
      />,
    );
  }

  it('offers every format for the provided current-page scope on web', () => {
    renderWebDialog();
    expect(formatValues()).toEqual([
      'csv',
      'tsv',
      'json',
      'markdown',
      'xlsx',
      'sql_insert',
      'sql_update',
    ]);
  });

  it('offers CSV only for the entire-table scope on web', () => {
    renderWebDialog({ initialScope: 'entire_table' });
    expect(scopeSelect()).toHaveValue('entire_table');
    expect(formatValues()).toEqual(['csv']);
  });

  it('offers CSV only for the filtered scope on web', () => {
    renderWebDialog({
      initialScope: 'filtered_table',
      onBuildFilteredExportSql: vi.fn().mockResolvedValue('SELECT id, name FROM users;'),
    });
    expect(scopeSelect()).toHaveValue('filtered_table');
    expect(formatValues()).toEqual(['csv']);
  });

  it('resets a non-CSV format to CSV when switching to an entire-table scope on web', () => {
    renderWebDialog();
    fireEvent.change(formatSelect(), { target: { value: 'xlsx' } });
    expect(formatSelect()).toHaveValue('xlsx');
    fireEvent.change(scopeSelect(), { target: { value: 'entire_table' } });
    expect(scopeSelect()).toHaveValue('entire_table');
    expect(formatSelect()).toHaveValue('csv');
    expect(formatValues()).toEqual(['csv']);
  });

  it('exports the current page as JSON on web via the frontend formatter', async () => {
    const { getByText } = renderWebDialog();
    fireEvent.change(formatSelect(), { target: { value: 'json' } });
    fireEvent.click(getByText('export.export'));

    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    const [contents, fileName, mimeType] = downloadBlobMock.mock.calls[0] as [
      string,
      string,
      string,
    ];
    expect(JSON.parse(contents)).toEqual([{ id: 1, name: 'a' }]);
    expect(fileName).toBe('users.json');
    expect(mimeType).toBe('application/json');
    expect(exportTablesStream).not.toHaveBeenCalled();
    await waitFor(() => expect(getByText('export.success')).toBeInTheDocument());
  });
});

describe('DataExportDialog column selection (item 1a)', () => {
  function renderDialog(extra: Partial<Parameters<typeof DataExportDialog>[0]> = {}) {
    return render(
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
        primaryKeyColumns={['id']}
        {...extra}
      />,
    );
  }

  it('renders the shared picker with the columns in display order and a PK badge', () => {
    renderDialog();
    const list = screen.getByTestId('export-columns-list');
    const labels = Array.from(list.querySelectorAll('label')).map(
      (label) => label.textContent,
    );
    expect(labels).toEqual(['idPKinteger', 'namevarchar']);
    expect(screen.getByTestId('export-column-id')).toBeChecked();
    expect(screen.getByTestId('export-column-name')).toBeChecked();
  });

  it('exports only the checked columns on the provided path', async () => {
    const { getByText } = renderDialog();
    fireEvent.click(screen.getByTestId('export-column-name'));
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(downloadBlobMock).toHaveBeenCalledTimes(1));
    expect(downloadBlobMock).toHaveBeenCalledWith('id\n1\n2', 'users.csv', 'text/csv');
    expect(exportTablesStream).not.toHaveBeenCalled();
  });

  it('clear-all disables the export button and reset-to-default re-enables it', () => {
    const { getByText } = renderDialog();
    const exportBtn = screen.getByRole('button', { name: 'export.export' });
    expect(exportBtn).not.toBeDisabled();
    fireEvent.click(screen.getByTestId('export-columns-clear-all'));
    expect(exportBtn).toBeDisabled();
    fireEvent.click(screen.getByTestId('export-columns-reset-default'));
    expect(exportBtn).not.toBeDisabled();
    expect(screen.getByTestId('export-column-id')).toBeChecked();
    expect(screen.getByTestId('export-column-name')).toBeChecked();
  });

  it('sends only the picked columns to Rust for the entire-table scope', async () => {
    envState.desktop = true;
    const { getByText } = renderDialog({
      totalRows: 100,
      initialScope: 'entire_table',
    });
    fireEvent.click(screen.getByTestId('export-column-name'));
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
    const request = exportTablesStream.mock.calls[0]![0] as {
      tables: { columns: string[] }[];
    };
    expect(request.tables[0]!.columns).toEqual(['id']);
  });

  it('resets the selection to every column when the dialog re-opens', () => {
    const { rerender } = renderDialog();
    fireEvent.click(screen.getByTestId('export-column-name'));
    expect(screen.getByTestId('export-column-name')).not.toBeChecked();
    rerender(
      <DataExportDialog
        open={false}
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        primaryKeyColumns={['id']}
      />,
    );
    rerender(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        primaryKeyColumns={['id']}
      />,
    );
    expect(screen.getByTestId('export-column-id')).toBeChecked();
    expect(screen.getByTestId('export-column-name')).toBeChecked();
  });
});
