import { create } from 'zustand';
import { databaseCommands } from '../commands/database';
import { DB_REGISTRY } from '../lib/databaseTypes';
import { ensureNamespacePath as ensureNamespacePathImpl } from '../lib/ensureNamespace';
import {
  mergeNamespacePath,
  pathKey,
  type NamespaceMergeKind,
  type SqlNamespace,
} from '../lib/sqlNamespace';
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

const EMPTY_NAMESPACE: SqlNamespace = {};

function isSchemaGroupingSchema(schema: string | null | undefined): boolean {
  return schema != null && schema !== 'CATALOG' && schema !== 'SCHEMA';
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
  databaseType: string | null;
  /** True when driver supports multi-db AND connection sees more than one database. */
  isMultiDatabase: boolean;
  tables: TableInfo[];
  views: TableInfo[];
  columnMap: Record<string, string[]>;
  namespaceTree: SqlNamespace;
  loadedPaths: Set<string>;
  supersetDbIds: Record<string, string>;
  expanded: Set<string>;
  selectedId: string | null;
  loading: boolean;
  error: string | null;

  loadForConnection: (connectionId: string, options?: LoadForConnectionOptions) => Promise<void>;
  loadTables: (database: string) => Promise<void>;
  /**
   * Apply an already-fetched table list into the store (for multi-db / custom
   * trees that keep local caches but must feed SQL editor autocomplete).
   */
  setLoadedTables: (database: string, all: TableInfo[]) => void;
  mergeNamespace: (segments: string[], kind: NamespaceMergeKind, names: string[]) => void;
  registerSupersetDatabases: (entries: { name: string; id: string }[]) => void;
  ensureNamespacePath: (segments: string[]) => Promise<void>;
  loadColumnMap: () => Promise<void>;
  toggleExpand: (id: string) => void;
  setSelected: (id: string | null) => void;
  reset: () => void;
}

export const useSchemaStore = create<SchemaStore>((set, get) => ({
  connectionId: null,
  currentDatabase: null,
  databases: [],
  databaseType: null,
  isMultiDatabase: false,
  tables: [],
  views: [],
  columnMap: {},
  namespaceTree: EMPTY_NAMESPACE,
  loadedPaths: new Set(),
  supersetDbIds: {},
  expanded: new Set(),
  selectedId: null,
  loading: false,
  error: null,

  loadForConnection: async (connectionId, options) => {
    set({
      loading: true,
      error: null,
      connectionId,
      databaseType: options?.databaseType ?? null,
      namespaceTree: EMPTY_NAMESPACE,
      loadedPaths: new Set(),
      supersetDbIds: {},
    });
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
      if (isMultiDatabase) {
        get().mergeNamespace([], 'branch', databases);
      }
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
      get().setLoadedTables(database, all);
      set({ loading: false });
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : t('schema.loadTablesFailed'),
      });
    }
  },

  mergeNamespace: (segments, kind, names) => {
    const { namespaceTree, loadedPaths } = get();
    set({
      namespaceTree: mergeNamespacePath(namespaceTree, segments, kind, names),
      loadedPaths: new Set(loadedPaths).add(pathKey(segments)),
    });
  },

  registerSupersetDatabases: (entries) => {
    const nextIds = { ...get().supersetDbIds };
    const names: string[] = [];
    for (const { name, id } of entries) {
      nextIds[name] = id;
      names.push(name);
    }
    set({ supersetDbIds: nextIds, databaseType: 'superset' });
    get().mergeNamespace([], 'branch', names);
  },

  ensureNamespacePath: async (segments) => {
    const s = get();
    if (!s.connectionId) return;
    await ensureNamespacePathImpl(segments, {
      connectionId: s.connectionId,
      databaseType: s.databaseType,
      isMultiDatabase: s.isMultiDatabase,
      loadedPaths: s.loadedPaths,
      supersetDbIds: s.supersetDbIds,
      namespaceTree: s.namespaceTree,
      tables: s.tables,
      databases: s.databases,
      currentDatabase: s.currentDatabase,
      mergeNamespace: get().mergeNamespace,
      registerSupersetDatabases: get().registerSupersetDatabases,
      getDatabases: databaseCommands.getDatabases,
      getTables: databaseCommands.getTables,
      useDatabase: databaseCommands.useDatabase,
    });
  },

  setLoadedTables: (database, all) => {
    const { databaseType, isMultiDatabase, namespaceTree, loadedPaths } = get();
    const tables = all.filter((item) => item.tableType !== 'view');
    const views = all.filter((item) => item.tableType === 'view');

    let nextTree = namespaceTree;
    let nextLoadedPaths = loadedPaths;

    if (databaseType !== 'superset') {
      const hasSchemaGrouping = all.some((item) => isSchemaGroupingSchema(item.schema));

      if (hasSchemaGrouping) {
        const bySchema = new Map<string, string[]>();
        for (const item of all) {
          if (!isSchemaGroupingSchema(item.schema)) continue;
          const list = bySchema.get(item.schema!) ?? [];
          list.push(item.name);
          bySchema.set(item.schema!, list);
        }
        nextLoadedPaths = new Set(loadedPaths);
        for (const [schema, names] of bySchema) {
          const segments = isMultiDatabase ? [database, schema] : [schema];
          nextTree = mergeNamespacePath(nextTree, segments, 'tables', names);
          nextLoadedPaths.add(pathKey(segments));
        }
      } else {
        const tableNames = all.map((item) => item.name);
        nextTree = mergeNamespacePath(nextTree, [database], 'tables', tableNames);
        nextLoadedPaths = new Set(loadedPaths).add(pathKey([database]));
      }
    }

    set({
      tables,
      views,
      currentDatabase: database,
      columnMap: {},
      namespaceTree: nextTree,
      loadedPaths: nextLoadedPaths,
    });
  },

  loadColumnMap: async () => {
    const { connectionId, tables, views } = get();
    if (!connectionId) return;
    const allNames = [...tables, ...views].map((item) => item.name);
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
      databaseType: null,
      isMultiDatabase: false,
      tables: [],
      views: [],
      columnMap: {},
      namespaceTree: EMPTY_NAMESPACE,
      loadedPaths: new Set(),
      supersetDbIds: {},
      expanded: new Set(),
      selectedId: null,
      loading: false,
      error: null,
    }),
}));
