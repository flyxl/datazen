import { create } from 'zustand';
import { connectionCommands } from '../commands/connection';
import { emitCrossWindow } from '../lib/crossWindowBus';
import { t } from '../locales/t';
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

/** Group connections by their `group` field; ungrouped come last. */
export function groupConnections(
  connections: ConnectionConfig[],
  groups: string[],
  searchQuery: string,
): { group: string; connections: ConnectionConfig[] }[] {
  const q = searchQuery.trim().toLowerCase();
  const filtered = connections.filter((c) => {
    if (!q) return true;
    const hay = `${c.name} ${c.host ?? ''} ${c.database ?? ''} ${c.databaseType}`.toLowerCase();
    return hay.includes(q);
  });

  const map = new Map<string, ConnectionConfig[]>();
  for (const g of groups) map.set(g, []);
  for (const c of filtered) {
    const key = c.group || '';
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(c);
  }
  const result: { group: string; connections: ConnectionConfig[] }[] = [];
  const seen = new Set<string>();
  for (const g of groups) {
    result.push({ group: g, connections: map.get(g) ?? [] });
    seen.add(g);
  }
  for (const [key, conns] of map) {
    if (key && !seen.has(key) && conns.length > 0) {
      result.push({ group: key, connections: conns });
    }
  }
  const ungrouped = map.get('');
  if (ungrouped && ungrouped.length > 0) {
    result.push({ group: '', connections: ungrouped });
  }
  return result;
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
  renameGroup: (oldName: string, newName: string) => Promise<void>;
  deleteGroup: (name: string) => Promise<void>;
  moveConnectionToGroup: (connectionId: string, group: string | undefined) => Promise<void>;
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
        error: e instanceof Error ? e.message : t('connStore.loadFailed'),
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
        error: e instanceof Error ? e.message : t('connStore.saveFailed'),
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
      name: `${source.name} (${t('conn.copyName')})`,
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
        error: e instanceof Error ? e.message : t('connStore.deleteFailed'),
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

  renameGroup: async (oldName, newName) => {
    const trimmed = newName.trim();
    if (!trimmed || trimmed === oldName) return;
    const { groups, connections } = get();
    const updated = groups.map((g) => (g === oldName ? trimmed : g));
    await connectionCommands.saveGroups(updated);
    for (const c of connections) {
      if (c.group === oldName) {
        await connectionCommands.saveConnection({ ...c, group: trimmed });
      }
    }
    await get().fetchConnections();
    set({ groups: updated });
  },

  deleteGroup: async (name) => {
    const { groups, connections } = get();
    const updated = groups.filter((g) => g !== name);
    await connectionCommands.saveGroups(updated);
    for (const c of connections) {
      if (c.group === name) {
        await connectionCommands.saveConnection({ ...c, group: undefined });
      }
    }
    await get().fetchConnections();
    set({ groups: updated, selectedGroup: null });
  },

  moveConnectionToGroup: async (connectionId, group) => {
    const { connections } = get();
    const conn = connections.find((c) => c.id === connectionId);
    if (!conn) return;
    await connectionCommands.saveConnection({ ...conn, group });
    await get().fetchConnections();
    void emitCrossWindow(EVENT_CONNECTIONS_CHANGED);
  },

  setSelectedGroup: (group) => set({ selectedGroup: group }),
  setSearchQuery: (query) => set({ searchQuery: query }),
}));
