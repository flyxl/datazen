import { create } from 'zustand';
import { databaseCommands } from '../commands/database';
import { DB_REGISTRY } from '../lib/databaseTypes';
import {
  ensureNamespacePath as ensureNamespacePathImpl,
  namespaceEnsurePending,
} from '../lib/ensureNamespace';
import {
  collectTableLeafNames,
  isSchemaGroupingSchema,
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
const columnInflight = new Set<string>();

/** Names that are safe to pass to `get_columns` (complete loaded tables only). */
export function knownTableNames(
  namespaceTree: SqlNamespace,
  tables: TableInfo[],
  views: TableInfo[] = [],
  pathItems: Record<string, TableInfo[]> = {},
): Set<string> {
  const names = collectTableLeafNames(namespaceTree);
  for (const item of [...tables, ...views]) {
    names.add(item.name);
  }
  for (const items of Object.values(pathItems)) {
    for (const item of items) {
      if (item.schema === 'CATALOG' || item.schema === 'SCHEMA') continue;
      const parts = item.name.split('/').filter(Boolean);
      names.add(parts[parts.length - 1] ?? item.name);
    }
  }
  return names;
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
  /**
   * Raw `get_tables` results keyed by fetch path (`dbId`, `dbId/catalog`, …).
   * Shared by SQL autocomplete and custom schema trees so expanding a node
   * already loaded via ensure does not hit the driver again.
   */
  pathItems: Record<string, TableInfo[]>;
  /** SQL display name → fetch path root id (plugin-registered). */
  pathAliases: Record<string, string>;
  /** When true, setLoadedTables skips namespace merges (plugin owns tree via SDK). */
  namespaceOwnedByPlugin: boolean;
  expanded: Set<string>;
  selectedId: string | null;
  loading: boolean;
  /** In-flight `ensureNamespacePath` fetches (SQL autocomplete loading UI). */
  ensuringCount: number;
  error: string | null;

  loadForConnection: (connectionId: string, options?: LoadForConnectionOptions) => Promise<void>;
  loadTables: (database: string) => Promise<void>;
  /**
   * Apply an already-fetched table list into the store (for multi-db / custom
   * trees that keep local caches but must feed SQL editor autocomplete).
   */
  setLoadedTables: (database: string, all: TableInfo[]) => void;
  mergeNamespace: (segments: string[], kind: NamespaceMergeKind, names: string[]) => void;
  cachePathItems: (fetchPath: string, items: TableInfo[]) => void;
  /** Register display-name → fetch-id aliases and seed top-level namespace branches. */
  registerPathAliases: (entries: { name: string; id: string }[]) => void;
  ensureNamespacePath: (segments: string[]) => Promise<void>;
  /**
   * Fetch columns only for known, complete table names (skips prefixes, names
   * already in columnMap, and failed lookups so they can retry).
   */
  ensureColumns: (tableNames: string[]) => Promise<void>;
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
  pathItems: {},
  pathAliases: {},
  namespaceOwnedByPlugin: false,
  expanded: new Set(),
  selectedId: null,
  loading: false,
  ensuringCount: 0,
  error: null,

  loadForConnection: async (connectionId, options) => {
    set({
      loading: true,
      ensuringCount: 0,
      error: null,
      connectionId,
      databaseType: options?.databaseType ?? null,
      namespaceTree: EMPTY_NAMESPACE,
      loadedPaths: new Set(),
      pathItems: {},
      pathAliases: {},
      namespaceOwnedByPlugin: false,
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
        !lockedToConfigured && computeIsMultiDatabase(meta?.hasMultiDatabase, databases.length);
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

  cachePathItems: (fetchPath, items) => {
    if (!fetchPath) return;
    set({ pathItems: { ...get().pathItems, [fetchPath]: items } });
  },

  registerPathAliases: (entries) => {
    const nextIds = { ...get().pathAliases };
    const names: string[] = [];
    for (const { name, id } of entries) {
      nextIds[name] = id;
      names.push(name);
    }
    set({ pathAliases: nextIds, namespaceOwnedByPlugin: true });
    get().mergeNamespace([], 'branch', names);
  },

  ensureNamespacePath: async (segments) => {
    const s = get();
    if (!s.connectionId) return;
    const deps = {
      connectionId: s.connectionId,
      databaseType: s.databaseType,
      isMultiDatabase: s.isMultiDatabase,
      loadedPaths: s.loadedPaths,
      pathItems: s.pathItems,
      pathAliases: s.pathAliases,
      namespaceTree: s.namespaceTree,
      tables: s.tables,
      databases: s.databases,
      currentDatabase: s.currentDatabase,
      mergeNamespace: get().mergeNamespace,
      cachePathItems: get().cachePathItems,
      registerPathAliases: get().registerPathAliases,
      getDatabases: databaseCommands.getDatabases,
      getTables: databaseCommands.getTables,
      useDatabase: databaseCommands.useDatabase,
    };
    const pending = namespaceEnsurePending(segments, deps);
    if (pending) set({ ensuringCount: get().ensuringCount + 1 });
    try {
      await ensureNamespacePathImpl(segments, deps);
    } finally {
      if (pending) set({ ensuringCount: Math.max(0, get().ensuringCount - 1) });
    }
  },

  setLoadedTables: (database, all) => {
    const { databaseType, isMultiDatabase, namespaceTree, loadedPaths, namespaceOwnedByPlugin } =
      get();
    const tables = all.filter((item) => item.tableType !== 'view');
    const views = all.filter((item) => item.tableType === 'view');
    const meta = databaseType ? DB_REGISTRY[databaseType as DatabaseType] : undefined;

    let nextTree = namespaceTree;
    let nextLoadedPaths = loadedPaths;

    if (!namespaceOwnedByPlugin && !meta?.namespaceOwnedByPlugin) {
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
        const names = all.map((item) => item.name);
        nextTree = mergeNamespacePath(nextTree, [database], 'tables', names);
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

  ensureColumns: async (tableNames) => {
    const { connectionId, columnMap, namespaceTree, tables, views, pathItems } = get();
    if (!connectionId) return;
    const known = knownTableNames(namespaceTree, tables, views, pathItems);
    if (known.size === 0) return;
    const wanted = [
      ...new Set(
        tableNames.map((name) => name.trim()).filter((name) => name.length > 0 && known.has(name)),
      ),
    ];
    const missing = wanted.filter((name) => !(name in columnMap) && !columnInflight.has(name));
    if (missing.length === 0) return;

    for (const name of missing) columnInflight.add(name);
    try {
      const settled = await Promise.all(
        missing.map(async (name) => {
          try {
            return { name, cols: await databaseCommands.getColumns(connectionId, name) };
          } catch {
            return { name, cols: null };
          }
        }),
      );
      const next = { ...get().columnMap };
      let changed = false;
      for (const row of settled) {
        if (row.cols == null) continue;
        next[row.name] = row.cols;
        changed = true;
      }
      if (changed) set({ columnMap: next });
    } finally {
      for (const name of missing) columnInflight.delete(name);
    }
  },

  loadColumnMap: async () => {
    const { tables, views } = get();
    const allNames = [...tables, ...views].map((item) => item.name);
    await get().ensureColumns(allNames);
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
      pathItems: {},
      pathAliases: {},
      namespaceOwnedByPlugin: false,
      expanded: new Set(),
      selectedId: null,
      loading: false,
      ensuringCount: 0,
      error: null,
    }),
}));
