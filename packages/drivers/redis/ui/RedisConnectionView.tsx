import { useCallback, useEffect, useRef, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { Button } from '../../../../src/components/ui/Button';
import { useSchemaStore } from '../../../../src/stores/schemaStore';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import type { ConnectionViewProps } from '../../../../src/lib/connectionViews/types';
import { RedisWorkbench, type RedisWorkbenchHandle } from './RedisWorkbench';
import { RedisConsole } from './RedisConsole';
import { MonitorPanel } from './MonitorPanel';
import { PubSubPanel } from './PubSubPanel';
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

export function RedisConnectionView({
  connectionId,
  connectionName,
  initialDatabase,
}: ConnectionViewProps) {
  const { t } = useI18n();
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const [activeTab, setActiveTab] = useState<ActiveTab>('items');
  const [dbIndex, setDbIndex] = useState(0);
  const [keySuggestions, setKeySuggestions] = useState<string[]>([]);
  const [pinnedNodeAddr, setPinnedNodeAddr] = useState(() =>
    readPinnedNodeAddr(connectionId),
  );
  const workbenchRef = useRef<RedisWorkbenchHandle>(null);

  useEffect(() => {
    setDbIndex(0);
    setKeySuggestions([]);
    setPinnedNodeAddr(readPinnedNodeAddr(connectionId));
  }, [connectionId]);

  const handleRefresh = useCallback(() => {
    void loadForConnection(connectionId, { skipLoadTables: true });
    workbenchRef.current?.refreshKeys();
  }, [connectionId, loadForConnection]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-4">
        <Button
          variant="secondary"
          className="h-8 w-8 !px-0"
          title={t('connWin.refresh')}
          onClick={handleRefresh}
        >
          <RefreshCw className="h-4 w-4" />
        </Button>
        <div className="mx-1 h-6 w-px bg-edge" />
        {TABS.map((tab) => (
          <button
            key={tab}
            type="button"
            className={cn(
              'relative px-4 py-3 text-sm transition-colors',
              activeTab === tab
                ? 'text-fg font-medium'
                : 'text-fg-secondary hover:text-fg',
            )}
            onClick={() => setActiveTab(tab)}
          >
            {t(TAB_LABEL_KEYS[tab])}
            {activeTab === tab && (
              <span className="absolute inset-x-0 bottom-0 h-0.5 bg-accent" />
            )}
          </button>
        ))}
        <div className="flex-1" />
        <span className="text-xs text-fg-muted">{connectionName}</span>
      </div>

      {activeTab === 'items' && (
        <RedisWorkbench
          ref={workbenchRef}
          connectionId={connectionId}
          initialDatabase={initialDatabase}
          onDbIndexChange={setDbIndex}
          onKeysChange={setKeySuggestions}
        />
      )}
      {activeTab === 'console' && (
        <RedisConsole
          connectionId={connectionId}
          dbIndex={dbIndex}
          keySuggestions={keySuggestions}
          pinnedNodeAddr={pinnedNodeAddr}
          onPinnedNodeAddrChange={setPinnedNodeAddr}
        />
      )}
      {activeTab === 'monitor' && (
        <MonitorPanel
          connectionId={connectionId}
          dbIndex={dbIndex}
          pinnedNodeAddr={pinnedNodeAddr}
          onPinnedNodeAddrChange={setPinnedNodeAddr}
        />
      )}
      {activeTab === 'pubsub' && (
        <PubSubPanel connectionId={connectionId} />
      )}
    </div>
  );
}
