import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';

/** Shown on the Transfer endpoints step only when the selected pair is unsupported. */
export function TransferPairingNote({ reason }: { reason?: string | null }) {
  const { t } = useI18n();
  return (
    <p
      data-testid="data-transfer-path"
      className={cn(
        'mt-4 inline-block rounded border border-edge bg-surface px-2 py-1 text-xs text-fg-muted',
      )}
    >
      {reason ?? t('transfer.unsupportedPair')}
    </p>
  );
}
