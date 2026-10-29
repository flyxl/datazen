import { ShieldAlert } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { readOnlyReasonMessageKey, type CellReadOnlyReason } from '../../lib/cellEditorKind';

/**
 * The notice that tells the user why a cell cannot be edited (DB-05).
 *
 * "Read-only" on its own leaves the user guessing: a read-only connection,
 * a view and a table without a primary key are three different problems
 * with three different fixes. The reason is therefore part of the notice.
 */
export interface TableReadOnlyTipProps {
  /** Why the cell cannot be edited. */
  readonly reason: CellReadOnlyReason;
}

export function TableReadOnlyTip({ reason }: TableReadOnlyTipProps) {
  const { t } = useI18n();
  return (
    <div
      className="flex shrink-0 items-center gap-1.5 border-b border-warning/30 bg-warning/10 px-3 py-1.5 text-xs text-warning"
      role="status"
      aria-live="polite"
      data-testid="table-read-only-tip"
    >
      <ShieldAlert className="h-3.5 w-3.5 shrink-0" />
      {t('tableData.readOnlyEditDisabled')}
      <span data-testid="table-read-only-reason">{t(readOnlyReasonMessageKey(reason))}</span>
    </div>
  );
}
