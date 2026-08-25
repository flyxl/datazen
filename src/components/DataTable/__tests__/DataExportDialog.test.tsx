import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, waitFor, cleanup } from '@testing-library/react';
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

vi.mock('../../../commands/file', () => ({
  fileCommands: {
    saveTextWithDialog: (...args: unknown[]) => saveTextWithDialog(...args),
    saveBase64WithDialog: (...args: unknown[]) => saveBase64WithDialog(...args),
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
    expect(getByText('export.title')).toBeInTheDocument();
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
    const { container, getByText } = render(
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
    const { container } = render(
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
