import { useCallback, useEffect, useRef, useState } from 'react';
import { connectionCommands } from '../../commands/connection';
import { emitCrossWindow, listenCrossWindow } from '../../lib/crossWindowBus';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { useI18n } from '../../hooks/useI18n';
import { consumePendingConnection, type ConnectionTab } from './connectionPageUtils';

export function useConnectionTabs() {
  const { t } = useI18n();
  const initialPendingRef = useRef(consumePendingConnection());
  const [tabs, setTabs] = useState<ConnectionTab[]>(() => {
    return initialPendingRef.current ? [initialPendingRef.current.tab] : [];
  });
  const [activeIdx, setActiveIdx] = useState(0);
  const pendingActionRef = useRef<string | null>(null);

  const activeTab = tabs[activeIdx] ?? null;

  useEffect(() => {
    if (initialPendingRef.current?.action) {
      pendingActionRef.current = initialPendingRef.current.action;
      initialPendingRef.current = null;
    }
  }, []);

  const connectTab = useCallback(async (tab: ConnectionTab): Promise<string> => {
    const store = useActiveConnectionStore.getState();
    const existing = store.connections[tab.connectionId];
    if (existing?.status === 'connected' && existing.dbSessionId) {
      return existing.dbSessionId;
    }
    store.markConnecting(tab.connectionId, tab.initialDatabase ?? null);
    return connectionCommands.connect(tab.connectionId);
  }, []);

  const connectingTabsRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    if (tabs.length === 0) return;
    const pending = tabs.filter(
      (tab) =>
        tab.status === 'connecting' &&
        !tab.dbSessionId &&
        !connectingTabsRef.current.has(tab.connectionId),
    );
    for (const tab of pending) {
      connectingTabsRef.current.add(tab.connectionId);
      void (async () => {
        try {
          const sessionId = await connectTab(tab);
          useActiveConnectionStore.getState().markConnected(tab.connectionId, sessionId);
          setTabs((prev) =>
            prev.map((entry) =>
              entry.connectionId === tab.connectionId
                ? { ...entry, dbSessionId: sessionId, status: 'connected', error: undefined }
                : entry,
            ),
          );
          void emitCrossWindow('datazen:connection-ready', {
            connectionId: tab.connectionId,
            dbSessionId: sessionId,
          });
        } catch (e) {
          const msg =
            typeof e === 'string' ? e : e instanceof Error ? e.message : t('backend.unknownError');
          useActiveConnectionStore.getState().markError(tab.connectionId, msg);
          setTabs((prev) =>
            prev.map((entry) =>
              entry.connectionId === tab.connectionId
                ? { ...entry, status: 'error', error: msg }
                : entry,
            ),
          );
        } finally {
          connectingTabsRef.current.delete(tab.connectionId);
        }
      })();
    }
  }, [tabs, connectTab, t]);

  useEffect(() => {
    const cleanups: (() => void)[] = [];

    void listenCrossWindow('datazen:connection-ready', (payload) => {
      const data = payload as { connectionId?: string; dbSessionId?: string } | undefined;
      if (!data?.connectionId || !data?.dbSessionId) return;
      setTabs((prev) =>
        prev.map((tab) =>
          tab.connectionId === data.connectionId
            ? { ...tab, dbSessionId: data.dbSessionId!, status: 'connected', error: undefined }
            : tab,
        ),
      );
    }).then((fn) => cleanups.push(fn));

    void listenCrossWindow('datazen:connection-failed', (payload) => {
      const data = payload as { connectionId?: string; error?: string } | undefined;
      if (!data?.connectionId) return;
      setTabs((prev) =>
        prev.map((tab) =>
          tab.connectionId === data.connectionId
            ? { ...tab, status: 'error', error: data.error ?? 'Unknown error' }
            : tab,
        ),
      );
    }).then((fn) => cleanups.push(fn));

    void listenCrossWindow('datazen:disconnect-requested', (payload) => {
      const data = payload as { dbSessionId?: string } | undefined;
      if (!data?.dbSessionId) return;
      setTabs((prev) => prev.filter((entry) => entry.dbSessionId !== data.dbSessionId));
    }).then((fn) => cleanups.push(fn));

    return () => cleanups.forEach((fn) => fn());
  }, []);

  useEffect(() => {
    const HEARTBEAT_MS = 5 * 60 * 1000;
    const timer = setInterval(() => {
      for (const tab of tabs) {
        if (tab.dbSessionId && tab.status === 'connected') {
          connectionCommands.pingConnection(tab.dbSessionId).catch(() => {});
        }
      }
    }, HEARTBEAT_MS);
    return () => clearInterval(timer);
  }, [tabs]);

  return {
    tabs,
    setTabs,
    activeIdx,
    setActiveIdx,
    activeTab,
    pendingActionRef,
  };
}
