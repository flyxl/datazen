import { useSchemaStore } from '../../stores/schemaStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { PENDING_CONNECTION_KEY } from '../../lib/windowManager';
import type { DatabaseType } from '../../types';

export type WorkspaceMode = 'connections' | 'workflow' | 'dashboard' | 'workspace' | 'plugins';
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
  useTableDataStore.getState().setActiveConnection(dbSessionId);
}

export function removeConnectionFromStores(dbSessionId: string) {
  useSchemaStore.getState().removeConnection(dbSessionId);
  useTableDataStore.getState().removeConnection(dbSessionId);
}
