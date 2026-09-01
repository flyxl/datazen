import { invoke } from '@tauri-apps/api/core';
import { toIpcConnectionConfig } from '../lib/connectionConfig';
import type { ConnectionConfig, DriverCapabilities, ServerInfo } from '../types';

async function waitForE2EConnectDelay(connectionId: string): Promise<void> {
  if (!import.meta.env.VITE_E2E || typeof window === 'undefined') return;
  const delays = (
    window as Window & {
      __DATAZEN_E2E_CONNECT_DELAY_MS__?: Record<string, number>;
    }
  ).__DATAZEN_E2E_CONNECT_DELAY_MS__;
  const delayMs = delays?.[connectionId] ?? 0;
  if (!Number.isFinite(delayMs) || delayMs <= 0) return;
  await new Promise<void>((resolve) => window.setTimeout(resolve, delayMs));
}

export const connectionCommands = {
  getConnections: () => invoke<ConnectionConfig[]>('get_connections'),

  saveConnection: (config: ConnectionConfig) =>
    invoke<void>('save_connection', { config: toIpcConnectionConfig(config) }),

  deleteConnection: (id: string) => invoke<void>('delete_connection', { id }),

  reorderConnections: (orderedIds: string[]) => invoke<void>('reorder_connections', { orderedIds }),

  testConnection: (config: ConnectionConfig) =>
    invoke<ServerInfo>('test_connection', { config: toIpcConnectionConfig(config) }),

  /** connectionId = 持久化配置连接 id；返回值为运行时 dbSessionId。 */
  connect: async (connectionId: string) => {
    await waitForE2EConnectDelay(connectionId);
    return invoke<string>('connect', { connectionId });
  },

  /** Sub-window only: always opens a new db session, optionally pinned to `database`. */
  connectDedicated: (connectionId: string, database?: string) =>
    invoke<string>('connect_dedicated', { connectionId, database: database ?? null }),

  pingConnection: (dbSessionId: string) => invoke<boolean>('ping_connection', { dbSessionId }),

  releaseConnection: (dbSessionId: string) =>
    invoke<boolean>('release_connection', { dbSessionId }),

  disconnect: (dbSessionId: string) => invoke<void>('disconnect', { dbSessionId }),

  getConnectionInfo: (dbSessionId: string) =>
    invoke<{
      databaseType: string;
      driverCategory: string;
      name: string;
      host?: string;
      port?: number;
      database?: string;
      schema?: string;
      serverVersion?: string;
      /** Null/absent means the backend or legacy driver did not advertise it. */
      capabilities?: DriverCapabilities | null;
    }>('get_connection_info', { dbSessionId }),

  getAvailableDrivers: () => invoke<string[]>('get_available_drivers'),

  getGroups: () => invoke<string[]>('get_groups'),

  saveGroups: (groups: string[]) => invoke<void>('save_groups', { groups }),

  /**
   * Decision 3 merged IPC: native save dialog + encrypted export.
   * The wire-level `override_path` escape hatch is webdriver/E2E-only and is
   * never sent from production code. Returns the connection count, or null
   * when the dialog was dismissed.
   */
  exportConnections: (password: string, defaultFileName: string) =>
    invoke<number | null>('export_connections', { password, defaultFileName }),

  importConnectionsPreview: (password: string) =>
    invoke<{
      connections: ConnectionConfig[];
      groups: string[];
    } | null>('import_connections_preview', { password }),

  /**
   * Legacy merged IPC (password then file). Prefer pickConnectionsImportFile +
   * importConnectionsAtPath for the TablePlus-style file-first flow.
   */
  importConnections: (password: string) =>
    invoke<{
      imported: number;
      overwritten: number;
      groupsAdded: number;
      skipped?: string[];
      sourceFormat?: string;
    } | null>('import_connections_with_dialog', { password }),

  /** Native open dialog only; returns the picked path or null if cancelled. */
  pickConnectionsImportFile: () => invoke<string | null>('pick_connections_import_file'),

  /** Import after the file has already been chosen in the UI. */
  importConnectionsAtPath: (password: string, path: string) =>
    invoke<{
      imported: number;
      overwritten: number;
      groupsAdded: number;
      skipped?: string[];
      sourceFormat?: string;
    }>('import_connections_at_path', { password, path }),

  detectConnectionImportPath: (source: string) =>
    invoke<{ path: string; found: boolean }>('detect_connection_import_path', { source }),

  pickConnectionImportPathWithDialog: (mode: 'file' | 'folder', source: string) =>
    invoke<string | null>('pick_connection_import_path_with_dialog', { mode, source }),

  importConnectionsFromApp: (source: string, password: string, dataPath: string) =>
    invoke<{
      imported: number;
      overwritten: number;
      groupsAdded: number;
      skipped?: string[];
      sourceFormat?: string;
    }>('import_connections_from_app', { source, password, dataPath }),
};
