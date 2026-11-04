/**
 * Right panel of the Redis DB view: tabbed detail area with 键详情/命令行/发布订阅/慢日志.
 *
 * Extracted from `RedisConnectionView` during the left-right split refactoring.
 *
 * ## Unmount-on-switch with state restoration
 *
 * Only the active tab is mounted at any time — switching tabs unmounts the
 * previous panel and mounts the new one (same pattern as the host's
 * `PanelContentRenderer` which uses `key={panel.id}` + Zustand stores).
 *
 * Panels that need to survive the unmount keep their state in a module-level
 * store rather than a `useRef<Map>` snapshot cache: the console's scrollback
 * (`consoleTranscript.ts`) and the active tab (`rightTabState.ts`) both do this,
 * so a sub-tab switch or a top-level tab switch restores the same view.
 *
 * State machine (AGENTS.md):
 *  - enter: panel mounts; `activeTab` comes from the parent (module store);
 *  - state: switching tabs unmounts old panel, mounts new one;
 *  - exit: the panel is unmounted when the connection closes.
 */
import { useCallback } from 'react';
import { cn, useI18n } from '@datazen/ui';
import type { KeyDetail } from '../shared/types';
import { DetailColumn } from './DetailColumn';
import { RedisConsole } from '../console/RedisConsole';
import { PubSubPanel } from '../observe/PubSubPanel';
import { SlowlogPanel } from '../observe/SlowlogPanel';
import { requestDraftLeave } from '../shared/draftGuard';
import { useRightTab, writeRightTab, type RightTab } from '../shared/rightTabState';

const RIGHT_TABS: RightTab[] = ['detail', 'console', 'pubsub', 'slowlog'];

const RIGHT_TAB_LABEL_KEYS: Record<RightTab, string> = {
  detail: 'redis.items',
  console: 'redis.console',
  pubsub: 'redis.pubsub',
  slowlog: 'redis.slowlog',
};

export interface RedisRightPanelProps {
  dbSessionId: string;
  /**
   * Persisted connection config id (the host's `connectionId`). The Pub/Sub
   * panel filters the `redis-pubsub-message` stream on it.
   */
  connectionId?: string;
  dbIndex: number;
  /** Key currently selected in the left tree. */
  selectedKey: string | null;
  /** Fetched key detail (null when nothing selected or still loading). */
  detail: KeyDetail | null;
  detailLoading: boolean;
  modules: string[] | null;
  onRefresh: () => void;
  onRenamed: (newKey: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
  /** Console completion feed — the current key list from the left panel. */
  keySuggestions?: string[];
  /** Cluster node for console/monitor. */
  pinnedNodeAddr?: string;
  onPinnedNodeAddrChange?: (addr: string) => void;
  /** Connection name shown in the header for context. */
  connectionName?: string;
  selectedDb?: string;
  /**
   * Identity of the owning top-level tab (`shared/panelId.ts`). Per-tab UI
   * state is keyed by this and never by `dbSessionId`, which every db tab of one
   * connection shares. Required so the console below is scoped per tab too.
   */
  panelId: string;
  /** Currently active tab (controlled by parent). */
  activeTab?: RightTab;
  /** Callback when the user clicks a tab (controlled by parent). */
  onTabChange?: (tab: RightTab) => void;
}

/**
 * Tab bar + content of the right panel.  The caller is responsible for the
 * resize handle and the overall left-right split layout.
 */
export function RedisRightPanel({
  dbSessionId,
  connectionId,
  dbIndex,
  selectedKey,
  detail,
  detailLoading,
  modules,
  onRefresh,
  onRenamed,
  onDirtyChange,
  keySuggestions,
  pinnedNodeAddr,
  onPinnedNodeAddrChange,
  connectionName,
  selectedDb,
  panelId,
  activeTab: controlledTab,
  onTabChange,
}: RedisRightPanelProps) {
  const { t } = useI18n();
  // Both the controlled and the uncontrolled path read from the module-level
  // store, keyed by panel scope. The uncontrolled fallback used to be a plain
  // `useState('detail')`, which meant an uncontrolled caller silently lost its
  // tab on every remount — the same bug as the controlled path had.
  const storeTab = useRightTab(panelId);
  const activeTab = controlledTab ?? storeTab;

  const handleTabClick = useCallback(
    async (tab: RightTab) => {
      if (tab === activeTab) return;
      // I-1: switching tabs unmounts the current panel, so an unsaved draft would be
      // lost.  Ask first; the leave dialog portals to document.body so it survives
      // the unmount.
      if (!(await requestDraftLeave())) return;
      if (onTabChange) {
        onTabChange(tab);
      } else {
        writeRightTab(panelId, tab);
      }
    },
    [activeTab, onTabChange, panelId],
  );

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col" data-testid="redis-right-panel">
      {/* Tab bar */}
      {/*
        `h-10` is the shared column-header height. The key tree's R1 toolbar row
        (`KeyTreeHeader`) is pinned to the same value, so the two columns' first
        rows line up instead of differing by whatever padding each happened to
        carry. Both sides must name the literal class — a Tailwind size token
        cannot be shared through a constant and still be extracted — and
        `columnHeaderHeight.test.tsx` is what keeps the two literals in step.
      */}
      <div
        className="flex h-10 shrink-0 items-center gap-0 border-b border-edge bg-surface-alt"
        data-testid="redis-right-tab-bar"
      >
        {RIGHT_TABS.map((tab) => (
          <button
            key={tab}
            type="button"
            data-testid={`redis-right-tab-${tab}`}
            data-active={activeTab === tab ? 'true' : 'false'}
            className={cn(
              // `h-full` (not `py-*`): the active-tab underline is positioned
              // against the button's own box, so the button has to reach the
              // bar's bottom edge for the underline to sit on the border.
              'relative flex h-full items-center px-4 text-xs transition-colors',
              activeTab === tab ? 'text-fg font-medium' : 'text-fg-secondary hover:text-fg',
            )}
            onClick={() => void handleTabClick(tab)}
          >
            {t(RIGHT_TAB_LABEL_KEYS[tab])}
            <span
              className={cn(
                'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                activeTab === tab ? 'opacity-100' : 'opacity-0',
              )}
            />
          </button>
        ))}
        <div className="flex-1" />
        {connectionName && selectedDb && (
          <span
            className="max-w-[40%] truncate px-3 text-[11px] text-fg-muted"
            title={`${connectionName} · ${selectedDb}`}
            data-testid="redis-right-context"
          >
            {connectionName} · {selectedDb}
          </span>
        )}
      </div>

      {/* Tab content — only the active tab is mounted; React key forces unmount/remount */}
      <TabContent
        activeTab={activeTab}
        dbSessionId={dbSessionId}
        connectionId={connectionId}
        panelId={panelId}
        dbIndex={dbIndex}
        selectedKey={selectedKey}
        detail={detail}
        detailLoading={detailLoading}
        modules={modules}
        onRefresh={onRefresh}
        onRenamed={onRenamed}
        onDirtyChange={onDirtyChange}
        keySuggestions={keySuggestions}
        pinnedNodeAddr={pinnedNodeAddr}
        onPinnedNodeAddrChange={onPinnedNodeAddrChange}
      />
    </div>
  );
}

/**
 * Renders only the active tab panel.  Uses a `key` derived from `activeTab` so
 * React fully unmounts the old panel and mounts the new one on every switch.
 */
function TabContent({
  activeTab,
  dbSessionId,
  connectionId,
  panelId,
  dbIndex,
  selectedKey,
  detail,
  detailLoading,
  modules,
  onRefresh,
  onRenamed,
  onDirtyChange,
  keySuggestions,
  pinnedNodeAddr,
  onPinnedNodeAddrChange,
}: {
  activeTab: RightTab;
  dbSessionId: string;
  connectionId?: string;
  panelId: string;
  dbIndex: number;
  selectedKey: string | null;
  detail: KeyDetail | null;
  detailLoading: boolean;
  modules: string[] | null;
  onRefresh: () => void;
  onRenamed: (newKey: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
  keySuggestions?: string[];
  pinnedNodeAddr?: string;
  onPinnedNodeAddrChange?: (addr: string) => void;
}) {
  // The key forces React to unmount/remount on tab switch, matching the host's
  // PanelContentRenderer pattern (key={panel.id}).
  switch (activeTab) {
    case 'detail':
      return (
        <div key="detail" className="flex min-h-0 flex-1 flex-col">
          <DetailColumn
            dbSessionId={dbSessionId}
            dbIndex={dbIndex}
            selectedKey={selectedKey}
            detail={detail}
            detailLoading={detailLoading}
            modules={modules}
            onRefresh={onRefresh}
            onRenamed={onRenamed}
            onDirtyChange={onDirtyChange}
          />
        </div>
      );
    case 'console':
      return (
        <div key="console" className="flex min-h-0 flex-1 flex-col">
          <RedisConsole
            dbSessionId={dbSessionId}
            panelId={panelId}
            dbIndex={dbIndex}
            keySuggestions={keySuggestions}
            pinnedNodeAddr={pinnedNodeAddr}
            onPinnedNodeAddrChange={onPinnedNodeAddrChange}
          />
        </div>
      );
    case 'pubsub':
      return (
        <div key="pubsub" className="flex min-h-0 flex-1 flex-col">
          <PubSubPanel dbSessionId={dbSessionId} connectionId={connectionId} />
        </div>
      );
    case 'slowlog':
      return (
        <div key="slowlog" className="flex min-h-0 flex-1 flex-col">
          <SlowlogPanel dbSessionId={dbSessionId} />
        </div>
      );
    default:
      return null;
  }
}
