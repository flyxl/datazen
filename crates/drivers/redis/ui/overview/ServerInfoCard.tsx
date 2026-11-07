import { useI18n } from '@datazen/ui';
import { cn } from '@datazen/ui';
import { OverviewCard } from './OverviewCard';
import type { ServerOverviewRow } from './overviewModel';
import type { OverviewSource } from './useOverviewData';

/**
 * 卡 1 — Server 概览（PRD §3.1：10 行两列，异常值自动 warning 色）。
 *
 * Warning rules come from the model (`evicted_keys > 0`, and the memory card owns
 * the 碎片率 > 1.5 rule), so this component only colours what it is told.
 */
export interface ServerInfoCardProps {
  status: OverviewSource<unknown>['status'];
  rows: ServerOverviewRow[];
  onRetry: () => void;
}

export function formatServerRowValue(
  row: ServerOverviewRow,
  t: (key: string, params?: Record<string, string | number>) => string,
): string | null {
  if (row.value === null) return null;
  if (row.valueIsKey) return t(row.value);
  if (row.unitKey) return t(row.unitKey, { value: row.value });
  return row.value;
}

export function ServerInfoCard({ status, rows, onRetry }: ServerInfoCardProps) {
  const { t } = useI18n();
  const allMissing = rows.every((row) => row.value === null);

  return (
    <OverviewCard
      cardId="server"
      titleKey="redis.overview.server.title"
      source="info"
      status={status}
      empty={allMissing}
      emptyKey="redis.overview.server.empty"
      unauthorizedKey="redis.overview.server.unauthorized"
      onRetry={onRetry}
    >
      <dl className="grid grid-cols-1 gap-x-4 gap-y-0.5 p-2.5 text-xs sm:grid-cols-2">
        {rows.map((row) => {
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
    </OverviewCard>
  );
}
