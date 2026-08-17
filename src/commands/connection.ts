import { invoke } from '@tauri-apps/api/core';
import type { ConnectionConfig, ServerInfo } from '../types';

export const connectionCommands = {
  getConnections: () => invoke<ConnectionConfig[]>('get_connections'),

  saveConnection: (config: ConnectionConfig) => invoke<void>('save_connection', { config }),

  deleteConnection: (id: string) => invoke<void>('delete_connection', { id }),

  testConnection: (config: ConnectionConfig) => invoke<ServerInfo>('test_connection', { config }),

  connect: (configId: string) => invoke<string>('connect', { configId }),

  pingConnection: (connectionId: string) => invoke<boolean>('ping_connection', { connectionId }),

  releaseConnection: (connectionId: string) =>
    invoke<boolean>('release_connection', { connectionId }),

  disconnect: (connectionId: string) => invoke<void>('disconnect', { connectionId }),

  getConnectionInfo: (connectionId: string) =>
    invoke<{
      databaseType: string;
      driverCategory: string;
      name: string;
      host?: string;
      port?: number;
      database?: string;
      schema?: string;
      serverVersion?: string;
    }>('get_connection_info', { connectionId }),

  getAvailableDrivers: () => invoke<string[]>('get_available_drivers'),

  getGroups: () => invoke<string[]>('get_groups'),

  saveGroups: (groups: string[]) => invoke<void>('save_groups', { groups }),

  /** @deprecated Legacy path IPC; gated to webdriver builds. Prefer exportConnectionsWithDialog. */
  exportConnections: (path: string, password: string) =>
    invoke<number>('export_connections', { path, password }),

  exportConnectionsWithDialog: (password: string, defaultFileName: string) =>
    invoke<number | null>('export_connections_with_dialog', { password, defaultFileName }),

  /** @deprecated Legacy path IPC; gated to webdriver builds. Prefer importConnectionsWithDialog. */
  importConnectionsPreview: (path: string, password: string) =>
    invoke<{ connections: ConnectionConfig[]; groups: string[] }>('import_connections_preview', {
      path,
      password,
    }),

  importConnectionsWithDialog: (password: string) =>
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
