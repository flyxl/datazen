import { create } from 'zustand';
import { connectionCommands } from '../commands/connection';
import { emitCrossWindow } from '../lib/crossWindowBus';
import type { ConnectionConfig, ServerInfo } from '../types';

const EVENT_CONNECTIONS_CHANGED = 'datazen:connections-changed';

export function filterConnections(
  connections: ConnectionConfig[],
  selectedGroup: string | null,
  searchQuery: string,
): ConnectionConfig[] {
  const q = searchQuery.trim().toLowerCase();
  return connections.filter((c) => {
    if (selectedGroup && c.group !== selectedGroup) return false;
    if (!q) return true;
    const hay = `${c.name} ${c.host ?? ''} ${c.database ?? ''} ${c.databaseType}`.toLowerCase();
    return hay.includes(q);
  });
}

interface ConnectionStore {
  connections: ConnectionConfig[];
  groups: string[];
  selectedGroup: string | null;
  searchQuery: string;
  loading: boolean;
  error: string | null;

  fetchConnections: () => Promise<void>;
  fetchGroups: () => Promise<void>;
  saveConnection: (config: ConnectionConfig) => Promise<void>;
  duplicateConnection: (id: string) => Promise<void>;
  deleteConnection: (id: string) => Promise<void>;
  testConnection: (config: ConnectionConfig) => Promise<ServerInfo>;
  addGroup: (name: string) => Promise<void>;
  setSelectedGroup: (group: string | null) => void;
  setSearchQuery: (query: string) => void;
}

export const useConnectionStore = create<ConnectionStore>((set, get) => ({
  connections: [],
  groups: [],
  selectedGroup: null,
  searchQuery: '',
  loading: false,
  error: null,

  fetchConnections: async () => {
    set({ loading: true, error: null });
    try {
      const connections = await connectionCommands.getConnections();
      set({ connections, loading: false });
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : '加载连接失败',
      });
    }
  },

  fetchGroups: async () => {
    try {
      const groups = await connectionCommands.getGroups();
      set({ groups });
    } catch (e) {
      console.error('[connectionStore] fetchGroups failed', e);
    }
  },

  saveConnection: async (config) => {
    set({ loading: true, error: null });
    try {
      await connectionCommands.saveConnection(config);
      await get().fetchConnections();
      await get().fetchGroups();
      void emitCrossWindow(EVENT_CONNECTIONS_CHANGED);
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : '保存连接失败',
      });
    }
  },

  duplicateConnection: async (id) => {
    const { connections } = get();
    const source = connections.find((c) => c.id === id);
    if (!source) return;
    const copy: ConnectionConfig = {
      ...source,
      id: `conn_${Math.random().toString(36).slice(2, 10)}`,
      name: `${source.name} (副本)`,
      lastConnectedAt: undefined,
    };
    await get().saveConnection(copy);
  },

  deleteConnection: async (id) => {
    set({ loading: true, error: null });
    try {
      await connectionCommands.deleteConnection(id);
      await get().fetchConnections();
      await get().fetchGroups();
      void emitCrossWindow(EVENT_CONNECTIONS_CHANGED);
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : '删除连接失败',
      });
    }
  },

  testConnection: async (config) => {
    return connectionCommands.testConnection(config);
  },

  addGroup: async (name) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    const { groups } = get();
    if (groups.includes(trimmed)) return;
    const updated = [...groups, trimmed];
    await connectionCommands.saveGroups(updated);
    set({ groups: updated });
  },

  setSelectedGroup: (group) => set({ selectedGroup: group }),
  setSearchQuery: (query) => set({ searchQuery: query }),
}));
