import { RefreshCw } from 'lucide-react';
import { Button, useI18n } from '@datazen/ui';
import type { DatabaseTypeMeta } from '@datazen/driver-sdk';
import type { BannerPill } from './overviewModel';
import { redisMeta } from '../shared/meta';
import redisIcon from '../icons/redis.svg';

/**
 * 屏 A 头部横幅 —— 压缩到 48px（h-12）。
 *
 * Layout: [icon] [name] [pills…] ··························· [Refresh]
 * Height is fixed at `h-12` = 48px.
 */
export interface RedisOverviewBannerProps {
  connectionName: string;
  pills: BannerPill[];
  loading: boolean;
  meta?: Pick<DatabaseTypeMeta, 'iconBg' | 'iconColor' | 'label'>;
  /** Global refresh callback — single refresh for all overview cards. */
  onRefresh?: () => void;
}

const PILL_LABEL: Record<BannerPill['id'], string> = {
  version: 'redis.overview.pill.version',
  mode: 'redis.overview.pill.mode',
  usedMemory: 'redis.overview.pill.usedMemory',
};

export function RedisOverviewBanner({
  connectionName,
  pills,
  loading,
  meta = redisMeta,
  onRefresh,
}: RedisOverviewBannerProps) {
  const { t } = useI18n();

  return (
    <header
      data-overview-banner
      className="flex h-12 shrink-0 items-center gap-3 border-b border-edge bg-surface-alt px-4"
    >
      <img
        data-overview-banner-icon
        src={redisIcon}
        alt={meta.label ?? 'Redis'}
        className="h-8 w-8 shrink-0"
      />

      <span
        data-overview-banner-name
        className="truncate text-sm font-semibold text-fg"
        title={connectionName}
      >
        {connectionName}
      </span>

      <div className="flex min-w-0 items-center gap-1.5 overflow-x-auto">
        {pills.map((pill) => (
          <span
            key={pill.id}
            data-overview-pill={pill.id}
            className="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-edge bg-surface px-2.5 text-[11px] text-fg-secondary"
          >
            <span className="text-fg-muted">{t(PILL_LABEL[pill.id])}</span>
            <span data-overview-pill-value={pill.id} className="font-mono font-semibold text-fg">
              {pill.value ?? '—'}
            </span>
          </span>
        ))}
      </div>

      <span className="flex-1" />

      {onRefresh ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          data-overview-banner-refresh
          className="h-7 gap-1.5 px-2 text-[11px]"
          onClick={onRefresh}
          disabled={loading}
        >
          <RefreshCw className={loading ? 'h-3 w-3 animate-spin' : 'h-3 w-3'} />
          {t('redis.overview.refresh')}
        </Button>
      ) : null}
    </header>
  );
}
