import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, waitFor, cleanup, screen } from '@testing-library/react';
import { BatchExportDialog } from '../BatchExportDialog';
import type { BatchExportTableInput } from '../../../lib/batchExportJob';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
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
      data-testid="data-format"
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

// Task 6: `IS_DESKTOP_BUILD` flips through the getter mock (the pattern in
// `lib/__tests__/dataFetcher.test.ts`). Desktop is the default here — the
// existing tests pin the Rust batch routing — and the web describe below
// flips it to false per test.
const envState = vi.hoisted(() => ({ desktop: true }));
vi.mock('../../../platform/env', () => ({
  get IS_DESKTOP_BUILD() {
    return envState.desktop;
  },
}));

// Form-split F4: the export flow rides the dual-flow port; the test binds a
// fake exporter through the platform bindings exactly like an assembly does.
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

const TABLES = ['users', 'orders', 'products'];

function mockTable(name: string): BatchExportTableInput {
  return {
    tableName: name,
    ddl: `CREATE TABLE ${name} (id INT);`,
    columns: [{ name: 'id' }],
    rows: [{ id: 1 }],
  };
}

afterEach(() => {
  cleanup();
  clearPlatform();
});

beforeEach(() => {
  vi.clearAllMocks();
  envState.desktop = true;
  bindFakeExporter();
  exportTablesStream.mockResolvedValue({ Saved: 1 });
});

describe('BatchExportDialog formats (item 3d)', () => {
  function renderDialog() {
    const loadTableExportData = vi.fn(async (name: string) => mockTable(name));
    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        dbSessionId="c1"
        databaseType="postgres"
        tables={['users']}
        initialSelected={['users']}
        loadTableExportData={loadTableExportData}
      />,
    );
    return { loadTableExportData };
  }

  it('offers every format the Rust exporter supports', () => {
    renderDialog();
    const values = Array.from(
      (screen.getByTestId('data-format') as HTMLSelectElement).options,
    ).map((o) => o.value);
    expect(values).toEqual(['csv', 'tsv', 'json', 'markdown', 'xlsx', 'sql_insert', 'sql_update']);
  });

  it.each([['xlsx'], ['sql_update'], ['tsv'], ['markdown']] as const)(
    'routes %s through the Rust batch exporter',
    async (format) => {
      renderDialog();
      fireEvent.change(screen.getByTestId('data-format'), { target: { value: format } });
      fireEvent.click(screen.getByText('batchExport.export'));

      await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
      const request = exportTablesStream.mock.calls[0]![0] as { dataFormat: string };
      expect(request.dataFormat).toBe(format);
      await waitFor(() => expect(screen.getByText('batchExport.success')).toBeInTheDocument());
    },
  );
});

describe('BatchExportDialog', () => {
  it('selects all tables and clears selection', () => {
    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        dbSessionId="c1"
        tables={TABLES}
        loadTableExportData={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText('common.selectAll'));
    for (const name of TABLES) {
      const checkbox = screen.getByLabelText(name) as HTMLInputElement;
      expect(checkbox.checked).toBe(true);
    }

    fireEvent.click(screen.getByText('batchExport.clearAll'));
    for (const name of TABLES) {
      const checkbox = screen.getByLabelText(name) as HTMLInputElement;
      expect(checkbox.checked).toBe(false);
    }
  });

  it('honors initialSelected', () => {
    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        dbSessionId="c1"
        tables={TABLES}
        initialSelected={['orders']}
        loadTableExportData={vi.fn()}
      />,
    );
    expect((screen.getByLabelText('orders') as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText('users') as HTMLInputElement).checked).toBe(false);
  });

  it('toggles one table off and back on from its checkbox', () => {
    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        dbSessionId="c1"
        tables={TABLES}
        initialSelected={['users']}
        loadTableExportData={vi.fn()}
      />,
    );
    const checkbox = screen.getByLabelText('users') as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
    expect(screen.getByText('batchExport.export')).not.toBeDisabled();

    fireEvent.click(checkbox);
    expect(checkbox.checked).toBe(false);
    expect(screen.getByText('batchExport.export')).toBeDisabled();

    fireEvent.click(checkbox);
    expect(checkbox.checked).toBe(true);
    expect(screen.getByText('batchExport.export')).not.toBeDisabled();
  });

  it('shows the driver lock warning and disables export when capability is none', () => {
    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        dbSessionId="c1"
        tables={TABLES}
        initialSelected={['users']}
        loadTableExportData={vi.fn()}
        dataExportCapability="none"
      />,
    );
    expect(screen.getByText('batchExport.disabledByDriver')).toBeInTheDocument();
    expect(screen.getByText('batchExport.export')).toBeDisabled();
  });

  it('switches mode and hides format for structure_only', () => {
    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        dbSessionId="c1"
        tables={TABLES}
        loadTableExportData={vi.fn()}
      />,
    );

    expect(screen.getByTestId('data-format')).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('common.structureOnly'));
    expect(screen.queryByTestId('data-format')).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('common.dataOnly'));
    expect(screen.getByTestId('data-format')).toBeInTheDocument();
  });

  it('exports via loadTableExportData then shows a success view (not auto-close)', async () => {
    const onClose = vi.fn();
    const loadTableExportData = vi.fn(async (name: string) => mockTable(name));

    render(
      <BatchExportDialog
        open
        onClose={onClose}
        dbSessionId="c1"
        databaseType="postgres"
        tables={TABLES}
        initialSelected={['users', 'orders']}
        loadTableExportData={loadTableExportData}
      />,
    );

    // default output is zip; structure+data with csv
    fireEvent.click(screen.getByText('batchExport.export'));

    await waitFor(() => {
      expect(loadTableExportData).toHaveBeenCalledTimes(2);
      expect(exportTablesStream).toHaveBeenCalledTimes(1);
      // Success view replaces the form; the dialog does not auto-close.
      expect(screen.getByText('batchExport.success')).toBeInTheDocument();
      expect(onClose).not.toHaveBeenCalled();
    });
    expect(loadTableExportData.mock.calls.map((c) => c[0])).toEqual(['users', 'orders']);

    const request = exportTablesStream.mock.calls[0]![0];
    // Wire key mirrors backend ExportTablesRequest (serde camelCase).
    expect(request.dbSessionId).toBe('c1');
    expect(request.databaseType).toBe('postgres');
    expect(request.tables.map((t: { tableName: string }) => t.tableName)).toEqual([
      'users',
      'orders',
    ]);

    // Closing is explicit via the Close button.
    fireEvent.click(screen.getByText('common.close'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('exports combined single file via saveText', async () => {
    const loadTableExportData = vi.fn(async (name: string) => mockTable(name));

    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        dbSessionId="c1"
        tables={['users']}
        initialSelected={['users']}
        loadTableExportData={loadTableExportData}
      />,
    );

    fireEvent.click(screen.getByLabelText('common.structureOnly'));
    fireEvent.click(screen.getByLabelText('batchExport.outputSingle'));
    fireEvent.click(screen.getByText('batchExport.export'));

    await waitFor(() => {
      expect(exportTablesStream).toHaveBeenCalledTimes(1);
      expect(screen.getByText('batchExport.success')).toBeInTheDocument();
    });
    const request = exportTablesStream.mock.calls[0]![0];
    expect(request.outputMode).toBe('single');
    expect(request.mode).toBe('structure_only');
  });

  it('shows error when export fails', async () => {
    const loadTableExportData = vi.fn().mockRejectedValue(new Error('boom'));

    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        dbSessionId="c1"
        tables={['users']}
        initialSelected={['users']}
        loadTableExportData={loadTableExportData}
      />,
    );

    fireEvent.click(screen.getByText('batchExport.export'));

    await waitFor(() => {
      expect(screen.getByText(/common\.exportFailed/)).toBeInTheDocument();
    });
  });

  it('uses onExport override when provided', async () => {
    const onExport = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    const loadTableExportData = vi.fn();

    render(
      <BatchExportDialog
        open
        onClose={onClose}
        dbSessionId="c1"
        tables={['users']}
        initialSelected={['users']}
        loadTableExportData={loadTableExportData}
        onExport={onExport}
      />,
    );

    fireEvent.click(screen.getByText('batchExport.export'));

    await waitFor(() => {
      expect(onExport).toHaveBeenCalledWith(
        expect.objectContaining({
          selectedTables: ['users'],
          mode: 'data_and_structure',
          outputMode: 'zip',
        }),
      );
      expect(screen.getByText('batchExport.success')).toBeInTheDocument();
      expect(onClose).not.toHaveBeenCalled();
    });
    expect(loadTableExportData).not.toHaveBeenCalled();
  });

  it('returns to the form when the native save dialog is dismissed', async () => {
    exportTablesStream.mockResolvedValue({ Cancelled: null });
    const onClose = vi.fn();
    const loadTableExportData = vi.fn(async (name: string) => mockTable(name));

    render(
      <BatchExportDialog
        open
        onClose={onClose}
        dbSessionId="c1"
        tables={['users']}
        initialSelected={['users']}
        loadTableExportData={loadTableExportData}
      />,
    );

    fireEvent.click(screen.getByText('batchExport.export'));

    await waitFor(() => {
      expect(exportTablesStream).toHaveBeenCalledTimes(1);
      // Back on the form (Export button visible again), no success view.
      expect(screen.getByText('batchExport.export')).toBeInTheDocument();
      expect(screen.queryByText('batchExport.success')).not.toBeInTheDocument();
      expect(onClose).not.toHaveBeenCalled();
    });
  });
});

describe('BatchExportDialog web limits (task 6, plan §2.8)', () => {
  beforeEach(() => {
    envState.desktop = false;
  });

  function renderWebDialog(onExport?: (opts: {
    selectedTables: string[];
    mode: string;
    dataFormat: string;
    outputMode: string;
  }) => Promise<void>) {
    const loadTableExportData = vi.fn(async (name: string) => mockTable(name));
    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        dbSessionId="c1"
        databaseType="postgres"
        tables={['users']}
        initialSelected={['users']}
        loadTableExportData={loadTableExportData}
        onExport={onExport}
      />,
    );
    return { loadTableExportData };
  }

  function outputRadio(value: 'single' | 'zip'): HTMLInputElement {
    return document.querySelector(
      `input[name="batch-export-output"][value="${value}"]`,
    ) as HTMLInputElement;
  }

  it('offers CSV only: batch export is always a SQL-scope export on web', () => {
    renderWebDialog();
    const values = Array.from(
      (screen.getByTestId('data-format') as HTMLSelectElement).options,
    ).map((o) => o.value);
    expect(values).toEqual(['csv']);
  });

  it('disables zip output and defaults to single-file on web', async () => {
    const onExport = vi.fn().mockResolvedValue(undefined);
    renderWebDialog(onExport);

    expect(outputRadio('zip').disabled).toBe(true);
    expect(outputRadio('single').disabled).toBe(false);
    expect(outputRadio('single').checked).toBe(true);

    fireEvent.click(screen.getByText('batchExport.export'));
    await waitFor(() => expect(onExport).toHaveBeenCalledTimes(1));
    expect(onExport).toHaveBeenCalledWith(
      expect.objectContaining({ dataFormat: 'csv', outputMode: 'single' }),
    );
  });

  it('still routes a web CSV batch through the tables exporter', async () => {
    renderWebDialog();
    fireEvent.click(screen.getByText('batchExport.export'));

    await waitFor(() => expect(exportTablesStream).toHaveBeenCalledTimes(1));
    const request = exportTablesStream.mock.calls[0]![0] as {
      dataFormat: string;
      outputMode: string;
    };
    expect(request.dataFormat).toBe('csv');
    expect(request.outputMode).toBe('single');
    await waitFor(() => expect(screen.getByText('batchExport.success')).toBeInTheDocument());
  });
});
