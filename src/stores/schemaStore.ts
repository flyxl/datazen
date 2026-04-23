import { create } from 'zustand';
import { databaseCommands } from '../commands/database';
import { t } from '../locales/t';
import type { TableInfo } from '../types';

interface SchemaStore {
  connectionId: string | null;
  currentDatabase: string | null;
  databases: string[];
  tables: TableInfo[];
  views: TableInfo[];
  expanded: Set<string>;
  selectedId: string | null;
  loading: boolean;
  error: string | null;

  loadForConnection: (connectionId: string, options?: { skipLoadTables?: boolean }) => Promise<void>;
  loadTables: (database: string) => Promise<void>;
  toggleExpand: (id: string) => void;
  setSelected: (id: string | null) => void;
  reset: () => void;
}

export const useSchemaStore = create<SchemaStore>((set, get) => ({
  connectionId: null,
  currentDatabase: null,
  databases: [],
  tables: [],
  views: [],
  expanded: new Set(),
  selectedId: null,
  loading: false,
  error: null,

  loadForConnection: async (connectionId, options) => {
    set({ loading: true, error: null, connectionId });
    try {
      const databases = await databaseCommands.getDatabases(connectionId);
      set({ databases, loading: false });
      const first = databases[0] ?? null;
      if (first && !options?.skipLoadTables) {
        await get().loadTables(first);
        get().setSelected(`db:${first}`);
      }
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : t('schema.loadDbFailed'),
      });
    }
  },

  loadTables: async (database) => {
    const { connectionId } = get();
    if (!connectionId) return;
    set({ loading: true, error: null });
    try {
      const all = await databaseCommands.getTables(connectionId, database);
      const tables = all.filter((t) => t.tableType !== 'view');
      const views = all.filter((t) => t.tableType === 'view');
      set({ tables, views, loading: false, currentDatabase: database });
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : t('schema.loadTablesFailed'),
      });
    }
  },

  toggleExpand: (id) =>
    set((s) => {
      const next = new Set(s.expanded);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return { expanded: next };
    }),

  setSelected: (id) => set({ selectedId: id }),

  reset: () =>
    set({
      connectionId: null,
      currentDatabase: null,
      databases: [],
      tables: [],
      views: [],
      expanded: new Set(),
      selectedId: null,
      loading: false,
      error: null,
    }),
}));
