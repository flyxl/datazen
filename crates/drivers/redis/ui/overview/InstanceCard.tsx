/**
 * InstanceCard — merges Server info + Memory gauge into a single full-width card.
 *
 * Layout: two-column internal split (server left, memory right) so both
 * sections share one card header, eliminating the alignment gap that
 * separate ServerInfoCard + MemoryCard create in a 2-col grid.
 *
 * The card renders its own five-state resolution via OverviewCard, using
 * the `info` source (same data source as both sub-sections).
 */
import { useI18n, cn } from '@datazen/ui';
import { OverviewCard } from './OverviewCard';
import { formatServerRowValue } from './ServerInfoCard';
import { formatSize } from '../shared/formatSize';
import type { MemoryModel, ServerOverviewRow } from './overviewModel';
import type { OverviewSourceStatus } from './useOverviewData';

const FRAG_DECIMALS = 2;

function fragText(value: number | null): string {
  if (value === null) return '—';
  return value.toFixed(FRAG_DECIMALS);
}

function percentText(value: number | null): string {
  if (value === null) return '—';
  return `${value.toFixed(1)}%`;
}

export interface InstanceCardProps {
  status: OverviewSourceStatus;
  serverRows: ServerOverviewRow[];
  memoryModel: MemoryModel;
  onRetry: () => void;
}

export function InstanceCard({ status, serverRows, memoryModel, onRetry }: InstanceCardProps) {
  const { t } = useI18n();

  const allServerMissing = serverRows.every((row) => row.value === null);
  const memoryEmpty =
    status === 'ready' && memoryModel.usedBytes === null && memoryModel.fragRatio === null;
  const isEmpty = allServerMissing && memoryEmpty;

  const barPercent = memoryModel.usedPercent ?? 0;

  return (
    <OverviewCard
      cardId="server"
      titleKey="redis.overview.server.title"
      source="info"
      status={status}
      empty={isEmpty}
      emptyKey="redis.overview.server.empty"
      unauthorizedKey="redis.overview.server.unauthorized"
      onRetry={onRetry}
    >
      <div className="grid grid-cols-1 gap-0 divide-y divide-edge/50 text-xs sm:grid-cols-2 sm:divide-x sm:divide-y-0">
        {/* Left: Server info rows */}
        <dl className="grid grid-cols-1 gap-x-4 gap-y-0.5 p-2.5">
          {serverRows.map((row) => {
            const value = formatServerRowValue(row, t);
            return (
              <div
                key={row.id}
                data-overview-server-row={row.id}
                data-overview-warn={row.warn ? 'true' : 'false'}
                className="flex items-baseline justify-between gap-3 border-b border-edge/50 py-1 last:border-b-0"
              >
                <dt className="truncate text-fg-muted">{t(row.labelKey)}</dt>
                <dd
                  data-overview-server-value={row.id}
                  className={cn(
                    'shrink-0 font-mono tabular-nums',
                    row.warn ? 'font-semibold text-warning' : 'text-fg',
                  )}
                >
                  {value ?? '—'}
                </dd>
              </div>
            );
          })}
        </dl>

        {/* Right: Memory gauge */}
        <div className="flex flex-col gap-1.5 p-2.5">
          <div data-overview-memory-gauge className="flex flex-col gap-1">
            <div className="flex items-baseline justify-between gap-2">
              <span className="text-fg-muted">{t('redis.overview.memory.used')}</span>
              <span data-overview-memory-used className="font-mono tabular-nums text-fg">
                {memoryModel.usedHuman ??
                  (memoryModel.usedBytes === null ? '—' : formatSize(memoryModel.usedBytes))}
              </span>
            </div>

            <div
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={memoryModel.unlimited ? undefined : 100}
              aria-valuenow={memoryModel.unlimited ? undefined : Math.round(barPercent)}
              aria-label={t('redis.overview.memory.title')}
              data-overview-memory-bar
              data-overview-memory-bar-state={memoryModel.unlimited ? 'unlimited' : 'bounded'}
              data-overview-memory-bar-percent={Math.round(barPercent)}
              className="h-2 w-full overflow-hidden rounded-full bg-surface-raised"
            >
              <div
                className={cn(
                  'h-full rounded-full transition-[width] duration-300',
                  !memoryModel.unlimited && barPercent >= 90 ? 'bg-danger' : 'bg-accent',
                )}
                style={{ width: `${memoryModel.unlimited ? 100 : barPercent}%` }}
              />
            </div>

            <div className="flex items-baseline justify-between gap-2">
              <span className="text-fg-muted">
                {memoryModel.unlimited
                  ? t('redis.overview.memory.maxUnlimited')
                  : t('redis.overview.memory.max')}
              </span>
              <span
                data-overview-memory-max={memoryModel.unlimited ? 'unlimited' : 'bounded'}
                className="font-mono tabular-nums text-fg-secondary"
              >
                {memoryModel.unlimited
                  ? t('redis.overview.memory.unlimited')
                  : (memoryModel.maxHuman ??
                    (memoryModel.maxBytes === null ? '—' : formatSize(memoryModel.maxBytes)))}
              </span>
            </div>

            <div className="mt-0.5 flex items-baseline justify-between gap-2">
              <span className="text-fg-muted">{t('redis.overview.memory.fragRatio')}</span>
              <span
                data-overview-memory-frag={
                  memoryModel.fragRatio === null ? 'unknown' : memoryModel.fragWarn ? 'warn' : 'ok'
                }
                className={cn(
                  'font-mono tabular-nums',
                  memoryModel.fragWarn ? 'font-semibold text-warning' : 'text-fg',
                )}
              >
                {fragText(memoryModel.fragRatio)}
              </span>
            </div>

            <div className="flex items-baseline justify-between gap-2">
              <span className="text-fg-muted">{t('redis.overview.memory.policy')}</span>
              <span
                data-overview-memory-policy={memoryModel.policy ?? 'none'}
                className="font-mono text-fg-secondary"
              >
                {memoryModel.policy ?? '—'}
              </span>
            </div>

            <div className="flex items-baseline justify-between gap-2 border-t border-edge/50 pt-1">
              <span className="text-fg-muted">{t('redis.overview.memory.usedPercent')}</span>
              <span
                data-overview-memory-percent
                className="font-mono tabular-nums text-fg-secondary"
              >
                {percentText(memoryModel.usedPercent)}
              </span>
            </div>
          </div>
        </div>
      </div>
    </OverviewCard>
  );
}
