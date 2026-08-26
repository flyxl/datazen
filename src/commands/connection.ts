import { invoke } from '@tauri-apps/api/core';
import type { ConnectionConfig, ServerInfo } from '../types';

export const connectionCommands = {
  getConnections: () => invoke<ConnectionConfig[]>('get_connections'),

  saveConnection: (config: ConnectionConfig) => invoke<void>('save_connection', { config }),

  deleteConnection: (id: string) => invoke<void>('delete_connection', { id }),

  reorderConnections: (orderedIds: string[]) => invoke<void>('reorder_connections', { orderedIds }),

  testConnection: (config: ConnectionConfig) => invoke<ServerInfo>('test_connection', { config }),

  /** connectionId = 持久化配置连接 id；返回值为运行时 dbSessionId。 */
  connect: (connectionId: string) => invoke<string>('connect', { connectionId }),

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
   * Decision 3 merged IPC: native open dialog + decrypt/merge import.
   * The wire-level `override_path` escape hatch is webdriver/E2E-only and is
   * never sent from production code. Returns import stats, or null when the
   * dialog was dismissed.
   */
  importConnections: (password: string) =>
    invoke<{
      imported: number;
      overwritten: number;
      groupsAdded: number;
      skipped?: string[];
      sourceFormat?: string;
    } | null>('import_connections_with_dialog', { password }),

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
