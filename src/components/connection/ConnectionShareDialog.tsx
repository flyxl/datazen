import { useCallback, useEffect, useState } from 'react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { connectionCommands } from '../../commands/connection';

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
  const appImport = mode === 'import' && isImportApp(importSource);

  useEffect(() => {
    if (!open) {
      setPassword('');
      setConfirmPassword('');
      setDataPath('');
      setPathFound(false);
      setLocalError(null);
      setSubmitting(false);
      setDetecting(false);
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
        onError(e instanceof Error ? e.message : t('connShare.importFailed'));
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
        const count = await connectionCommands.exportConnectionsWithDialog(
          password,
          `datazen-connections-${date}.json`,
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
      } else {
        const result = await connectionCommands.importConnectionsWithDialog(password);
        onClose();
        if (result !== null) {
          onImportSuccess(result);
        }
      }
    } catch (e) {
      const message =
        e instanceof Error
          ? e.message
          : mode === 'export'
            ? t('connShare.exportFailed')
            : t('connShare.importFailed');
      onError(message);
    } finally {
      setSubmitting(false);
    }
  }, [
    appImport,
    confirmPassword,
    dataPath,
    importSource,
    mode,
    onClose,
    onError,
    onExportSuccess,
    onImportSuccess,
    password,
    pathFound,
    t,
  ]);

  const title =
    mode === 'export'
      ? t('connShare.exportTitle')
      : appImport && isImportApp(importSource)
        ? t('connShare.importFromAppTitle', { app: CONNECTION_IMPORT_APP_LABEL[importSource] })
        : t('connShare.importTitle');

  return (
    <Dialog
      open={open}
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} disabled={submitting}>
            {t('common.cancel')}
          </Button>
          <Button variant="primary" onClick={() => void handleSubmit()} disabled={submitting || detecting}>
            {mode === 'export' ? t('connShare.exportAction') : t('connShare.importAction')}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {mode === 'import' && !appImport && (
          <p className="text-xs leading-relaxed text-fg-muted">{t('connShare.importFormatsHint')}</p>
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

        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">
            {t('connShare.password')}
            {mode === 'import' ? (
              <span className="ml-1 font-normal text-fg-muted">({t('connShare.passwordOptional')})</span>
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

        {mode === 'export' && (
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
