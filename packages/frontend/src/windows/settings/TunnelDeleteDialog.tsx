import { useEffect, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { ErrorBanner } from '../../components/ui/ErrorBanner';
import { tunnelCommands } from '../../commands/tunnel';
import { useI18n } from '../../hooks/useI18n';
import type { DeleteTunnelOutcome } from '../../lib/tunnelDeletion';
import type { SavedTunnelSummary, TunnelUsage } from '../../types';

export interface TunnelDeleteDialogProps {
  open: boolean;
  tunnel: SavedTunnelSummary | null;
  onClose: () => void;
  /**
   * Unbind every referencing connection, then delete the tunnel. Resolves with
   * the outcome so this dialog can report exactly how far it got.
   */
  onConfirm: (id: string) => Promise<DeleteTunnelOutcome>;
}

/**
 * A dialog error is either a raw IPC message or a translation key resolved at
 * render time — see `TunnelEditDialog` for why the effect must not depend on
 * the `t` function's identity.
 */
type DialogError = { message: string } | { key: string };

function toDialogError(e: unknown, fallbackKey: string): DialogError {
  if (typeof e === 'string' && e) return { message: e };
  if (e instanceof Error && e.message) return { message: e.message };
  return { key: fallbackKey };
}

/**
 * Delete confirmation with reference integrity.
 *
 * Two options only: cancel, or "delete and unbind". The affected connection
 * names are shown first so the destructive action is never silent, and the
 * unbind-then-delete ordering lives in `deleteTunnelAndUnbind` (the backend
 * `delete_tunnel` does not clean up references itself).
 */
export function TunnelDeleteDialog({ open, tunnel, onClose, onConfirm }: TunnelDeleteDialogProps) {
  const { t } = useI18n();
  const [usage, setUsage] = useState<TunnelUsage | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DialogError | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const tunnelId = tunnel?.id ?? null;

  useEffect(() => {
    if (!open || !tunnelId) return;
    let cancelled = false;
    setUsage(null);
    setError(null);
    setNote(null);
    setBusy(false);
    setLoading(true);
    void tunnelCommands
      .getTunnelUsage(tunnelId)
      .then((result) => {
        if (!cancelled) setUsage(result);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(toDialogError(e, 'settings.tunnels.delete.failed'));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, tunnelId]);

  const handleConfirm = async () => {
    if (!tunnelId || busy) return;
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      const outcome = await onConfirm(tunnelId);
      if (outcome.kind === 'deleted') {
        onClose();
        return;
      }
      if (outcome.kind === 'unbind-failed') {
        setError({
          message: `${t('settings.tunnels.delete.unbindFailed')} ${outcome.message}`,
        });
        if (outcome.rollbackFailed.length > 0) {
          setNote(
            t('settings.tunnels.delete.rollbackNote', {
              restored: outcome.rolledBack.length,
              failed: outcome.rollbackFailed.join(', '),
            }),
          );
        }
        return;
      }
      setError({
        message: `${t('settings.tunnels.delete.deleteFailed')} ${outcome.message}`,
      });
    } catch (e) {
      setError(toDialogError(e, 'settings.tunnels.delete.failed'));
    } finally {
      setBusy(false);
    }
  };

  const names = usage?.connectionNames ?? [];
  const errorText = error ? ('key' in error ? t(error.key) : error.message) : null;

  return (
    <Dialog
      open={open}
      title={t('settings.tunnels.delete.title')}
      description={t('settings.tunnels.delete.description', { name: tunnel?.name ?? '' })}
      onClose={busy ? () => {} : onClose}
      testId="tunnel-delete-dialog"
      footer={
        <>
          <Button
            variant="secondary"
            onClick={onClose}
            disabled={busy}
            data-testid="tunnel-delete-cancel"
          >
            {t('common.cancel')}
          </Button>
          {/* The unbind option is the highlighted default action. */}
          <Button
            variant="danger"
            onClick={() => void handleConfirm()}
            disabled={busy || loading}
            data-testid="tunnel-delete-confirm"
          >
            {busy ? t('settings.tunnels.delete.confirming') : t('settings.tunnels.delete.confirm')}
          </Button>
        </>
      }
    >
      <div className="space-y-3" data-testid="tunnel-delete-body">
        {loading ? (
          <p className="text-xs text-fg-muted">{t('common.loading')}</p>
        ) : names.length === 0 ? (
          <p className="text-xs text-fg-muted" data-testid="tunnel-delete-no-references">
            {t('settings.tunnels.delete.noReferences')}
          </p>
        ) : (
          <>
            <p className="text-xs text-fg-secondary" data-testid="tunnel-delete-affected-label">
              {t('settings.tunnels.delete.affected')}
            </p>
            <ul className="space-y-1" data-testid="tunnel-delete-affected">
              {names.map((name, index) => (
                <li
                  key={`${usage?.connectionIds[index] ?? name}`}
                  data-testid="tunnel-delete-affected-item"
                  className="rounded-md border border-edge bg-surface px-3 py-1.5 text-sm text-fg"
                >
                  {name}
                </li>
              ))}
            </ul>
          </>
        )}

        <p className="text-[11px] text-fg-muted">{t('settings.tunnels.delete.unbindHint')}</p>

        {errorText ? (
          <ErrorBanner as="p" data-testid="tunnel-delete-error">
            {errorText}
          </ErrorBanner>
        ) : null}
        {note ? (
          <p className="text-xs text-amber-400" data-testid="tunnel-delete-note">
            {note}
          </p>
        ) : null}
      </div>
    </Dialog>
  );
}
