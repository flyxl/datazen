import { Dialog } from '../ui/Dialog';
import { ErrorBanner } from '../ui/ErrorBanner';
import { Spinner } from '../ui/Spinner';
import { useI18n } from '../../hooks/useI18n';
import type { ColumnStatsState } from '../../hooks/useColumnStats';

/**
 * The column-statistics dialog (DB-12): what the header menu's "Column
 * Stats" opens. It renders the hook's state and owns no fetching — the
 * numbers are the server's aggregates, never the loaded page (the PRD's
 * 100M-row acceptance case).
 *
 * A value the server did not report renders as an em dash: an honest
 * "no value" beats a zero that was never meant.
 */
export function ColumnStatsDialog({
  state,
  onClose,
}: {
  state: ColumnStatsState;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const open = state.status !== 'idle';
  const column = state.column ?? '';

  const formatValue = (value: unknown): string => {
    if (value == null) return '—';
    if (typeof value === 'string') return value === '' ? "''" : value;
    return String(value);
  };

  const rows: { key: string; label: string; value: string }[] = state.stats
    ? [
        {
          key: 'rows',
          label: t('columnStats.rows'),
          value: state.stats.rowCount != null ? state.stats.rowCount.toLocaleString() : '—',
        },
        {
          key: 'nonNull',
          label: t('columnStats.nonNull'),
          value: state.stats.nonNullCount != null ? state.stats.nonNullCount.toLocaleString() : '—',
        },
        {
          key: 'nulls',
          label: t('columnStats.nulls'),
          value: state.stats.nullCount != null ? state.stats.nullCount.toLocaleString() : '—',
        },
        {
          key: 'distinct',
          label: t('columnStats.distinct'),
          value:
            state.stats.distinctCount != null ? state.stats.distinctCount.toLocaleString() : '—',
        },
        { key: 'min', label: t('columnStats.min'), value: formatValue(state.stats.min) },
        { key: 'max', label: t('columnStats.max'), value: formatValue(state.stats.max) },
        ...(state.stats.supportsAvg
          ? [{ key: 'avg', label: t('columnStats.avg'), value: formatValue(state.stats.avg) }]
          : []),
      ]
    : [];

  return (
    <Dialog
      open={open}
      title={column ? `${t('columnStats.title')} — ${column}` : t('columnStats.title')}
      onClose={onClose}
      testId="column-stats-dialog"
    >
      {state.status === 'loading' && (
        <div className="flex items-center gap-2 py-6 text-sm text-fg-muted">
          <Spinner />
          {t('columnStats.loading')}
        </div>
      )}
      {state.status === 'error' && (
        <ErrorBanner>{state.error ?? t('columnStats.error')}</ErrorBanner>
      )}
      {state.status === 'ready' && (
        <div className="min-w-72">
          <dl className="text-sm">
            {rows.map((row) => (
              <div
                key={row.key}
                className="flex items-baseline justify-between gap-6 border-b border-edge/60 py-1.5 last:border-b-0"
                data-testid={`column-stats-${row.key}`}
              >
                <dt className="shrink-0 text-fg-muted">{row.label}</dt>
                <dd className="max-w-96 truncate text-right font-mono" title={row.value}>
                  {row.value}
                </dd>
              </div>
            ))}
          </dl>
          {state.stats && !state.stats.supportsAvg && (
            <p className="pt-2 text-xs text-fg-muted">{t('columnStats.avgUnsupported')}</p>
          )}
        </div>
      )}
    </Dialog>
  );
}
