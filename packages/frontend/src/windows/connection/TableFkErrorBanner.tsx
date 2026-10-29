import { useMemo } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { CopyableError } from '../../components/ui/CopyableError';
import type { GridErrorCode } from '../../lib/gridErrors';

/**
 * The inline error of a refused foreign-key jump (DB-07).
 *
 * A refused jump opens **no** tab: the tab it started from says why. The
 * message has two parts:
 *
 * - the **conclusion** of the classified reason ("the foreign key changed;
 *   refresh the table structure") — this is what the user can act on;
 * - the **original** message of the host — this is what a report can carry.
 *
 * An unclassified message is shown alone: replacing it with a generic
 * sentence would throw away the only information the user has (00-contracts
 * §8.1).
 */
export interface TableFkErrorBannerProps {
  /** The refused jump; `null` when there was none. */
  error: { code: GridErrorCode | 'unknown'; raw: string } | null;
  /** Hide the error. */
  onDismiss: () => void;
}

export function TableFkErrorBanner({ error, onDismiss }: TableFkErrorBannerProps) {
  const { t } = useI18n();
  const message = useMemo(() => {
    if (!error) return null;
    if (error.code === 'unknown') return error.raw;
    const reason = error.code.startsWith('grid.fk.') ? error.code.slice('grid.fk.'.length) : '';
    const title = reason ? t(`fkNav.error.${reason}`) : '';
    return title ? `${title}\n${error.raw}` : error.raw;
  }, [error, t]);

  if (!message) return null;
  return (
    <div
      className="flex shrink-0 items-start gap-2 border-b border-danger/30 bg-danger/10 px-3 py-1.5"
      data-testid="table-fk-error"
    >
      <CopyableError message={message} copyButton className="text-xs text-red-400" />
      <button
        type="button"
        className="ml-auto shrink-0 text-xs text-accent hover:underline"
        onClick={onDismiss}
      >
        {t('common.close')}
      </button>
    </div>
  );
}
