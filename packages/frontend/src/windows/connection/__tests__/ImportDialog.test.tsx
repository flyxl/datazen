import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, screen, fireEvent, waitFor } from '@testing-library/react';
import { ImportDialog } from '../ImportDialog';
import type { ColumnSchema } from '../../../types';
import type { ImportRunResult } from '../../../lib/importExecution';

/**
 * Wizard-level journeys for the DBR-01 import dialog: the wizard never shows a
 * start button for an unexecutable mapping, reports parse refusals, and the
 * finished run surfaces committed rows plus refresh failures.
 */

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string, params?: Record<string, string | number>) => {
    if (params == null) return key;
    return `${key}:${Object.values(params).join(',')}`;
  } }),
}));

vi.mock('../../../lib/schemaCache', () => ({
  getCachedTableSchema: vi.fn(),
}));

vi.mock('../../../commands/file', () => ({
  fileCommands: {
    openBase64WithDialog: vi.fn(),
  },
}));

vi.mock('../../../stores/schemaStore', () => ({
  useConnectionSchemaField: vi.fn().mockReturnValue([]),
}));

vi.mock('../../../components/ui/Select', () => ({
  // Native select so options and values are DOM-readable (same discipline as
  // the export dialog tests).
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
      value={String(value)}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
      data-testid={`select-${String(value).slice(0, 12)}`}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value} disabled={o.disabled}>
          {o.label}
        </option>
      ))}
    </select>
  ),
}));

vi.mock('../../../components/ui/Spinner', () => ({
  Spinner: () => <span data-testid="spinner" />,
}));

vi.mock('../../../lib/importExecution', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/importExecution')>();
  return {
    ...actual,
    createIpcImportTransport: vi.fn().mockReturnValue({
      runStream: vi.fn(),
      cancelInFlight: vi.fn(),
    }),
    executeImport: vi.fn(),
  };
});

import { getCachedTableSchema } from '../../../lib/schemaCache';
import { fileCommands } from '../../../commands/file';
import { executeImport, createIpcImportTransport } from '../../../lib/importExecution';

const TARGET_COLUMNS: ColumnSchema[] = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
];

/** base64 of a small CSV with header `id,name`. */
const CSV_BASE64 = btoa('id,name\n1,alpha\n2,beta');

function renderDialog(overrides: Partial<Parameters<typeof ImportDialog>[0]> = {}) {
  return render(
    <ImportDialog
      open
      onClose={vi.fn()}
      dbSessionId="c1"
      tableName="users"
      schema={null}
      database="app"
      connectionId="conn1"
      onImported={vi.fn()}
      databaseType="postgresql"
      {...overrides}
    />,
  );
}

async function pickCsvFile(fileName = 'data.csv', dataBase64 = CSV_BASE64) {
  vi.mocked(fileCommands.openBase64WithDialog).mockResolvedValue({
    fileName,
    dataBase64,
  } as never);
  // Before a file is picked the trigger is labeled by the i18n key (the
  // mocked t returns keys); after picking, the file name replaces it.
  fireEvent.click(screen.getByRole('button', { name: /import\.selectFile/ }));
}

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getCachedTableSchema).mockResolvedValue({
    columns: TARGET_COLUMNS,
  } as never);
});

describe('ImportDialog wizard (DBR-01)', () => {
  it('parses the picked CSV, loads target columns and plans the mapping', async () => {
    renderDialog();
    await pickCsvFile();

    await waitFor(() => {
      expect(screen.getByTestId('import-mapping-table')).toBeTruthy();
    });

    // Both columns matched by name → summary line says 2 mapped of 2.
    const summary = screen.getByTestId('import-summary').textContent ?? '';
    expect(summary).toContain('2');
    // Preview renders both rows.
    expect(screen.getByTestId('import-preview').textContent).toContain('alpha');
    expect(screen.getByTestId('import-preview').textContent).toContain('beta');
    // Start is enabled with the row count.
    const start = screen.getByRole('button', { name: /import\.start/i });
    expect(start.getAttribute('disabled')).toBeNull();
  });

  it('refuses to start while target columns fail to load', async () => {
    vi.mocked(getCachedTableSchema).mockRejectedValue(new Error('relation not found') as never);
    renderDialog();
    await pickCsvFile();

    // The readable load failure surfaces; no mapping table can render without
    // target columns, and the start button stays disabled.
    await waitFor(() => {
      expect(screen.getByText(/import\.loadTargetsFailed/)).toBeTruthy();
    });
    const start = screen.getByRole('button', { name: /import\.start|import\.import/i });
    expect(start.getAttribute('disabled')).not.toBeNull();
  });

  it('a duplicate source column is surfaced as a blocking warning', async () => {
    renderDialog();
    await pickCsvFile('dup.csv', btoa('id,id\n1,2'));

    // The duplicate-column banner blocks the start: the mapping cannot
    // represent two sources with one name, so the file must be fixed first.
    await waitFor(() => {
      expect(screen.getByTestId('import-duplicate-warning')).toBeTruthy();
    });
    const start = screen.getByRole('button', { name: /import\.start/i });
    expect(start.getAttribute('disabled')).not.toBeNull();
  });

  it('runs the import, awaits the refresh callback and reports the committed count', async () => {
    const onImported = vi.fn().mockResolvedValue(undefined);
    const run: ImportRunResult = {
      status: 'completed',
      committedRows: 2,
      failedRows: [],
      stoppedBy: null,
    };
    vi.mocked(executeImport).mockResolvedValue(run);
    renderDialog({ onImported });
    await pickCsvFile();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /import\.start/i }).getAttribute('disabled')).toBeNull();
    });
    fireEvent.click(screen.getByRole('button', { name: /import\.start/i }));

    await waitFor(() => {
      expect(screen.getByTestId('import-result')).toBeTruthy();
    });
    expect(executeImport).toHaveBeenCalledWith(
      expect.objectContaining({
        table: 'users',
        onError: 'stop',
        commitMode: 'transaction',
        rowOrigins: [2, 3],
      }),
    );
    expect(createIpcImportTransport).toHaveBeenCalledWith({
      dbSessionId: 'c1',
      database: 'app',
      schema: null,
    });
    expect(onImported).toHaveBeenCalledWith({ table: 'users', schema: null, database: 'app' });
    expect(screen.getByTestId('import-result').textContent).toContain('2');
  });

  it('a rejected refresh callback becomes the refresh-failed notice, not a lost result', async () => {
    const onImported = vi.fn().mockRejectedValue(new Error('refresh boom'));
    vi.mocked(executeImport).mockResolvedValue({
      status: 'completed',
      committedRows: 2,
      failedRows: [],
      stoppedBy: null,
    });
    renderDialog({ onImported });
    await pickCsvFile();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /import\.start/i }).getAttribute('disabled')).toBeNull();
    });
    fireEvent.click(screen.getByRole('button', { name: /import\.start/i }));

    await waitFor(() => {
      expect(screen.getByTestId('import-result')).toBeTruthy();
    });
    expect(screen.getByTestId('import-result').textContent).toContain('import.refreshFailed');
  });
});
