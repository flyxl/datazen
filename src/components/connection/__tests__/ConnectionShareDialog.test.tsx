import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { ConnectionShareDialog } from '../ConnectionShareDialog';

const exportMock = vi.fn();
const importFileMock = vi.fn();
const importAppMock = vi.fn();
const detectMock = vi.fn();
const pickMock = vi.fn();

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    exportConnectionsWithDialog: (...args: unknown[]) => exportMock(...args),
    importConnectionsWithDialog: (...args: unknown[]) => importFileMock(...args),
    importConnectionsFromApp: (...args: unknown[]) => importAppMock(...args),
    detectConnectionImportPath: (...args: unknown[]) => detectMock(...args),
    pickConnectionImportPathWithDialog: (...args: unknown[]) => pickMock(...args),
  },
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) => {
      if (!params) return key;
      return Object.entries(params).reduce(
        (text, [k, v]) => text.replace(`{${k}}`, String(v)),
        key,
      );
    },
  }),
}));

const onClose = vi.fn();
const onExportSuccess = vi.fn();
const onImportSuccess = vi.fn();
const onError = vi.fn();

function renderImport(source: 'file' | 'dbx' | 'navicat' = 'file') {
  return render(
    <ConnectionShareDialog
      open
      mode="import"
      importSource={source}
      onClose={onClose}
      onExportSuccess={onExportSuccess}
      onImportSuccess={onImportSuccess}
      onError={onError}
    />,
  );
}

describe('ConnectionShareDialog', () => {
  beforeEach(() => {
    exportMock.mockReset();
    importFileMock.mockReset();
    importAppMock.mockReset();
    detectMock.mockReset();
    pickMock.mockReset();
    onClose.mockReset();
    onExportSuccess.mockReset();
    onImportSuccess.mockReset();
    onError.mockReset();
    detectMock.mockResolvedValue({ path: '/tmp/com.dbx.app/dbx.db', found: true });
    importFileMock.mockResolvedValue({ imported: 1, overwritten: 0, groupsAdded: 0 });
    importAppMock.mockResolvedValue({
      imported: 2,
      overwritten: 0,
      groupsAdded: 0,
      sourceFormat: 'DBX',
    });
  });

  afterEach(() => {
    cleanup();
  });

  it('imports from file picker without a path field', async () => {
    renderImport('file');
    expect(screen.queryByTestId('import-data-path')).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('connShare.importAction'));
    await waitFor(() => expect(importFileMock).toHaveBeenCalledWith(''));
    expect(onImportSuccess).toHaveBeenCalled();
  });

  it('detects default path for app import and submits it', async () => {
    renderImport('dbx');
    await waitFor(() =>
      expect(screen.getByTestId('import-data-path')).toHaveValue('/tmp/com.dbx.app/dbx.db'),
    );
    expect(screen.getByText('connShare.dataPathFoundHint')).toBeInTheDocument();
    fireEvent.click(screen.getByText('connShare.importAction'));
    await waitFor(() =>
      expect(importAppMock).toHaveBeenCalledWith('dbx', '', '/tmp/com.dbx.app/dbx.db'),
    );
    expect(onImportSuccess).toHaveBeenCalledWith(
      expect.objectContaining({ imported: 2, sourceFormat: 'DBX' }),
    );
  });

  it('lets the user override a missing install path', async () => {
    detectMock.mockResolvedValue({ path: '', found: false });
    renderImport('navicat');
    await waitFor(() => expect(screen.getByTestId('import-data-path')).toHaveValue(''));
    expect(screen.getByText('connShare.dataPathMissingHint')).toBeInTheDocument();
    fireEvent.click(screen.getByText('connShare.importAction'));
    await waitFor(() => expect(screen.getByText('connShare.pathRequired')).toBeInTheDocument());
    fireEvent.change(screen.getByTestId('import-data-path'), {
      target: { value: '/Applications/Navicat Premium.app' },
    });
    fireEvent.click(screen.getByText('connShare.importAction'));
    await waitFor(() =>
      expect(importAppMock).toHaveBeenCalledWith(
        'navicat',
        '',
        '/Applications/Navicat Premium.app',
      ),
    );
  });

  it('browse folder fills the path field', async () => {
    pickMock.mockResolvedValue('/custom/dbx-data');
    renderImport('dbx');
    await waitFor(() => expect(screen.getByTestId('import-data-path')).toBeInTheDocument());
    fireEvent.click(screen.getByText('connShare.browseFolder'));
    await waitFor(() =>
      expect(pickMock).toHaveBeenCalledWith('folder', 'dbx'),
    );
    await waitFor(() =>
      expect(screen.getByTestId('import-data-path')).toHaveValue('/custom/dbx-data'),
    );
  });

  it('export still requires matching passwords', async () => {
    render(
      <ConnectionShareDialog
        open
        mode="export"
        onClose={onClose}
        onExportSuccess={onExportSuccess}
        onImportSuccess={onImportSuccess}
        onError={onError}
      />,
    );
    fireEvent.click(screen.getByText('connShare.exportAction'));
    expect(screen.getByText('connShare.passwordRequired')).toBeInTheDocument();
  });
});
