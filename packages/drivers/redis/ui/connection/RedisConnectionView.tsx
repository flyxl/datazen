/**
 * Redis connection view: left-right split layout.
 *
 * Layout (post-refactoring to match prototype):
 *  - Left: Key browser (RedisWorkbench with renderRightPanel override)
 *  - Right: Tabbed detail panel (键详情 / 命令行 / 发布订阅 / 慢日志)
 *
 * The top-level tabs are gone; the right panel owns its own tab bar.  The key
 * browser is always visible on the left, giving the user constant access to the
 * key tree regardless of which right-panel tab is active.
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';

import type { ConnectionViewProps } from '@datazen/driver-sdk';
import { RedisWorkbench } from '../key-browser/RedisWorkbench';
import type { RedisWorkbenchHandle } from '../key-browser/RedisWorkbench';
import { RedisRightPanel } from '../key-browser/RedisRightPanel';
import { useRightTab, writeRightTab, type RightTab } from '../shared/rightTabState';
import { bindPanelClose } from '../shared/panelLifecycle';
import { readPinnedNodeAddr } from './ClusterNodePicker';
import type { RedisPendingAction } from '../overview/overviewNavigation';

function parseRedisDbIndex(database?: string): number {
  const value = database?.trim().toLowerCase() ?? '';
  const match = /^(?:db)?(\d+)$/.exec(value);
  return match ? Number(match[1]) : 0;
}

function formatRedisDbName(database?: string): string {
  return `db${parseRedisDbIndex(database)}`;
}

/**
 * Map legacy pending-action tab ids to the new right-panel tab ids.
 *  - 'items' → 'detail' (键详情)
 *  - 'console' → 'console'
 *  - 'pubsub' → 'pubsub'
 *  - 'monitor' → 'slowlog' when monitorSubPage is 'slowlog', else 'detail'
 */
function mapPendingTab(
  tab: RedisPendingAction['tab'],
  monitorSubPage?: RedisPendingAction['monitorSubPage'],
): RightTab {
  if (tab === 'console') return 'console';
  if (tab === 'pubsub') return 'pubsub';
  if (tab === 'monitor' && monitorSubPage === 'slowlog') return 'slowlog';
  return 'detail';
}

interface RedisConnectionViewExtraProps {
  /**
   * One-shot action from the host (overview → panel jump bridge).  Consumed
   * exactly once on mount / first render so repeated renders never replay it.
   */
  pendingAction?: RedisPendingAction;
}

export function RedisConnectionView({
  panelId,
  onPanelClosed,
  dbSessionId,
  connectionId,
  connectionName,
  initialDatabase,
  hideSidebar,
  isActive = true,
  selectTableRef,
  kvSlotState,
  pendingAction,
}: ConnectionViewProps & RedisConnectionViewExtraProps) {
  const [dbIndex, setDbIndex] = useState(() => parseRedisDbIndex(initialDatabase));
  const [selectedDb, setSelectedDb] = useState(() => formatRedisDbName(initialDatabase));
  const [keySuggestions, setKeySuggestions] = useState<string[]>([]);
  const [pinnedNodeAddr, setPinnedNodeAddr] = useState(() => readPinnedNodeAddr(dbSessionId));
  const workbenchRef = useRef<RedisWorkbenchHandle>(null);
  const pendingActionRef = useRef<RedisPendingAction | undefined>(pendingAction);
  // Right-panel tab state — the pending action may switch the active tab.
  // Module-level store, not useState: the host unmounts this whole subtree on a
  // top-level tab switch, and component state would reset the user to 'detail'.
  // Keyed by `panelId`, NOT `dbSessionId`: every db tab of one connection shares
  // that session id, so it would leak this tab's sub-tab to a sibling db.
  const rightTab = useRightTab(panelId);
  const setRightTab = useCallback((tab: RightTab) => writeRightTab(panelId, tab), [panelId]);

  // Release this tab's state when the host closes it. Survives unmount on
  // purpose — see `shared/panelLifecycle`.
  useEffect(() => bindPanelClose(panelId, onPanelClosed), [panelId, onPanelClosed]);

  // Sync the prop into the ref so that when the host calls `updatePanel` with a
  // new pendingAction on an EXISTING panel, the consumption effect picks it up.
  useEffect(() => {
    if (pendingAction) pendingActionRef.current = pendingAction;
  }, [pendingAction]);

  // ── Phase 1: consume tab switch + stash key selection for phase 2 ──
  const pendingKeyRef = useRef<{ key: string; dbIndex: number } | null>(null);

  useEffect(() => {
    const action = pendingActionRef.current;
    if (!action) return;
    pendingActionRef.current = undefined;

    // Tab switch in the right panel.
    if (action.tab) {
      const targetTab = mapPendingTab(action.tab, action.monitorSubPage);
      setRightTab(targetTab);
    }

    // Key selection — stash so phase 2 picks it up once the workbench is ready.
    if (action.selectKey) {
      pendingKeyRef.current = { key: action.selectKey, dbIndex: action.keyDbIndex ?? 0 };
    }
  }, [pendingAction]); // Re-run when the host sends a new pending action.

  // ── Phase 2: key selection after workbench mount ──
  useEffect(() => {
    if (!pendingKeyRef.current) return;
    if (!workbenchRef.current) return;
    const { key, dbIndex: keyDbIndex } = pendingKeyRef.current;
    pendingKeyRef.current = null;
    workbenchRef.current.selectKey(key, keyDbIndex);
  }, [dbIndex]); // Re-check after dbIndex update mounts the workbench.

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

  /** Delegate right-panel tab switching to RedisRightPanel via callback. */
  const handleRightTabChange = useCallback(
    (tab: RightTab) => {
      setRightTab(tab);
    },
    [setRightTab],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1">
        {/* ── Left: key browser (always visible) ──────────────────────── */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <RedisWorkbench
            ref={workbenchRef}
            dbSessionId={dbSessionId}
            connectionId={connectionId}
            initialDatabase={initialDatabase}
            hideSidebar={hideSidebar}
            onDbIndexChange={setDbIndex}
            onDatabaseChange={handleDatabaseChange}
            onKeysChange={setKeySuggestions}
            kvSlotState={kvSlotState}
            pendingAction={pendingAction}
            renderRightPanel={(detailProps) => (
              <RedisRightPanel
                {...detailProps}
                keySuggestions={keySuggestions}
                pinnedNodeAddr={pinnedNodeAddr}
                onPinnedNodeAddrChange={setPinnedNodeAddr}
                connectionName={connectionName}
                selectedDb={selectedDb}
                panelId={panelId}
                activeTab={rightTab}
                onTabChange={handleRightTabChange}
              />
            )}
          />
        </div>
      </div>
    </div>
  );
}
