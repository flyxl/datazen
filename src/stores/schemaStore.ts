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
  omitTableLeaf,
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

/** Fallback key when mutating schema without an active DB session (singleton compat). */
const DEFAULT_SCHEMA_KEY = '__default__';

/** Per-session schema cache entry (map key = runtime DB session id). */
export interface ConnectionSchemaState {
  currentDatabase: string | null;
  databases: string[];
  databaseType: string | null;
  isMultiDatabase: boolean;
  tables: TableInfo[];
  views: TableInfo[];
  /** All schema names including those with no tables (e.g. from PG schemata). */
  schemaNames: string[];
  columnMap: Record<string, string[]>;
  namespaceTree: SqlNamespace;
  loadedPaths: Set<string>;
  pathItems: Record<string, TableInfo[]>;
  pathAliases: Record<string, string>;
  namespaceOwnedByPlugin: boolean;
  schemaEpoch: number;
  expanded: Set<string>;
  selectedId: string | null;
  loading: boolean;
  ensuringCount: number;
  error: string | null;
  columnInflight: Set<string>;
}

const CONNECTION_STATE_KEYS = [
  'currentDatabase',
  'databases',
  'databaseType',
  'isMultiDatabase',
  'tables',
  'views',
  'schemaNames',
  'columnMap',
  'namespaceTree',
  'loadedPaths',
  'pathItems',
  'pathAliases',
  'namespaceOwnedByPlugin',
  'schemaEpoch',
  'expanded',
  'selectedId',
  'loading',
  'ensuringCount',
  'error',
  'columnInflight',
] as const satisfies readonly (keyof ConnectionSchemaState)[];

function createEmptyConnectionSchema(): ConnectionSchemaState {
  return {
    currentDatabase: null,
    databases: [],
    databaseType: null,
    isMultiDatabase: false,
    tables: [],
    views: [],
    schemaNames: [],
    columnMap: {},
    namespaceTree: EMPTY_NAMESPACE,
    loadedPaths: new Set(),
    pathItems: {},
    pathAliases: {},
    namespaceOwnedByPlugin: false,
    schemaEpoch: 0,
    expanded: new Set(),
    selectedId: null,
    loading: false,
    ensuringCount: 0,
    error: null,
    columnInflight: new Set(),
  };
}

function activeFlatten(
  schemas: Map<string, ConnectionSchemaState>,
  activeDbSessionId: string | null,
): ConnectionSchemaState & { dbSessionId: string | null } {
  const readKey =
    activeDbSessionId ?? (schemas.has(DEFAULT_SCHEMA_KEY) ? DEFAULT_SCHEMA_KEY : null);
  if (!readKey) {
    return { ...createEmptyConnectionSchema(), dbSessionId: null };
  }
  const schema = schemas.get(readKey);
  if (!schema) {
    return { ...createEmptyConnectionSchema(), dbSessionId: activeDbSessionId };
  }
  return { ...schema, dbSessionId: activeDbSessionId };
}

function extractSchemaPatch(partial: Partial<SchemaStore>): Partial<ConnectionSchemaState> {
  const result: Partial<ConnectionSchemaState> = {};
  for (const key of CONNECTION_STATE_KEYS) {
    if (!(key in partial)) continue;
    (result as Record<string, unknown>)[key] = partial[key as keyof ConnectionSchemaState];
  }
  return result;
}

function mergePartialIntoStore(
  current: { schemas: Map<string, ConnectionSchemaState>; activeDbSessionId: string | null },
  partial: Partial<SchemaStore>,
): Pick<SchemaStore, 'schemas' | 'activeDbSessionId'> &
  ConnectionSchemaState & { dbSessionId: string | null } {
  let schemas = partial.schemas ?? current.schemas;
  let activeDbSessionId = current.activeDbSessionId;

  if ('activeDbSessionId' in partial) {
    activeDbSessionId = partial.activeDbSessionId ?? null;
  } else if ('dbSessionId' in partial) {
    activeDbSessionId = partial.dbSessionId ?? null;
  }

  if (activeDbSessionId && !schemas.has(activeDbSessionId)) {
    schemas = new Map(schemas);
    schemas.set(activeDbSessionId, createEmptyConnectionSchema());
  }

  const schemaPatch = extractSchemaPatch(partial);

  const mutationKey =
    activeDbSessionId ?? (Object.keys(schemaPatch).length > 0 ? DEFAULT_SCHEMA_KEY : null);

  if (mutationKey && Object.keys(schemaPatch).length > 0) {
    schemas = new Map(schemas);
    const prev = schemas.get(mutationKey) ?? createEmptyConnectionSchema();
    schemas.set(mutationKey, { ...prev, ...schemaPatch });
  }

  return {
    schemas,
    activeDbSessionId,
    ...activeFlatten(schemas, activeDbSessionId),
  };
}

function patchConnectionSchema(
  schemas: Map<string, ConnectionSchemaState>,
  dbSessionId: string,
  patch: Partial<ConnectionSchemaState>,
): Map<string, ConnectionSchemaState> {
  const next = new Map(schemas);
  const prev = next.get(dbSessionId) ?? createEmptyConnectionSchema();
  next.set(dbSessionId, { ...prev, ...patch });
  return next;
}

function resolveTargetConnectionId(
  state: { activeDbSessionId: string | null },
  dbSessionId?: string,
): string {
  return dbSessionId ?? state.activeDbSessionId ?? DEFAULT_SCHEMA_KEY;
}

function resolveRealConnectionId(
  state: { activeDbSessionId: string | null },
  dbSessionId?: string,
): string | null {
  return dbSessionId ?? state.activeDbSessionId;
}

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

interface SchemaStore extends ConnectionSchemaState {
  /** Keyed per-connection schema cache. */
  schemas: Map<string, ConnectionSchemaState>;
  /** Runtime DB session id of the active session. */
  activeDbSessionId: string | null;
  /** Alias for `activeDbSessionId` (backward compatible). */
  dbSessionId: string | null;

  loadForConnection: (dbSessionId: string, options?: LoadForConnectionOptions) => Promise<void>;
  loadTables: (database: string, dbSessionId?: string) => Promise<void>;
  /**
   * Switch the session to a different logical database and refresh the editor
   * context (tables/namespace/currentDatabase) for it. Unlike `loadTables`,
   * this does NOT bump `schemaEpoch`, so sidebar/query-panel listeners that
   * treat epoch bumps as schema-wide invalidations keep their per-database
   * cache. It is meant for lightweight context switches (e.g. the Query Panel
   * database dropdown) where the connection schema itself did not change.
   */
  switchDatabase: (database: string, dbSessionId?: string) => Promise<void>;
  setLoadedTables: (database: string, all: TableInfo[], dbSessionId?: string) => void;
  removeRelation: (name: string, dbSessionId?: string) => void;
  mergeNamespace: (
    segments: string[],
    kind: NamespaceMergeKind,
    names: string[],
    dbSessionId?: string,
  ) => void;
  cachePathItems: (fetchPath: string, items: TableInfo[], dbSessionId?: string) => void;
  registerPathAliases: (entries: { name: string; id: string }[], dbSessionId?: string) => void;
  ensureNamespacePath: (segments: string[], dbSessionId?: string) => Promise<void>;
  ensureColumns: (tableNames: string[], dbSessionId?: string) => Promise<void>;
  loadColumnMap: (dbSessionId?: string) => Promise<void>;
  toggleExpand: (id: string, dbSessionId?: string) => void;
  setSelected: (id: string | null, dbSessionId?: string) => void;
  reset: () => void;
  setActiveConnection: (dbSessionId: string | null) => void;
  removeConnection: (dbSessionId: string) => void;
  getConnectionSchema: (dbSessionId: string) => ConnectionSchemaState | undefined;
}

export const useSchemaStore = create<SchemaStore>((set, get) => {
  type SchemaUpdater = Partial<SchemaStore> | ((state: SchemaStore) => Partial<SchemaStore>);

  const setSynced = (partial: SchemaUpdater) => {
    if (typeof partial === 'function') {
      set(
        (state) => ({ ...state, ...mergePartialIntoStore(state, partial(state)) }) as SchemaStore,
      );
    } else {
      set((state) => ({ ...state, ...mergePartialIntoStore(state, partial) }) as SchemaStore);
    }
  };

  const commitConnectionPatch = (
    dbSessionId: string,
    patch: Partial<ConnectionSchemaState>,
    options?: { activate?: boolean },
  ) => {
    setSynced((state) => {
      const schemas = patchConnectionSchema(state.schemas, dbSessionId, patch);
      const activeDbSessionId = options?.activate ? dbSessionId : state.activeDbSessionId;
      return {
        schemas,
        activeDbSessionId,
        ...activeFlatten(schemas, activeDbSessionId),
      };
    });
  };

  const empty = createEmptyConnectionSchema();

  return {
    schemas: new Map(),
    activeDbSessionId: null,
    dbSessionId: null,
    ...empty,

    setActiveConnection: (dbSessionId) => {
      setSynced((state) => ({
        activeDbSessionId: dbSessionId,
        ...activeFlatten(state.schemas, dbSessionId),
      }));
    },

    removeConnection: (dbSessionId) => {
      setSynced((state) => {
        const schemas = new Map(state.schemas);
        schemas.delete(dbSessionId);
        const activeDbSessionId =
          state.activeDbSessionId === dbSessionId ? null : state.activeDbSessionId;
        return {
          schemas,
          activeDbSessionId,
          ...activeFlatten(schemas, activeDbSessionId),
        };
      });
    },

    getConnectionSchema: (dbSessionId) => get().schemas.get(dbSessionId),

    loadForConnection: async (dbSessionId, options) => {
      commitConnectionPatch(
        dbSessionId,
        {
          loading: true,
          ensuringCount: 0,
          error: null,
          databaseType: options?.databaseType ?? null,
          namespaceTree: EMPTY_NAMESPACE,
          loadedPaths: new Set(),
          pathItems: {},
          pathAliases: {},
          namespaceOwnedByPlugin: false,
        },
        { activate: true },
      );
      try {
        const allDatabases = await databaseCommands.getDatabases(dbSessionId);
        const meta = options?.databaseType
          ? DB_REGISTRY[options.databaseType as DatabaseType]
          : undefined;
        const { databases, preferred, lockedToConfigured } = resolveVisibleDatabases(
          allDatabases,
          options?.preferredDatabase,
        );
        const isMultiDatabase =
          !lockedToConfigured && computeIsMultiDatabase(meta?.hasMultiDatabase, databases.length);
        commitConnectionPatch(
          dbSessionId,
          { databases, isMultiDatabase, loading: false, currentDatabase: preferred },
          { activate: true },
        );
        const isPathHierarchy =
          meta?.schemaTreeMode === 'custom' || meta?.namespaceEnsure === 'path-hierarchy';
        if (isMultiDatabase && !isPathHierarchy) {
          get().mergeNamespace([], 'branch', databases, dbSessionId);
        }
        if (options?.skipLoadTables) return;
        if (preferred) {
          await get().loadTables(preferred, dbSessionId);
          get().setSelected(`db:${preferred}`, dbSessionId);
        }
      } catch (e) {
        commitConnectionPatch(
          dbSessionId,
          {
            loading: false,
            error: e instanceof Error ? e.message : t('schema.loadDbFailed'),
            isMultiDatabase: false,
          },
          { activate: true },
        );
      }
    },

    loadTables: async (database, dbSessionIdOverride) => {
      const dbSessionId = resolveRealConnectionId(get(), dbSessionIdOverride);
      if (!dbSessionId) return;
      commitConnectionPatch(dbSessionId, { loading: true, error: null });
      try {
        await databaseCommands.useDatabase(dbSessionId, database);
        const all = await databaseCommands.getTables(dbSessionId, database);
        get().setLoadedTables(database, all, dbSessionId);
        const schema = get().schemas.get(dbSessionId);
        commitConnectionPatch(dbSessionId, {
          loading: false,
          schemaEpoch: (schema?.schemaEpoch ?? 0) + 1,
        });
      } catch (e) {
        commitConnectionPatch(dbSessionId, {
          loading: false,
          error: e instanceof Error ? e.message : t('schema.loadTablesFailed'),
        });
      }
    },

    switchDatabase: async (database, dbSessionIdOverride) => {
      const dbSessionId = resolveRealConnectionId(get(), dbSessionIdOverride);
      if (!dbSessionId) return;
      commitConnectionPatch(dbSessionId, { loading: true, error: null });
      try {
        await databaseCommands.useDatabase(dbSessionId, database);
        const all = await databaseCommands.getTables(dbSessionId, database);
        get().setLoadedTables(database, all, dbSessionId);
        commitConnectionPatch(dbSessionId, { loading: false });
      } catch (e) {
        commitConnectionPatch(dbSessionId, {
          loading: false,
          error: e instanceof Error ? e.message : t('schema.loadTablesFailed'),
        });
      }
    },

    mergeNamespace: (segments, kind, names, dbSessionIdOverride) => {
      const dbSessionId = resolveTargetConnectionId(get(), dbSessionIdOverride);
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      commitConnectionPatch(dbSessionId, {
        namespaceTree: mergeNamespacePath(schema.namespaceTree, segments, kind, names),
        loadedPaths: new Set(schema.loadedPaths).add(pathKey(segments)),
      });
    },

    cachePathItems: (fetchPath, items, dbSessionIdOverride) => {
      if (!fetchPath) return;
      const dbSessionId = resolveTargetConnectionId(get(), dbSessionIdOverride);
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      commitConnectionPatch(dbSessionId, {
        pathItems: { ...schema.pathItems, [fetchPath]: items },
      });
    },

    registerPathAliases: (entries, dbSessionIdOverride) => {
      const dbSessionId = resolveTargetConnectionId(get(), dbSessionIdOverride);
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      const nextIds = { ...schema.pathAliases };
      const names: string[] = [];
      for (const { name, id } of entries) {
        nextIds[name] = id;
        names.push(name);
      }
      commitConnectionPatch(dbSessionId, {
        pathAliases: nextIds,
        namespaceOwnedByPlugin: true,
      });
      get().mergeNamespace([], 'branch', names, dbSessionId);
    },

    ensureNamespacePath: async (segments, dbSessionIdOverride) => {
      const dbSessionId = resolveRealConnectionId(get(), dbSessionIdOverride);
      if (!dbSessionId) return;
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      const deps = {
        dbSessionId,
        databaseType: schema.databaseType,
        isMultiDatabase: schema.isMultiDatabase,
        loadedPaths: schema.loadedPaths,
        pathItems: schema.pathItems,
        pathAliases: schema.pathAliases,
        namespaceTree: schema.namespaceTree,
        tables: schema.tables,
        databases: schema.databases,
        currentDatabase: schema.currentDatabase,
        mergeNamespace: (segs: string[], kind: NamespaceMergeKind, names: string[]) =>
          get().mergeNamespace(segs, kind, names, dbSessionId),
        cachePathItems: (fetchPath: string, items: TableInfo[]) =>
          get().cachePathItems(fetchPath, items, dbSessionId),
        registerPathAliases: (entries: { name: string; id: string }[]) =>
          get().registerPathAliases(entries, dbSessionId),
        getDatabases: databaseCommands.getDatabases,
        getTables: databaseCommands.getTables,
        useDatabase: databaseCommands.useDatabase,
      };
      const pending = namespaceEnsurePending(segments, deps);
      if (pending) {
        commitConnectionPatch(dbSessionId, { ensuringCount: schema.ensuringCount + 1 });
      }
      try {
        await ensureNamespacePathImpl(segments, deps);
      } finally {
        if (pending) {
          const latest = get().schemas.get(dbSessionId);
          commitConnectionPatch(dbSessionId, {
            ensuringCount: Math.max(0, (latest?.ensuringCount ?? 1) - 1),
          });
        }
      }
    },

    setLoadedTables: (database, all, dbSessionIdOverride) => {
      const dbSessionId = resolveTargetConnectionId(get(), dbSessionIdOverride);
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      const { databaseType, isMultiDatabase, namespaceTree, loadedPaths, namespaceOwnedByPlugin } =
        schema;
      const realItems = all.filter((item) => item.name !== '');
      const tables = realItems.filter((item) => item.tableType !== 'view');
      const views = realItems.filter((item) => item.tableType === 'view');
      const meta = databaseType ? DB_REGISTRY[databaseType as DatabaseType] : undefined;

      let nextTree = namespaceTree;
      let nextLoadedPaths = loadedPaths;

      if (!namespaceOwnedByPlugin && !meta?.namespaceOwnedByPlugin) {
        const hasSchemaGrouping = all.some((item) => isSchemaGroupingSchema(item.schema));

        if (hasSchemaGrouping) {
          const bySchema = new Map<string, string[]>();
          for (const item of all) {
            if (!isSchemaGroupingSchema(item.schema)) continue;
            if (!item.name) {
              if (!bySchema.has(item.schema!)) bySchema.set(item.schema!, []);
              continue;
            }
            const list = bySchema.get(item.schema!) ?? [];
            list.push(item.name);
            bySchema.set(item.schema!, list);
          }
          nextLoadedPaths = new Set(loadedPaths);
          for (const [schemaName, names] of bySchema) {
            const segments = isMultiDatabase ? [database, schemaName] : [schemaName];
            nextTree = mergeNamespacePath(nextTree, segments, 'tables', names, { replace: true });
            nextLoadedPaths.add(pathKey(segments));
          }
        } else {
          const names = realItems.map((item) => item.name);
          nextTree = mergeNamespacePath(nextTree, [database], 'tables', names, { replace: true });
          nextLoadedPaths = new Set(loadedPaths).add(pathKey([database]));
        }
      }

      const schemaNames = [
        ...new Set(all.map((item) => item.schema).filter((s): s is string => !!s)),
      ];

      commitConnectionPatch(dbSessionId, {
        tables,
        views,
        schemaNames,
        currentDatabase: database,
        columnMap: {},
        namespaceTree: nextTree,
        loadedPaths: nextLoadedPaths,
      });
    },

    removeRelation: (name, dbSessionIdOverride) => {
      const dbSessionId = resolveTargetConnectionId(get(), dbSessionIdOverride);
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      const nextSelected =
        schema.selectedId === `table:${name}` || schema.selectedId === `view:${name}`
          ? null
          : schema.selectedId;
      commitConnectionPatch(dbSessionId, {
        tables: schema.tables.filter((item) => item.name !== name),
        views: schema.views.filter((item) => item.name !== name),
        namespaceTree: omitTableLeaf(schema.namespaceTree, name),
        selectedId: nextSelected,
      });
    },

    ensureColumns: async (tableNames, dbSessionIdOverride) => {
      const dbSessionId = resolveRealConnectionId(get(), dbSessionIdOverride);
      if (!dbSessionId) return;
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      const { columnMap, namespaceTree, tables, views, pathItems, columnInflight } = schema;
      const known = knownTableNames(namespaceTree, tables, views, pathItems);
      if (known.size === 0) return;
      const wanted = [
        ...new Set(
          tableNames
            .map((name) => name.trim())
            .filter((name) => name.length > 0 && known.has(name)),
        ),
      ];
      const missing = wanted.filter((name) => !(name in columnMap) && !columnInflight.has(name));
      if (missing.length === 0) return;

      const nextInflight = new Set(columnInflight);
      for (const name of missing) nextInflight.add(name);
      commitConnectionPatch(dbSessionId, { columnInflight: nextInflight });

      try {
        const settled = await Promise.all(
          missing.map(async (name) => {
            try {
              return { name, cols: await databaseCommands.getColumns(dbSessionId, name) };
            } catch {
              return { name, cols: null };
            }
          }),
        );
        const latest = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
        const nextColumnMap = { ...latest.columnMap };
        let changed = false;
        for (const row of settled) {
          if (row.cols == null) continue;
          nextColumnMap[row.name] = row.cols;
          changed = true;
        }
        const clearedInflight = new Set(latest.columnInflight);
        for (const name of missing) clearedInflight.delete(name);
        if (changed) {
          commitConnectionPatch(dbSessionId, {
            columnMap: nextColumnMap,
            columnInflight: clearedInflight,
          });
        } else {
          commitConnectionPatch(dbSessionId, { columnInflight: clearedInflight });
        }
      } catch {
        const latest = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
        const clearedInflight = new Set(latest.columnInflight);
        for (const name of missing) clearedInflight.delete(name);
        commitConnectionPatch(dbSessionId, { columnInflight: clearedInflight });
      }
    },

    loadColumnMap: async (dbSessionIdOverride) => {
      const dbSessionId = resolveRealConnectionId(get(), dbSessionIdOverride);
      if (!dbSessionId) return;
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      const allNames = [...schema.tables, ...schema.views].map((item) => item.name);
      await get().ensureColumns(allNames, dbSessionId);
    },

    toggleExpand: (id, dbSessionIdOverride) => {
      const dbSessionId = resolveTargetConnectionId(get(), dbSessionIdOverride);
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      const next = new Set(schema.expanded);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      commitConnectionPatch(dbSessionId, { expanded: next });
    },

    setSelected: (id, dbSessionIdOverride) => {
      const dbSessionId = resolveTargetConnectionId(get(), dbSessionIdOverride);
      commitConnectionPatch(dbSessionId, { selectedId: id });
    },

    reset: () => {
      const fresh = createEmptyConnectionSchema();
      setSynced({
        schemas: new Map(),
        activeDbSessionId: null,
        dbSessionId: null,
        ...fresh,
      });
    },
  };
});

/** Route external `setState` partials into the keyed schema map (backward compatible). */
const nativeSetState = useSchemaStore.setState.bind(useSchemaStore);
useSchemaStore.setState = ((partial, replace) => {
  const merge = (state: SchemaStore): SchemaStore =>
    ({
      ...state,
      ...(typeof partial === 'function'
        ? mergePartialIntoStore(state, partial(state))
        : mergePartialIntoStore(state, partial)),
    }) as SchemaStore;
  if (replace) {
    nativeSetState(merge as Parameters<typeof nativeSetState>[0], true);
  } else if (typeof partial === 'function') {
    nativeSetState((state) => merge(state));
  } else {
    nativeSetState((state) => merge(state));
  }
}) as typeof useSchemaStore.setState;

/**
 * Read a field from the keyed per-session schema store.
 * Falls back to the global (active-session) field when the keyed entry
 * is not yet populated, keeping backward compatibility for callers that
 * only have a single DB session open.
 */
export function useConnectionSchemaField<K extends keyof ConnectionSchemaState>(
  dbSessionId: string,
  field: K,
): ConnectionSchemaState[K] {
  return useSchemaStore((s) => {
    const entry = s.schemas.get(dbSessionId);
    if (entry) return entry[field];
    return (s as unknown as ConnectionSchemaState)[field];
  });
}

if (import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__schemaStore = useSchemaStore;
}
