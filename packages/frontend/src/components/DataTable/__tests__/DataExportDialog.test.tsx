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

const saveTextWithDialog = vi.fn();
const saveBase64WithDialog = vi.fn();
const beginSaveWithDialog = vi.fn();
const appendSaveText = vi.fn();
const finishSave = vi.fn();
const abortSave = vi.fn();
const executeQueryStream = vi.fn();

vi.mock('../../../commands/file', () => ({
  fileCommands: {
    saveTextWithDialog: (...args: unknown[]) => saveTextWithDialog(...args),
    saveBase64WithDialog: (...args: unknown[]) => saveBase64WithDialog(...args),
    beginSaveWithDialog: (...args: unknown[]) => beginSaveWithDialog(...args),
    appendSaveText: (...args: unknown[]) => appendSaveText(...args),
    finishSave: (...args: unknown[]) => finishSave(...args),
    abortSave: (...args: unknown[]) => abortSave(...args),
  },
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    executeQueryStream: (...args: unknown[]) => executeQueryStream(...args),
    cancelQuery: vi.fn().mockResolvedValue(undefined),
  },
}));

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  saveTextWithDialog.mockResolvedValue(true);
  saveBase64WithDialog.mockResolvedValue(true);
  // The streaming save session: a file token, chunks appended, then done.
  beginSaveWithDialog.mockResolvedValue('save-token');
  appendSaveText.mockResolvedValue(undefined);
  finishSave.mockResolvedValue(undefined);
  // DBR-06: abort answers whether the partial file was deleted.
  abortSave.mockResolvedValue(true);
  // The stream query: immediately end-of-stream, no rows.
  executeQueryStream.mockImplementation(
    (_dbSessionId: string, _sql: string, onData: (event: unknown) => void) => {
      onData({ type: 'end' });
      return Promise.resolve();
    },
  );
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
      />,
    );
    fireEvent.click(getByText('export.export'));
    await waitFor(() => {
      expect(saveTextWithDialog).toHaveBeenCalled();
      expect(getByText('export.success')).toBeInTheDocument();
    });
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(getByText('common.close'));
    expect(onClose).toHaveBeenCalled();
  });

  it('exports xlsx as binary', async () => {
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
      />,
    );
    const selects = document.querySelectorAll('select');
    fireEvent.change(selects[0], { target: { value: 'xlsx' } });
    fireEvent.click(getByText('export.export'));
    await waitFor(() => {
      expect(saveBase64WithDialog).toHaveBeenCalled();
    });
  });

  it('shows error when export fails', async () => {
    saveTextWithDialog.mockRejectedValue(new Error('disk full'));
    const { getByText } = render(
      <DataExportDialog open onClose={vi.fn()} columns={COLS} rows={[[1, 'a']]} />,
    );
    fireEvent.click(getByText('export.export'));
    await waitFor(() => {
      expect(getByText('disk full')).toBeInTheDocument();
    });
  });

  it('does not close when user cancels save dialog', async () => {
    saveTextWithDialog.mockResolvedValue(false);
    const onClose = vi.fn();
    const { getByText } = render(
      <DataExportDialog open onClose={onClose} columns={COLS} rows={[[1, 'a']]} />,
    );
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(saveTextWithDialog).toHaveBeenCalled());
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
    expect(saveTextWithDialog).not.toHaveBeenCalled();
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

  it('opens with the filtered scope preselected and streams its statement', async () => {
    const filteredSql = 'SELECT "id" FROM "users" WHERE "status" = \'active\';';
    const onBuildFilteredExportSql = vi.fn().mockResolvedValue(filteredSql);
    render(
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
      expect(executeQueryStream).toHaveBeenCalledOnce();
    });
    // The streamed statement is the backend's own filtered read, not a
    // SELECT the exporter built on its own.
    expect((executeQueryStream.mock.calls[0] as unknown[])[1]).toBe(filteredSql);
    await waitFor(() => expect(finishSave).toHaveBeenCalled());
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

describe('DataExportDialog column selection (DBR-06)', () => {
  it('selects every column by default and exports all of them', async () => {
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
    // Default = every column: both checkboxes are checked.
    expect(
      (document.querySelector('[data-testid="export-column-id"]') as HTMLInputElement).checked,
    ).toBe(true);
    expect(
      (document.querySelector('[data-testid="export-column-name"]') as HTMLInputElement).checked,
    ).toBe(true);
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(saveTextWithDialog).toHaveBeenCalled());
    const content = (saveTextWithDialog.mock.calls[0] as unknown[])[0] as string;
    expect(content.startsWith('id,name')).toBe(true);
  });

  it('exports only the picked columns, in display order', async () => {
    const { getByText } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
      />,
    );
    fireEvent.click(document.querySelector('[data-testid="export-column-id"]') as HTMLInputElement);
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(saveTextWithDialog).toHaveBeenCalled());
    const content = (saveTextWithDialog.mock.calls[0] as unknown[])[0] as string;
    expect(content.trim()).toBe('name\na');
  });

  it('disables the export button when every column is cleared, reset restores them', async () => {
    const { getByText, getByRole } = render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
      />,
    );
    fireEvent.click(screen.getByTestId('export-columns-clear-all'));
    expect(getByRole('button', { name: 'export.export' })).toBeDisabled();
    // 恢复默认：every column again.
    fireEvent.click(screen.getByTestId('export-columns-reset-default'));
    expect(getByRole('button', { name: 'export.export' })).toBeEnabled();
    fireEvent.click(getByText('export.export'));
    await waitFor(() => expect(saveTextWithDialog).toHaveBeenCalled());
    const content = (saveTextWithDialog.mock.calls[0] as unknown[])[0] as string;
    expect(content.startsWith('id,name')).toBe(true);
  });

  it('streams only the picked columns of the filtered statement', async () => {
    const filteredSql = 'SELECT "id", "name" FROM "users" WHERE "status" = \'active\';';
    render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        dbSessionId="c1"
        initialScope="filtered_table"
        onBuildFilteredExportSql={vi.fn().mockResolvedValue(filteredSql)}
      />,
    );
    // Keep only `name`; the streamed statement still selects both columns.
    fireEvent.click(document.querySelector('[data-testid="export-column-id"]') as HTMLInputElement);
    fireEvent.click(screen.getByText('export.export'));
    await waitFor(() => expect(executeQueryStream).toHaveBeenCalledOnce());
    const streamedSql = (executeQueryStream.mock.calls[0] as unknown[])[1];
    expect(streamedSql).toBe(filteredSql);
    await waitFor(() => expect(finishSave).toHaveBeenCalled());
  });
});

describe('DataExportDialog format gating (DBR-06)', () => {
  it('disables xlsx with its reason for the entire-table scope, not by removal', () => {
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
    fireEvent.change(selects[1], { target: { value: 'entire_table' } });
    const format = selects[0]!;
    const xlsx = Array.from(format.options).find((o) => o.value === 'xlsx')!;
    expect(xlsx).toBeDefined();
    expect(xlsx.disabled).toBe(true);
    // The reason is rendered, not guessed at.
    expect(screen.getByText('export.formatDisabledStream')).toBeInTheDocument();
  });

  it('disables sql_update without a primary key column', () => {
    render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
      />,
    );
    const format = document.querySelectorAll('select')[0]!;
    const sqlUpdate = Array.from(format.options).find((o) => o.value === 'sql_update')!;
    expect(sqlUpdate.disabled).toBe(true);
    expect(screen.getByText('export.formatDisabledNoKey')).toBeInTheDocument();
  });

  it('offers sql_update once a primary key column is declared', () => {
    render(
      <DataExportDialog
        open
        onClose={vi.fn()}
        columns={COLS}
        rows={[[1, 'a']]}
        tableName="users"
        primaryKeyColumns={['id']}
      />,
    );
    const format = document.querySelectorAll('select')[0]!;
    const sqlUpdate = Array.from(format.options).find((o) => o.value === 'sql_update')!;
    expect(sqlUpdate.disabled).toBe(false);
    // The grid marks the key column for the picker too.
    expect(screen.getByText('PK')).toBeInTheDocument();
  });
});

describe('DataExportDialog streaming cancel and progress (DBR-06)', () => {
  it('cancels a streaming run and reports the partial file as deleted', async () => {
    let releaseStream: (() => void) | undefined;
    executeQueryStream.mockImplementation(
      (_dbSessionId: string, _sql: string, onData: (event: unknown) => void) => {
        onData({ type: 'executionStarted', executionId: 'exec-9' });
        onData({
          type: 'statementStart',
          index: 0,
          sql: 'SELECT id FROM users',
          columns: [{ name: 'id', dataType: 'int', nullable: false }],
        });
        onData({ type: 'rows', index: 0, rows: [[1], [2]] });
        return new Promise<void>((resolve) => {
          releaseStream = resolve;
        });
      },
    );
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
    fireEvent.change(document.querySelectorAll('select')[1], { target: { value: 'entire_table' } });
    fireEvent.click(screen.getByText('export.export'));
    // Progress of the streamed rows is visible while exporting.
    await waitFor(() => expect(screen.getByTestId('export-progress')).toBeInTheDocument());
    fireEvent.click(screen.getByText('export.cancelExport'));
    expect(releaseStream).toBeDefined();
    releaseStream!();
    await waitFor(() => expect(screen.getByTestId('export-aborted-panel')).toBeInTheDocument());
    expect(screen.getByText('export.cancelledPartialDeleted')).toBeInTheDocument();
    expect(finishSave).not.toHaveBeenCalled();
    expect(abortSave).toHaveBeenCalledWith('save-token');
  });

  it('reports a kept partial file when the abort could not delete it', async () => {
    abortSave.mockResolvedValue(false);
    let releaseStream: (() => void) | undefined;
    executeQueryStream.mockImplementation(
      (_dbSessionId: string, _sql: string, onData: (event: unknown) => void) => {
        onData({ type: 'executionStarted', executionId: 'exec-10' });
        return new Promise<void>((resolve) => {
          releaseStream = resolve;
        });
      },
    );
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
    fireEvent.change(document.querySelectorAll('select')[1], { target: { value: 'entire_table' } });
    fireEvent.click(screen.getByText('export.export'));
    await waitFor(() => expect(screen.getByText('export.exporting')).toBeInTheDocument());
    fireEvent.click(screen.getByText('export.cancelExport'));
    releaseStream!();
    await waitFor(() => expect(screen.getByTestId('export-aborted-panel')).toBeInTheDocument());
    expect(screen.getByText('export.cancelledPartialKept')).toBeInTheDocument();
  });
});
