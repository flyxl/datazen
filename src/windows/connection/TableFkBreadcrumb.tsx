import { ChevronLeft, ChevronRight, X } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import type { FkOrigin } from '../../stores/panelTypes';
import type { FkBreadcrumbModel } from '../../lib/fkNavigation';

/**
 * The breadcrumb of a tab that was reached by following a foreign key (DB-07).
 *
 * The tab bar shows the **table name**, so a user with four tabs open cannot
 * tell which of them came from where. This bar answers that: where the jump
 * came from, and how to go back.
 *
 * It is a **pure presentation**: the model comes in, the buttons go out. The
 * navigation stack itself lives in the store, and the tab bar is not a
 * history — it cannot show one.
 */
export interface TableFkBreadcrumbProps {
  /** Where the jump came from. */
  origin: FkOrigin;
  /** The stack dropped entries because it grew too deep. */
  truncated: boolean;
  /** There is a jump to go back to. */
  canGoBack: boolean;
  /** There is a jump to go forward to. */
  canGoForward: boolean;
  /** Go back one jump. */
  onBack: () => void;
  /** Go forward one jump. */
  onForward: () => void;
  /** Hide the breadcrumb (the tab stays, the history does not). */
  onDismiss?: () => void;
}

/**
 * The "from" text of the breadcrumb: `orders.customer_id → customers.id`.
 *
 * A composite key names **all** of its columns on both sides, because
 * "orders → customers" alone would not say which row is meant.
 */
export function fkOriginLabel(model: FkBreadcrumbModel): string {
  const side = (table: string, columns: readonly string[]): string => {
    if (columns.length === 0) return table;
    if (columns.length === 1) return `${table}.${columns[0]}`;
    return `${table}.(${columns.join(', ')})`;
  };
  return `${side(model.sourceTable, model.localColumns)} → ${side(model.targetTable, model.targetColumns)}`;
}

export function TableFkBreadcrumb({
  origin,
  truncated,
  canGoBack,
  canGoForward,
  onBack,
  onForward,
  onDismiss,
}: TableFkBreadcrumbProps) {
  const { t } = useI18n();
  const label = fkOriginLabel({
    sourceTable: origin.sourceTable,
    localColumns: origin.localColumns,
    targetTable: origin.targetTable,
    targetColumns: origin.targetColumns,
  });
  return (
    <div
      data-testid="table-fk-breadcrumb"
      className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-raised/60 px-3 py-1.5 text-xs"
    >
      <span className="truncate text-fg-secondary" data-testid="table-fk-breadcrumb-label">
        {t('fkNav.breadcrumbFrom', { source: label })}
      </span>
      {truncated && (
        <span className="text-fg-muted" data-testid="table-fk-breadcrumb-truncated">
          {t('fkNav.stackTruncated')}
        </span>
      )}
      <div className="ml-auto flex items-center gap-1">
        <button
          type="button"
          data-testid="table-fk-back"
          disabled={!canGoBack}
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-fg-secondary hover:bg-surface hover:text-accent disabled:opacity-40"
          title={t('fkNav.backToSource', { table: origin.sourceTable })}
          aria-label={t('fkNav.back')}
          onClick={onBack}
        >
          <ChevronLeft className="h-3 w-3" />
          {t('fkNav.back')}
        </button>
        <button
          type="button"
          data-testid="table-fk-forward"
          disabled={!canGoForward}
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-fg-secondary hover:bg-surface hover:text-accent disabled:opacity-40"
          title={t('fkNav.forward')}
          aria-label={t('fkNav.forward')}
          onClick={onForward}
        >
          <ChevronRight className="h-3 w-3" />
          {t('fkNav.forward')}
        </button>
        {onDismiss && (
          <button
            type="button"
            data-testid="table-fk-dismiss"
            className="flex h-4 w-4 items-center justify-center rounded text-fg-muted hover:bg-surface hover:text-fg"
            title={t('common.close')}
            aria-label={t('common.close')}
            onClick={onDismiss}
          >
            <X className="h-3 w-3" />
          </button>
        )}
      </div>
    </div>
  );
}
