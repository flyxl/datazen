import { CopyableError } from '../../components/ui/CopyableError';
import { useI18n } from '../../hooks/useI18n';

export interface TableStatusBannersProps {
  /** The panel's current error, if any. */
  error: string | null;
  /** The panel has staged work that is not committed yet. */
  hasWork: boolean;
  /** A one-off notice (e.g. "the inserted row could not be located"). */
  insertNotice: string | null;
  /** Re-read the page after a failed load. */
  onRetry: () => void;
}

/**
 * The panel's status strip above the toolbar: the error banner, the hint that
 * an error happened while changes are still staged, and the insert notice.
 *
 * Extracted verbatim from `TableView` (DB-06 track, GC budget). Every
 * `data-testid` here is consumed by the E2E specs, so it must not change.
 */
export function TableStatusBanners({
  error,
  hasWork,
  insertNotice,
  onRetry,
}: TableStatusBannersProps) {
  const { t } = useI18n();
  return (
    <>
      {error && (
        <div className="flex items-start gap-3 border-b border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-400">
          <CopyableError message={error} copyButton className="min-w-0 flex-1" />
          <button
            type="button"
            className="shrink-0 text-xs text-accent hover:underline"
            onClick={onRetry}
          >
            {t('common.retry')}
          </button>
        </div>
      )}
      {error && hasWork && (
        <div className="border-b border-warning/30 bg-warning/10 px-3 py-1.5 text-xs text-warning">
          {t('tableData.errorPendingHint')}
        </div>
      )}
      {insertNotice && (
        <div
          role="status"
          aria-live="polite"
          data-testid="table-insert-degraded"
          className="border-b border-edge bg-surface-raised px-3 py-1.5 text-xs text-fg-muted"
        >
          {insertNotice}
        </div>
      )}
    </>
  );
}
