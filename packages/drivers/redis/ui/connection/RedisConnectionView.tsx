import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useI18n } from '@datazen/ui';
import { cn } from '@datazen/ui';
import type { ConnectionViewProps } from '@datazen/driver-sdk';
import { RedisWorkbench } from '../key-browser/RedisWorkbench';
import type { RedisWorkbenchHandle } from '../key-browser/RedisWorkbench';
import { RedisConsole } from '../console/RedisConsole';
import { MonitorPanel } from '../observe/MonitorPanel';
import { PubSubPanel } from '../observe/PubSubPanel';
import { readPinnedNodeAddr } from './ClusterNodePicker';

type ActiveTab = 'items' | 'console' | 'monitor' | 'pubsub';

const TABS: ActiveTab[] = ['items', 'console', 'monitor', 'pubsub'];

const TAB_LABEL_KEYS: Record<
  ActiveTab,
  'redis.items' | 'redis.console' | 'redis.monitor' | 'redis.pubsub'
> = {
  items: 'redis.items',
  console: 'redis.console',
  monitor: 'redis.monitor',
  pubsub: 'redis.pubsub',
};

function parseRedisDbIndex(database?: string): number {
  const value = database?.trim().toLowerCase() ?? '';
  const match = /^(?:db)?(\d+)$/.exec(value);
  return match ? Number(match[1]) : 0;
}

function formatRedisDbName(database?: string): string {
  return `db${parseRedisDbIndex(database)}`;
}

export function RedisConnectionView({
  // W3 host contract: `dbSessionId` = live runtime session id.
  dbSessionId,
  connectionName,
  initialDatabase,
  hideSidebar,
  isActive = true,
  selectTableRef,
}: ConnectionViewProps) {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = useState<ActiveTab>('items');
  const [dbIndex, setDbIndex] = useState(() => parseRedisDbIndex(initialDatabase));
  const [selectedDb, setSelectedDb] = useState(() => formatRedisDbName(initialDatabase));
  const [keySuggestions, setKeySuggestions] = useState<string[]>([]);
  const [pinnedNodeAddr, setPinnedNodeAddr] = useState(() => readPinnedNodeAddr(dbSessionId));
  // Panels stay mounted once visited so tab switches never lose their state
  // (console draft/results, monitor samples, pub/sub subscriptions, …).
  const [visitedTabs, setVisitedTabs] = useState<ActiveTab[]>(['items']);
  const workbenchRef = useRef<RedisWorkbenchHandle>(null);

  useEffect(() => {
    setDbIndex(parseRedisDbIndex(initialDatabase));
    setSelectedDb(formatRedisDbName(initialDatabase));
    setKeySuggestions([]);
    setPinnedNodeAddr(readPinnedNodeAddr(dbSessionId));
  }, [dbSessionId, initialDatabase]);

  const handleSelectDatabase = useCallback((dbName: string) => {
    workbenchRef.current?.selectDatabase(dbName);
  }, []);

  const handleDatabaseChange = useCallback((dbName: string) => {
    setSelectedDb(dbName);
  }, []);

  useLayoutEffect(() => {
    if (selectTableRef && isActive) selectTableRef.current = handleSelectDatabase;
    return () => {
      if (selectTableRef && isActive) selectTableRef.current = undefined;
    };
  }, [selectTableRef, handleSelectDatabase, isActive]);

  const handleTabClick = useCallback((tab: ActiveTab) => {
    setActiveTab(tab);
    setVisitedTabs((prev) => (prev.includes(tab) ? prev : [...prev, tab]));
  }, []);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-4">
        {TABS.map((tab) => (
          <button
            key={tab}
            type="button"
            data-testid={`redis-tab-${tab}`}
            className={cn(
              'relative px-4 py-3 text-sm transition-colors',
              activeTab === tab ? 'text-fg font-medium' : 'text-fg-secondary hover:text-fg',
            )}
            onClick={() => handleTabClick(tab)}
          >
            {t(TAB_LABEL_KEYS[tab])}
            <span
              className={cn(
                'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                activeTab === tab ? 'opacity-100' : 'opacity-0',
              )}
            />
          </button>
        ))}
        <div className="flex-1" />
        <span
          className="max-w-[40%] truncate text-xs text-fg-muted"
          title={`${connectionName} · ${selectedDb}`}
          data-testid="redis-context"
        >
          {connectionName} · {selectedDb}
        </span>
      </div>

      {visitedTabs.includes('items') && (
        <div className={cn('flex min-h-0 flex-1 flex-col', activeTab !== 'items' && 'hidden')}>
          <RedisWorkbench
            ref={workbenchRef}
            dbSessionId={dbSessionId}
            initialDatabase={initialDatabase}
            hideSidebar={hideSidebar}
            onDbIndexChange={setDbIndex}
            onDatabaseChange={handleDatabaseChange}
            onKeysChange={setKeySuggestions}
          />
        </div>
      )}
      {visitedTabs.includes('console') && (
        <div className={cn('flex min-h-0 flex-1 flex-col', activeTab !== 'console' && 'hidden')}>
          <RedisConsole
            dbSessionId={dbSessionId}
            dbIndex={dbIndex}
            keySuggestions={keySuggestions}
            pinnedNodeAddr={pinnedNodeAddr}
            onPinnedNodeAddrChange={setPinnedNodeAddr}
          />
        </div>
      )}
      {visitedTabs.includes('monitor') && (
        <div className={cn('flex min-h-0 flex-1 flex-col', activeTab !== 'monitor' && 'hidden')}>
          <MonitorPanel
            dbSessionId={dbSessionId}
            dbIndex={dbIndex}
            pinnedNodeAddr={pinnedNodeAddr}
            onPinnedNodeAddrChange={setPinnedNodeAddr}
          />
        </div>
      )}
      {visitedTabs.includes('pubsub') && (
        <div className={cn('flex min-h-0 flex-1 flex-col', activeTab !== 'pubsub' && 'hidden')}>
          <PubSubPanel dbSessionId={dbSessionId} />
        </div>
      )}
    </div>
  );
}
