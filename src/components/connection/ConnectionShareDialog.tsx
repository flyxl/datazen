import { useCallback, useEffect, useState } from 'react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { connectionCommands } from '../../commands/connection';
import { importFileDisplayName, importFilePasswordPolicy } from '../../lib/importConnectionFile';

export type ConnectionShareMode = 'export' | 'import';

export type ConnectionImportApp = 'dbx' | 'navicat' | 'datagrip' | 'dbeaver' | 'tableplus';

export type ConnectionImportSource = 'file' | ConnectionImportApp;

export const CONNECTION_IMPORT_APP_LABEL: Record<ConnectionImportApp, string> = {
  dbx: 'DBX',
  navicat: 'Navicat',
  datagrip: 'DataGrip',
  dbeaver: 'DBeaver',
  tableplus: 'TablePlus',
};

function ipcErrorMessage(e: unknown, fallback: string): string {
  if (typeof e === 'string' && e.trim()) return e;
  if (e instanceof Error && e.message.trim()) return e.message;
  return fallback;
}

interface ConnectionShareDialogProps {
  open: boolean;
  mode: ConnectionShareMode;
  importSource?: ConnectionImportSource;
  onClose: () => void;
  onExportSuccess: (count: number) => void;
  onImportSuccess: (result: {
    imported: number;
    overwritten: number;
    groupsAdded: number;
    skipped?: string[];
    sourceFormat?: string;
  }) => void;
  onError: (message: string) => void;
}

function isImportApp(source: ConnectionImportSource | undefined): source is ConnectionImportApp {
  return source !== undefined && source !== 'file';
}

export function ConnectionShareDialog({
  open,
  mode,
  importSource = 'file',
  onClose,
  onExportSuccess,
  onImportSuccess,
  onError,
}: ConnectionShareDialogProps) {
  const { t } = useI18n();
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [dataPath, setDataPath] = useState('');
  const [pathFound, setPathFound] = useState(false);
  const [detecting, setDetecting] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [selectedImportFile, setSelectedImportFile] = useState<string | null>(null);
  const appImport = mode === 'import' && isImportApp(importSource);
  const fileImport = mode === 'import' && !appImport;
  const importPasswordPolicy = selectedImportFile
    ? importFilePasswordPolicy(selectedImportFile)
    : null;

  useEffect(() => {
    if (!open) {
      setPassword('');
      setConfirmPassword('');
      setDataPath('');
      setPathFound(false);
      setLocalError(null);
      setSubmitting(false);
      setDetecting(false);
      setSelectedImportFile(null);
      return;
    }
    if (mode !== 'import' || !isImportApp(importSource)) {
      return;
    }
    let cancelled = false;
    setDetecting(true);
    void connectionCommands
      .detectConnectionImportPath(importSource)
      .then((detected) => {
        if (cancelled) return;
        setDataPath(detected.path);
        setPathFound(detected.found);
      })
      .catch(() => {
        if (!cancelled) {
          setDataPath('');
          setPathFound(false);
        }
      })
      .finally(() => {
        if (!cancelled) setDetecting(false);
      });
    return () => {
      cancelled = true;
    };
  }, [importSource, mode, open]);

  const browsePath = useCallback(
    async (kind: 'file' | 'folder') => {
      if (!isImportApp(importSource)) return;
      try {
        const picked = await connectionCommands.pickConnectionImportPathWithDialog(
          kind,
          importSource,
        );
        if (picked) {
          setDataPath(picked);
          setPathFound(true);
          setLocalError(null);
        }
      } catch (e) {
        onError(ipcErrorMessage(e, t('common.importFailed')));
      }
    },
    [importSource, onError, t],
  );

  const handleSubmit = useCallback(async () => {
    setLocalError(null);

    if (mode === 'export') {
      if (!password.trim()) {
        setLocalError(t('connShare.passwordRequired'));
        return;
      }
      if (password !== confirmPassword) {
        setLocalError(t('connShare.passwordMismatch'));
        return;
      }
    }

    if (appImport && !dataPath.trim() && !pathFound) {
      setLocalError(t('connShare.pathRequired'));
      return;
    }

    setSubmitting(true);
    try {
      if (mode === 'export') {
        const date = new Date().toISOString().slice(0, 10).replace(/-/g, '');
        const count = await connectionCommands.exportConnections(
          password,
          `datazen-connections-${date}.datazenconnection`,
        );
        onClose();
        if (count !== null) {
          onExportSuccess(count);
        }
      } else if (appImport && isImportApp(importSource)) {
        const result = await connectionCommands.importConnectionsFromApp(
          importSource,
          password,
          dataPath,
        );
        onClose();
        onImportSuccess(result);
      } else if (fileImport) {
        if (!selectedImportFile) {
          const picked = await connectionCommands.pickConnectionsImportFile();
          if (!picked) return;
          setSelectedImportFile(picked);
          return;
        }

        if (importFilePasswordPolicy(selectedImportFile) === 'required' && !password.trim()) {
          setLocalError(t('connShare.encryptedImportPasswordRequired'));
          return;
        }

        const result = await connectionCommands.importConnectionsAtPath(
          password,
          selectedImportFile,
        );
        onClose();
        onImportSuccess(result);
      }
    } catch (e) {
      onError(
        ipcErrorMessage(e, mode === 'export' ? t('common.exportFailed') : t('common.importFailed')),
      );
    } finally {
      setSubmitting(false);
    }
  }, [
    appImport,
    confirmPassword,
    dataPath,
    fileImport,
    importSource,
    mode,
    onClose,
    onError,
    onExportSuccess,
    onImportSuccess,
    password,
    pathFound,
    selectedImportFile,
    t,
  ]);

  const handlePickImportFile = useCallback(async () => {
    setLocalError(null);
    setSubmitting(true);
    try {
      const picked = await connectionCommands.pickConnectionsImportFile();
      if (picked) setSelectedImportFile(picked);
    } catch (e) {
      onError(ipcErrorMessage(e, t('common.importFailed')));
    } finally {
      setSubmitting(false);
    }
  }, [onError, t]);

  const primaryActionLabel =
    mode === 'export'
      ? t('connShare.exportAction')
      : fileImport && !selectedImportFile
        ? t('connShare.chooseImportFile')
        : t('connShare.importAction');

  const title =
    mode === 'export'
      ? t('common.exportConnections')
      : appImport && isImportApp(importSource)
        ? t('connShare.importFromAppTitle', { app: CONNECTION_IMPORT_APP_LABEL[importSource] })
        : t('common.importConnections');

  const dialogWidthClass = mode === 'export' ? 'max-w-sm' : appImport ? 'max-w-lg' : 'max-w-md';

  return (
    <Dialog
      open={open}
      title={title}
      description={mode === 'export' ? t('connShare.exportHint') : undefined}
      className={dialogWidthClass}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} disabled={submitting}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            onClick={() => void handleSubmit()}
            disabled={submitting || detecting}
          >
            {primaryActionLabel}
          </Button>
        </>
      }
    >
      <div className={mode === 'export' ? 'space-y-3' : 'space-y-4'}>
        {mode === 'import' && !appImport && (
          <p className="text-xs leading-relaxed text-fg-muted">
            {t('connShare.importFormatsHint')}
          </p>
        )}

        {appImport && (
          <div className="space-y-2">
            <p className="text-xs leading-relaxed text-fg-muted">
              {pathFound ? t('connShare.dataPathFoundHint') : t('connShare.dataPathMissingHint')}
            </p>
            <label className="mb-1 block text-xs font-medium text-fg-secondary">
              {t('connShare.dataPath')}
            </label>
            <Input
              data-testid="import-data-path"
              value={dataPath}
              onChange={(e) => setDataPath(e.target.value)}
              disabled={submitting || detecting}
              placeholder={t('connShare.dataPathPlaceholder')}
            />
            <div className="flex flex-wrap gap-2">
              <Button
                variant="secondary"
                onClick={() => void browsePath('folder')}
                disabled={submitting || detecting}
              >
                {t('connShare.browseFolder')}
              </Button>
              <Button
                variant="secondary"
                onClick={() => void browsePath('file')}
                disabled={submitting || detecting}
              >
                {t('connShare.browseFile')}
              </Button>
            </div>
          </div>
        )}

        {fileImport && selectedImportFile && (
          <div className="space-y-2">
            <label className="mb-1 block text-xs font-medium text-fg-secondary">
              {t('connShare.selectedImportFile')}
            </label>
            <div className="flex flex-wrap items-center gap-2">
              <span
                className="min-w-0 flex-1 truncate text-sm text-fg-primary"
                data-testid="import-selected-file"
                title={selectedImportFile}
              >
                {importFileDisplayName(selectedImportFile)}
              </span>
              <Button
                variant="secondary"
                onClick={() => void handlePickImportFile()}
                disabled={submitting}
              >
                {t('connShare.changeImportFile')}
              </Button>
            </div>
          </div>
        )}

        {mode === 'export' && (
          <div className="space-y-3">
            <div>
              <label className="mb-1 block text-xs font-medium text-fg-secondary">
                {t('connShare.password')}
              </label>
              <Input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="new-password"
                disabled={submitting}
              />
            </div>
            <div>
              <label className="mb-1 block text-xs font-medium text-fg-secondary">
                {t('connShare.confirmPassword')}
              </label>
              <Input
                type="password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                autoComplete="new-password"
                disabled={submitting}
              />
            </div>
          </div>
        )}

        {(appImport || (fileImport && selectedImportFile)) && (
          <div>
            <label className="mb-1 block text-xs font-medium text-fg-secondary">
              {t('connShare.password')}
              {mode === 'import' && importPasswordPolicy !== 'required' ? (
                <span className="ml-1 font-normal text-fg-muted">
                  ({t('connShare.passwordOptional')})
                </span>
              ) : null}
            </label>
            <Input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="new-password"
              disabled={submitting}
              placeholder={mode === 'import' ? t('connShare.passwordImportPlaceholder') : undefined}
            />
          </div>
        )}

        {localError && (
          <div className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
            {localError}
          </div>
        )}
      </div>
    </Dialog>
  );
}
