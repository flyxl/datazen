import { useI18n } from '../../hooks/useI18n';
import type { TransferTableResult } from '../../commands/transfer';
import { mappingGateBlockReason } from './transferMappingView';

/**
 * The one sentence that explains a Next button the same predicate
 * disarmed. A create-new row nobody has named keeps the user on the mapping
 * step with a reason they can act on, instead of a button that is simply off —
 * and a prepare that came back admitted over rows that no longer clear the gate
 * still refuses to advance into a preview built for other rows.
 */
export function MappingGateNotice({
  visible,
  rows,
  gateLost,
}: {
  visible: boolean;
  rows: TransferTableResult[];
  gateLost: string;
}) {
  const { t } = useI18n();
  if (!visible) return null;
  const reason = mappingGateBlockReason(rows);
  const message = gateLost || (reason ? t(reason) : '');
  if (!message) return null;
  return (
    <div
      role="alert"
      data-testid="data-transfer-mapping-gate-error"
      className="rounded-lg border border-danger/40 bg-danger/10 px-4 py-3 text-sm text-danger"
    >
      {message}
    </div>
  );
}
