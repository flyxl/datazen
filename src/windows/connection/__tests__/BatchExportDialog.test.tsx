import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, waitFor, cleanup, screen } from '@testing-library/react';
import { BatchExportDialog } from '../BatchExportDialog';
import type { BatchExportTableInput } from '../../../lib/batchExport';

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

const fileCommands = vi.fn();
const exportTablesStream = vi.fn();

vi.mock('../../../commands/file', () => ({
  fileCommands: {
    exportTablesStream: (...args: unknown[]) => exportTablesStream(...args),
  },
}));

const TABLES = ['users', 'orders', 'products'];

function mockTable(name: string): BatchExportTableInput {
  return {
    tableName: name,
    ddl: `CREATE TABLE ${name} (id INT);`,
    columns: [{ name: 'id' }],
    rows: [{ id: 1 }],
  };
}

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  exportTablesStream.mockResolvedValue({ Saved: 1 });
});

describe('BatchExportDialog', () => {
  it('selects all tables and clears selection', () => {
    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        connectionId="c1"
        tables={TABLES}
        loadTableExportData={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText('batchExport.selectAll'));
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
        connectionId="c1"
        tables={TABLES}
        initialSelected={['orders']}
        loadTableExportData={vi.fn()}
      />,
    );
    expect((screen.getByLabelText('orders') as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText('users') as HTMLInputElement).checked).toBe(false);
  });

  it('switches mode and hides format for structure_only', () => {
    render(
      <BatchExportDialog
        open
        onClose={vi.fn()}
        connectionId="c1"
        tables={TABLES}
        loadTableExportData={vi.fn()}
      />,
    );

    expect(screen.getByTestId('data-format')).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('batchExport.modeStructureOnly'));
    expect(screen.queryByTestId('data-format')).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('batchExport.modeDataOnly'));
    expect(screen.getByTestId('data-format')).toBeInTheDocument();
  });

  it('exports via loadTableExportData then shows a success view (not auto-close)', async () => {
    const onClose = vi.fn();
    const loadTableExportData = vi.fn(async (name: string) => mockTable(name));

    render(
      <BatchExportDialog
        open
        onClose={onClose}
        connectionId="c1"
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
    expect(request.connectionId).toBe('c1');
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
        connectionId="c1"
        tables={['users']}
        initialSelected={['users']}
        loadTableExportData={loadTableExportData}
      />,
    );

    fireEvent.click(screen.getByLabelText('batchExport.modeStructureOnly'));
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
        connectionId="c1"
        tables={['users']}
        initialSelected={['users']}
        loadTableExportData={loadTableExportData}
      />,
    );

    fireEvent.click(screen.getByText('batchExport.export'));

    await waitFor(() => {
      expect(screen.getByText(/batchExport\.failed/)).toBeInTheDocument();
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
        connectionId="c1"
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
        connectionId="c1"
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
