import { useEffect, useState } from 'react';
import { Button } from '../ui/Button';
import { Dialog } from '../ui/Dialog';
import { ErrorBanner } from '../ui/ErrorBanner';
import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { Label } from './shared';
import type { SavedTunnel } from '../../types';

export interface SaveTunnelDialogProps {
  open: boolean;
  onClose: () => void;
  /**
   * Persist the current inline tunnel under `name`.
   * Resolves to the created entity, or `null` when persistence failed.
   */
  onSubmit: (name: string) => Promise<SavedTunnel | null>;
  busy?: boolean;
  error?: string | null;
}

/** Names the inline tunnel configuration and promotes it to a reusable entity. */
export function SaveTunnelDialog({
  open,
  onClose,
  onSubmit,
  busy = false,
  error = null,
}: SaveTunnelDialogProps) {
  const { t } = useI18n();
  const [name, setName] = useState('');
  const [submitting, setSubmitting] = useState(false);

  // Reopening always starts from a clean form.
  useEffect(() => {
    if (open) {
      setName('');
      setSubmitting(false);
    }
  }, [open]);

  const trimmed = name.trim();
  const pending = busy || submitting;

  const handleSubmit = async () => {
    if (!trimmed || pending) return;
    setSubmitting(true);
    try {
      const created = await onSubmit(trimmed);
      if (created) onClose();
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog
      open={open}
      title={t('newConn.tunnelSaveTitle')}
      description={t('newConn.tunnelSaveDesc')}
      onClose={onClose}
      footer={
        <>
          <Button
            variant="secondary"
            onClick={onClose}
            disabled={pending}
            data-testid="save-tunnel-cancel"
          >
            {t('common.cancel')}
          </Button>
          <Button
            onClick={() => void handleSubmit()}
            disabled={!trimmed || pending}
            data-testid="save-tunnel-confirm"
          >
            {pending ? t('newConn.tunnelSaving') : t('newConn.tunnelSaveConfirm')}
          </Button>
        </>
      }
    >
      <Label required>{t('newConn.tunnelName')}</Label>
      <Input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder={t('newConn.tunnelNamePlaceholder')}
        data-testid="save-tunnel-name"
      />
      {error ? (
        <ErrorBanner variant="boxed" className="mt-3" data-testid="save-tunnel-error">
          {error}
        </ErrorBanner>
      ) : null}
    </Dialog>
  );
}
