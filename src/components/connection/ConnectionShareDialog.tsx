import { useCallback, useEffect, useState } from 'react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { connectionCommands } from '../../commands/connection';

export type ConnectionShareMode = 'export' | 'import';

interface ConnectionShareDialogProps {
  open: boolean;
  mode: ConnectionShareMode;
  onClose: () => void;
  onExportSuccess: (count: number) => void;
  onImportSuccess: (result: { imported: number; overwritten: number; groupsAdded: number }) => void;
  onError: (message: string) => void;
}

export function ConnectionShareDialog({
  open,
  mode,
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

  useEffect(() => {
    if (!open) {
      setPassword('');
      setConfirmPassword('');
      setLocalError(null);
      setSubmitting(false);
    }
  }, [open]);

  const handleSubmit = useCallback(async () => {
    setLocalError(null);

    if (!password.trim()) {
      setLocalError(t('connShare.passwordRequired'));
      return;
    }

    if (mode === 'export' && password !== confirmPassword) {
      setLocalError(t('connShare.passwordMismatch'));
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
    confirmPassword,
    mode,
    onClose,
    onError,
    onExportSuccess,
    onImportSuccess,
    password,
    t,
  ]);

  const title = mode === 'export' ? t('connShare.exportTitle') : t('connShare.importTitle');

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
          <Button variant="primary" onClick={() => void handleSubmit()} disabled={submitting}>
            {mode === 'export' ? t('connShare.exportAction') : t('connShare.importAction')}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
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
