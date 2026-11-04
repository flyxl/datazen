import { useI18n, cn } from '@datazen/ui';
import { OverviewCard } from './OverviewCard';
import { formatSize } from '../shared/formatSize';
import type { BigKeyRow, SlowlogRow } from './overviewModel';
import type { OverviewSourceStatus } from './useOverviewData';
import type { OverviewJumpHandler, OverviewJumpTarget } from './overviewNavigation';
import { jumpStateAttribute, typeBadgeClass } from './overviewNavigation';

/**
 * Combined performance card: Slow Queries + Top 5 Big Keys.
 * Merges two related performance data sources into one compact card
 * to reduce vertical space waste in the overview grid.
 */
export interface PerformanceCardProps {
  slowlogStatus: OverviewSourceStatus;
  slowlogRows: SlowlogRow[];
  memoryStatus: OverviewSourceStatus;
  bigKeys: BigKeyRow[];
  sampledDbIndex: number;
  truncated?: boolean;
  onRetry: () => void;
  onJump: (target: OverviewJumpTarget) => void;
  jumpHandler?: OverviewJumpHandler;
}

function bigKeyTtlText(row: BigKeyRow, t: (key: string) => string): string {
  if (row.missing) return t('redis.overview.memory.bigKeyGone');
  const ttl = row.ttlMs;
  if (ttl === null || (ttl < 0 && ttl !== -1)) return '—';
  if (ttl === -1) return t('redis.noExpiry');
  return `${Math.round(ttl / 1000)}${t('redis.seconds')}`;
}

export function PerformanceCard({
  slowlogStatus,
  slowlogRows,
  memoryStatus,
  bigKeys,
  sampledDbIndex,
  truncated = false,
  onRetry,
  onJump,
  jumpHandler,
}: PerformanceCardProps) {
  const { t } = useI18n();
  const jumpState = jumpStateAttribute(jumpHandler);

  const hasSlowlog = slowlogStatus === 'ready' && slowlogRows.length > 0;
  const hasBigKeys = memoryStatus === 'ready' && bigKeys.length > 0;
  const isReady = slowlogStatus === 'ready' || memoryStatus === 'ready';
  // Only report empty when BOTH sections genuinely have no data and no errors.
  // If slowlog or memory failed/unauthorized, sections show their own error state.
  const isEmpty =
    isReady &&
    !hasSlowlog &&
    !hasBigKeys &&
    slowlogStatus !== 'unauthorized' &&
    slowlogStatus !== 'failed' &&
    memoryStatus !== 'unauthorized' &&
    memoryStatus !== 'failed';

  // Determine overall status for OverviewCard — card stays 'ready' when it has
  // section-level content to show (sections handle their own empty/error states
  // internally via data-overview-bigkey-state and data-overview-slowlog-state attributes).
  let cardStatus: OverviewSourceStatus = 'ready';
  if (slowlogStatus === 'loading' && memoryStatus === 'loading') {
    cardStatus = 'loading';
  } else if (!isReady && slowlogStatus !== 'loading' && memoryStatus !== 'loading') {
    cardStatus = 'failed';
  }

  const handleSlowlogClick = () => {
    onJump({ kind: 'monitor', section: 'slowlog' });
  };

  return (
    <OverviewCard
      cardId="performance"
      titleKey="redis.overview.performance.title"
      source="slowlog_get + memory_sample"
      status={cardStatus}
      empty={isEmpty}
      emptyKey="redis.overview.performance.empty"
      unauthorizedKey="redis.overview.slowlog.unauthorized"
      onRetry={onRetry}
      className="h-full"
    >
      <div className="flex flex-col divide-y divide-edge/50 text-xs">
        {/* Slow Queries Section */}
        {hasSlowlog ? (
          <div className="p-2.5">
            <h4 className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-fg-secondary">
              {t('redis.overview.slowlog.title')}
            </h4>
            <table className="w-full border-collapse">
              <thead>
                <tr className="border-b border-edge text-left text-[10px] uppercase tracking-wide text-fg-muted">
                  <th className="w-5 px-1 py-0.5 font-medium">#</th>
                  <th className="w-20 px-1 py-0.5 font-medium">
                    {t('redis.overview.slowlog.duration')}
                  </th>
                  <th className="px-1 py-0.5 font-medium">{t('redis.overview.slowlog.command')}</th>
                  <th className="w-28 px-1 py-0.5 text-right font-medium">
                    {t('redis.overview.slowlog.client')}
                  </th>
                </tr>
              </thead>
              <tbody>
                {slowlogRows.map((row) => (
                  <tr
                    key={`${row.id}-${row.rank}`}
                    data-overview-slowlog-row={row.rank}
                    data-overview-jump={jumpState}
                    className="border-b border-edge/30 last:border-b-0 transition-colors hover:bg-surface-raised"
                    onClick={handleSlowlogClick}
                  >
                    <td className="px-1 py-0.5 font-mono text-fg-muted">{row.rank}</td>
                    <td
                      data-overview-slowlog-duration={row.rank}
                      data-overview-duration-us={row.durationUs}
                      className="px-1 py-0.5 font-mono tabular-nums text-fg"
                    >
                      {t('redis.overview.unit.microseconds', { value: row.durationUs })}
                    </td>
                    <td className="max-w-0 px-1 py-0.5">
                      <code
                        data-overview-slowlog-command={row.rank}
                        className="block truncate font-mono text-fg-secondary"
                        title={row.commandSummary}
                      >
                        {row.commandSummary || '—'}
                      </code>
                    </td>
                    <td className="px-1 py-0.5 text-right">
                      <span
                        data-overview-slowlog-client={row.rank}
                        data-overview-client-state={row.client === null ? 'unknown' : 'known'}
                        className="block truncate font-mono text-fg-muted"
                        title={row.client ?? undefined}
                      >
                        {row.client ?? t('redis.overview.slowlog.noClient')}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : slowlogStatus === 'loading' ? (
          <div className="px-2.5 py-2 text-fg-muted">{t('redis.overview.loading')}</div>
        ) : slowlogStatus === 'unauthorized' ? (
          <div
            data-overview-slowlog-state="unauthorized"
            className="flex flex-col gap-0.5 px-2.5 py-2 text-xs"
          >
            <span className="text-warning">{t('redis.overview.slowlog.unauthorized')}</span>
          </div>
        ) : slowlogStatus === 'failed' ? (
          <p data-overview-slowlog-state="failed" className="px-2.5 py-2 text-xs text-danger">
            {t('redis.overview.loadFailed')}
          </p>
        ) : null}

        {/* Big Keys Section */}
        {hasBigKeys ? (
          <div className="p-2.5">
            <div className="mb-1.5 flex items-baseline justify-between gap-2">
              <h4 className="text-[11px] font-semibold uppercase tracking-wide text-fg-secondary">
                {t('redis.overview.memory.bigKeys')}
              </h4>
              <code data-overview-memory-sample-db className="font-mono text-[10px] text-fg-muted">
                {t('redis.overview.memory.sampledDb', { db: `db${sampledDbIndex}` })}
              </code>
            </div>

            {truncated ? (
              <p data-overview-bigkey-truncated className="mb-1 text-warning">
                {t('redis.overview.memory.truncated')}
              </p>
            ) : null}

            <ul className="flex flex-col">
              {bigKeys.map((row) => (
                <li key={row.key}>
                  <button
                    type="button"
                    data-overview-bigkey={row.rank}
                    data-overview-key={row.key}
                    data-overview-db-index={sampledDbIndex}
                    data-overview-bigkey-type={row.keyType ?? 'unknown'}
                    data-overview-bigkey-ttl-ms={row.ttlMs ?? ''}
                    data-overview-bigkey-missing={row.missing ? 'true' : 'false'}
                    data-overview-jump={jumpState}
                    className="flex w-full items-baseline gap-1.5 rounded px-1 py-0.5 text-left transition-colors hover:bg-surface-raised"
                    onClick={() =>
                      onJump({
                        kind: 'key',
                        dbIndex: sampledDbIndex,
                        key: row.key,
                        keyType: row.keyType,
                      })
                    }
                  >
                    <span className="w-3.5 shrink-0 font-mono text-[10px] text-fg-muted">
                      {row.rank}
                    </span>
                    <span className="min-w-0 flex-1 truncate font-mono text-fg" title={row.key}>
                      {row.key}
                    </span>
                    <span
                      className={cn(
                        'shrink-0 rounded border px-1 py-0.5 font-mono text-[10px] leading-none',
                        typeBadgeClass(row.keyType),
                      )}
                    >
                      {row.keyType ?? t('redis.overview.typeUnknown')}
                    </span>
                    <span
                      data-overview-bigkey-bytes={row.rank}
                      className="shrink-0 font-mono tabular-nums text-fg-secondary"
                    >
                      {formatSize(row.bytes)}
                    </span>
                    <span
                      data-overview-bigkey-ttl={row.rank}
                      className={cn(
                        'w-12 shrink-0 text-right font-mono tabular-nums',
                        row.missing ? 'text-fg-muted' : 'text-fg-secondary',
                      )}
                    >
                      {bigKeyTtlText(row, t)}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ) : memoryStatus === 'loading' ? (
          <div data-overview-bigkey-state="loading" className="px-2.5 py-2 text-fg-muted">
            {t('redis.overview.loading')}
          </div>
        ) : memoryStatus === 'unauthorized' ? (
          <div
            data-overview-bigkey-state="unauthorized"
            className="flex flex-col gap-0.5 px-2.5 py-2 text-xs"
          >
            <span className="text-warning">{t('redis.overview.memory.bigKeysUnauthorized')}</span>
            <span className="text-fg-muted">
              {t('redis.overview.memory.bigKeysUnauthorizedHint')}
            </span>
          </div>
        ) : memoryStatus === 'failed' ? (
          <p data-overview-bigkey-state="failed" className="px-2.5 py-2 text-xs text-danger">
            {t('redis.overview.loadFailed')}
          </p>
        ) : memoryStatus === 'ready' && !hasBigKeys ? (
          <p data-overview-bigkey-state="empty" className="px-2.5 py-2 text-xs text-fg-muted">
            {t('redis.overview.memory.bigKeysEmpty')}
          </p>
        ) : null}
      </div>
    </OverviewCard>
  );
}
