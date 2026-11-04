import { useCallback, useMemo, useState } from 'react';
import { X } from 'lucide-react';
import { Button, useI18n } from '@datazen/ui';
import type { ConnectionHomeSlotProps } from '@datazen/driver-sdk';
import { useOverviewData } from './useOverviewData';
import {
  BIG_KEY_LIMIT,
  SLOWLOG_LIMIT,
  buildBannerPills,
  buildBigKeyRows,
  buildMemoryModel,
  buildServerRows,
  buildSlowlogRows,
  parseInfoFieldsFromRaw,
} from './overviewModel';
import {
  parseDbIndex,
  requestOverviewJump,
  type OverviewJumpHandler,
  type OverviewJumpTarget,
} from './overviewNavigation';
import { RedisOverviewBanner } from './RedisOverviewBanner';
import { InstanceCard } from './InstanceCard';
import { PerformanceCard } from './PerformanceCard';
import { NavigationCard } from './NavigationCard';
import { clearBrowseHistory, pushBrowseEntry, readBrowseHistory } from '../lib/redisBrowseHistory';

/**概览屏 Recent Keys 最多显示条数（适配一屏布局）。 */
const RECENT_KEYS_LIMIT = 3;

/**
 * 屏 A — Redis 连接总览（`kvSlots.connectionHome` 的驱动贡献）。
 *
 * Two-row layout that fits a single viewport without scrollbars:
 *  Row 1: InstanceCard (Server + Memory side-by-side, full width)
 *  Row 2: PerformanceCard (Slowlog + Big Keys) ‖ NavigationCard (Quick Actions + Recent Keys)
 *
 * All fed by the commands in `useOverviewData`, zero SCAN.
 */
export interface RedisOverviewHomeProps extends ConnectionHomeSlotProps {
  onOpenTarget?: OverviewJumpHandler;
}

export function RedisOverviewHome({
  connectionId,
  dbSessionId,
  connectionName,
  initialDatabase,
  onOpenTarget,
}: RedisOverviewHomeProps) {
  const { t } = useI18n();
  const dbIndex = useMemo(() => parseDbIndex(initialDatabase), [initialDatabase]);
  const data = useOverviewData({ dbSessionId, dbIndex });
  const [recent, setRecent] = useState(() => readBrowseHistory(connectionId));
  const [hintKey, setHintKey] = useState<string | null>(null);

  const infoFields = useMemo(
    () => data.info.data?.fields ?? parseInfoFieldsFromRaw(''),
    [data.info.data],
  );
  const serverRows = useMemo(() => buildServerRows(infoFields), [infoFields]);
  const memoryModel = useMemo(() => buildMemoryModel(infoFields), [infoFields]);
  const bigKeys = useMemo(
    () => buildBigKeyRows(data.memory.data, BIG_KEY_LIMIT),
    [data.memory.data],
  );
  const slowlogRows = useMemo(
    () => buildSlowlogRows(data.slowlog.data, SLOWLOG_LIMIT),
    [data.slowlog.data],
  );
  const pills = useMemo(() => buildBannerPills(infoFields, memoryModel), [infoFields, memoryModel]);

  const handleJump = useCallback(
    (target: OverviewJumpTarget) => {
      const outcome = requestOverviewJump(target, onOpenTarget);
      if (outcome.handled) {
        setHintKey(null);
        if (target.kind === 'key') {
          setRecent(
            pushBrowseEntry(connectionId, {
              key: target.key,
              dbIndex: target.dbIndex,
              keyType: target.keyType ?? null,
            }),
          );
        }
        return;
      }
      setHintKey(outcome.hintKey);
    },
    [connectionId, onOpenTarget],
  );

  const handleClearRecent = useCallback(() => {
    clearBrowseHistory(connectionId);
    setRecent([]);
  }, [connectionId]);

  return (
    <div data-overview-root className="flex min-h-0 flex-1 flex-col bg-surface">
      <RedisOverviewBanner
        connectionName={connectionName}
        pills={pills}
        loading={data.info.status === 'loading'}
        onRefresh={data.refresh}
      />

      <div className="min-h-0 flex-1 overflow-hidden p-3">
        {hintKey ? (
          <div
            role="status"
            data-overview-jump-hint={hintKey}
            className="mb-3 flex items-center gap-2 rounded border border-warning/25 bg-warning/10 px-3 py-1.5 text-xs text-warning"
          >
            <span className="min-w-0 flex-1">{t(hintKey)}</span>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              data-overview-jump-hint-dismiss
              className="h-6 px-1.5"
              onClick={() => setHintKey(null)}
            >
              <X className="h-3 w-3" aria-hidden="true" />
            </Button>
          </div>
        ) : null}

        <div data-overview-grid className="grid min-h-0 grid-rows-[auto_1fr] gap-2">
          {/* 1. Instance: Server + Memory side-by-side, full width */}
          <InstanceCard
            status={data.info.status}
            serverRows={serverRows}
            memoryModel={memoryModel}
            onRetry={data.refresh}
          />

          {/* 2+3. Performance + Navigation side-by-side */}
          <div className="grid min-h-0 grid-cols-2 gap-2">
            <PerformanceCard
              slowlogStatus={data.slowlog.status}
              slowlogRows={slowlogRows}
              memoryStatus={data.memory.status}
              bigKeys={bigKeys}
              sampledDbIndex={dbIndex}
              truncated={data.memory.data?.truncated === true}
              onRetry={data.refresh}
              onJump={handleJump}
              jumpHandler={onOpenTarget}
            />
            <NavigationCard
              defaultDbIndex={dbIndex}
              recentEntries={recent.slice(0, RECENT_KEYS_LIMIT)}
              onJump={handleJump}
              onClearRecent={handleClearRecent}
              jumpHandler={onOpenTarget}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

export default RedisOverviewHome;
