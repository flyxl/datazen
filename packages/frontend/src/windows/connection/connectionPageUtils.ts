import { useSchemaStore } from '../../stores/schemaStore';
import { PENDING_CONNECTION_KEY } from '../../lib/windowManager';
import type { DatabaseType } from '../../types';

export type WorkspaceMode = 'connections' | 'workflow' | 'dashboard' | 'workspace' | 'extension';
export type MainView = 'workspace' | 'settings';

export interface ConnectionTab {
  /** Persistent connection id (saved connection this tab was opened from). */
  connectionId: string;
  /** Live database session id ('' while connecting). */
  dbSessionId: string;
  connectionName: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
  status: 'connecting' | 'connected' | 'error';
  error?: string;
}

export function makeTabFromPayload(data: Record<string, string>): ConnectionTab | null {
  const connectionId = data.connectionId ?? '';
  if (!connectionId) return null;
  const dbSessionId = data.dbSessionId ?? '';
  return {
    connectionId,
    dbSessionId,
    connectionName: data.connectionName ?? '',
    databaseType: (data.databaseType ?? 'postgresql') as DatabaseType,
    initialDatabase: data.database,
    status: dbSessionId ? 'connected' : 'connecting',
  };
}

export interface ConnectionLookup {
  id: string;
  name: string;
  databaseType: string;
  database?: string | null;
}

/** Merge a host startup event that only knows the connection id with the store
 * record, so the existing tab/query UI receives a complete payload. */
export function enrichOpenConnectionPayload(
  data: Record<string, string>,
  connections: readonly ConnectionLookup[],
): Record<string, string> {
  const connection = connections.find((item) => item.id === data.connectionId);
  if (!connection) return data;
  const enriched: Record<string, string> = {
    ...data,
    connectionName: data.connectionName || connection.name,
    databaseType: data.databaseType || connection.databaseType,
  };
  if (!enriched.database && connection.database) {
    enriched.database = connection.database;
  }
  return enriched;
}

export interface PendingOpenQuery {
  connectionId: string;
  sql: string;
  requestId?: string;
}

/** SQL that a pending host open request should preload, or null when the
 * connection is not ready or a query panel already exists. */
export function pendingOpenQuerySql(
  pending: PendingOpenQuery | null,
  tabs: readonly Pick<ConnectionTab, 'connectionId' | 'status'>[],
  panels: readonly Pick<{ connectionId: string }, 'connectionId'>[],
): string | null {
  if (!pending) return null;
  const connected = tabs.some(
    (tab) => tab.connectionId === pending.connectionId && tab.status === 'connected',
  );
  if (!connected) return null;
  if (panels.some((panel) => panel.connectionId === pending.connectionId)) return null;
  return pending.sql;
}

export function consumePendingConnection(): { tab: ConnectionTab; action?: string } | null {
  try {
    const raw = localStorage.getItem(PENDING_CONNECTION_KEY);
    localStorage.removeItem(PENDING_CONNECTION_KEY);
    if (!raw) return null;
    const data = JSON.parse(raw) as Record<string, string>;
    const tab = makeTabFromPayload(data);
    if (!tab) return null;
    return { tab, action: data.action };
  } catch {
    return null;
  }
}

export function syncStoresActiveConnection(dbSessionId: string | null) {
  useSchemaStore.getState().setActiveConnection(dbSessionId);
}

export function removeConnectionFromStores(dbSessionId: string) {
  useSchemaStore.getState().removeConnection(dbSessionId);
}
