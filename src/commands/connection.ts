import { invoke } from '@tauri-apps/api/core';
import type { ConnectionConfig, ServerInfo } from '../types';

export const connectionCommands = {
  getConnections: () => invoke<ConnectionConfig[]>('get_connections'),

  saveConnection: (config: ConnectionConfig) =>
    invoke<void>('save_connection', { config }),

  deleteConnection: (id: string) => invoke<void>('delete_connection', { id }),

  testConnection: (config: ConnectionConfig) =>
    invoke<ServerInfo>('test_connection', { config }),

  connect: (configId: string) => invoke<string>('connect', { configId }),

  disconnect: (connectionId: string) =>
    invoke<void>('disconnect', { connectionId }),

  getGroups: () => invoke<string[]>('get_groups'),

  saveGroups: (groups: string[]) => invoke<void>('save_groups', { groups }),
};
