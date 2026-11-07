import { useCallback, useState } from 'react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { connectionCommands } from '../../commands/connection';
import { ipcConnectionShareError } from '../../lib/connectionShareError';
import { useConnectionImport, type ConnectionImportResult } from './useConnectionImport';
import { ConnectionImportFields } from './ConnectionImportFields';

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
  onImportSuccess: (result: ConnectionImportResult) => void;
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
  const [submitting, setSubmitting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const appImport = mode === 'import' && isImportApp(importSource);

  // File/application import state machine (shared with the onboarding wizard).
  const imp = useConnectionImport({
    source: importSource,
    enabled: open && mode === 'import',
    onImportSuccess: (result) => {
      onClose();
      onImportSuccess(result);
    },
    onError,
  });

  const handleExportSubmit = useCallback(async () => {
    setLocalError(null);
    if (!password.trim()) {
      setLocalError(t('connShare.passwordRequired'));
      return;
    }
    if (password !== confirmPassword) {
      setLocalError(t('connShare.passwordMismatch'));
      return;
    }

    setSubmitting(true);
    try {
      const date = new Date().toISOString().slice(0, 10).replace(/-/g, '');
      const count = await connectionCommands.exportConnections(
        password,
        `datazen-connections-${date}.datazenconnection`,
      );
      onClose();
      if (count !== null) {
        onExportSuccess(count);
      }
    } catch (e) {
      onError(ipcConnectionShareError(e, t, t('common.exportFailed')));
    } finally {
      setSubmitting(false);
    }
  }, [confirmPassword, onClose, onError, onExportSuccess, password, t]);

  const primaryActionLabel =
    mode === 'export' ? t('connShare.exportAction') : imp.primaryActionLabel;

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
          <Button variant="secondary" onClick={onClose} disabled={submitting || imp.submitting}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            onClick={() => void (mode === 'export' ? handleExportSubmit() : imp.submit())}
            disabled={submitting || imp.busy}
          >
            {primaryActionLabel}
          </Button>
        </>
      }
    >
      {mode === 'export' ? (
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
          {localError && (
            <div className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
              {localError}
            </div>
          )}
        </div>
      ) : (
        <ConnectionImportFields import={imp} />
      )}
    </Dialog>
  );
}
