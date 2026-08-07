import { create } from 'zustand';
import { databaseCommands } from '../commands/database';
import { DB_REGISTRY } from '../lib/databaseTypes';
import { t } from '../locales/t';
import type { DatabaseType, TableInfo } from '../types';

/** Session multi-db UI: capability flag AND more than one *visible* database. */
export function computeIsMultiDatabase(
  hasMultiDatabase: boolean | undefined,
  databaseCount: number,
): boolean {
  return Boolean(hasMultiDatabase && databaseCount > 1);
}

export function resolvePreferredDatabase(
  databases: string[],
  preferredDatabase?: string,
): string | null {
  if (preferredDatabase && databases.includes(preferredDatabase)) {
    return preferredDatabase;
  }
  return databases[0] ?? null;
}

/**
 * When the connection config specifies a *logical* database that appears in
 * the driver list, lock the sidebar to that single database.
 *
 * If the preferred value is set but **not** in `allDatabases` (e.g. Kiwi stores
 * the instance domain in `config.database`), do **not** lock — expose the full
 * list and fall back to `resolvePreferredDatabase` semantics.
 */
export function resolveVisibleDatabases(
  allDatabases: string[],
  preferredDatabase?: string,
): { databases: string[]; preferred: string | null; lockedToConfigured: boolean } {
  const configured = preferredDatabase?.trim();
  if (configured && allDatabases.includes(configured)) {
    return {
      databases: [configured],
      preferred: configured,
      lockedToConfigured: true,
    };
  }
  return {
    databases: allDatabases,
    preferred: resolvePreferredDatabase(allDatabases, configured || undefined),
    lockedToConfigured: false,
  };
}

export interface LoadForConnectionOptions {
  skipLoadTables?: boolean;
  preferredDatabase?: string;
  /** Used to resolve hasMultiDatabase for session isMultiDatabase. */
  databaseType?: DatabaseType | string;
}

interface SchemaStore {
  connectionId: string | null;
  currentDatabase: string | null;
  databases: string[];
  /** True when driver supports multi-db AND connection sees more than one database. */
  isMultiDatabase: boolean;
  tables: TableInfo[];
  views: TableInfo[];
  columnMap: Record<string, string[]>;
  expanded: Set<string>;
  selectedId: string | null;
  loading: boolean;
  error: string | null;

  loadForConnection: (connectionId: string, options?: LoadForConnectionOptions) => Promise<void>;
  loadTables: (database: string) => Promise<void>;
  loadColumnMap: () => Promise<void>;
  toggleExpand: (id: string) => void;
  setSelected: (id: string | null) => void;
  reset: () => void;
}

export const useSchemaStore = create<SchemaStore>((set, get) => ({
  connectionId: null,
  currentDatabase: null,
  databases: [],
  isMultiDatabase: false,
  tables: [],
  views: [],
  columnMap: {},
  expanded: new Set(),
  selectedId: null,
  loading: false,
  error: null,

  loadForConnection: async (connectionId, options) => {
    set({ loading: true, error: null, connectionId });
    try {
      const allDatabases = await databaseCommands.getDatabases(connectionId);
      const meta = options?.databaseType
        ? DB_REGISTRY[options.databaseType as DatabaseType]
        : undefined;
      const { databases, preferred, lockedToConfigured } = resolveVisibleDatabases(
        allDatabases,
        options?.preferredDatabase,
      );
      const isMultiDatabase =
        !lockedToConfigured &&
        computeIsMultiDatabase(meta?.hasMultiDatabase, databases.length);
      set({ databases, isMultiDatabase, loading: false, currentDatabase: preferred });
      if (options?.skipLoadTables) return;
      if (preferred) {
        await get().loadTables(preferred);
        get().setSelected(`db:${preferred}`);
      }
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : t('schema.loadDbFailed'),
        isMultiDatabase: false,
      });
    }
  },

  loadTables: async (database) => {
    const { connectionId } = get();
    if (!connectionId) return;
    set({ loading: true, error: null });
    try {
      // Session switch for MySQL/MariaDB (and no-op for drivers without override).
      await databaseCommands.useDatabase(connectionId, database);
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

  loadColumnMap: async () => {
    const { connectionId, tables, views } = get();
    if (!connectionId) return;
    const allNames = [...tables, ...views].map((t) => t.name);
    const results = await Promise.all(
      allNames.map((name) =>
        databaseCommands.getColumns(connectionId, name).catch(() => [] as string[]),
      ),
    );
    const map: Record<string, string[]> = {};
    allNames.forEach((name, i) => {
      map[name] = results[i];
    });
    set({ columnMap: map });
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
      isMultiDatabase: false,
      tables: [],
      views: [],
      columnMap: {},
      expanded: new Set(),
      selectedId: null,
      loading: false,
      error: null,
    }),
}));
