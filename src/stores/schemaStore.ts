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

/** Fallback key when mutating schema without an active connection (singleton compat). */
const DEFAULT_SCHEMA_KEY = '__default__';

/** Per-connection schema cache entry. */
export interface ConnectionSchemaState {
  currentDatabase: string | null;
  databases: string[];
  databaseType: string | null;
  isMultiDatabase: boolean;
  tables: TableInfo[];
  views: TableInfo[];
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
  activeConnectionId: string | null,
): ConnectionSchemaState & { connectionId: string | null } {
  const readKey =
    activeConnectionId ?? (schemas.has(DEFAULT_SCHEMA_KEY) ? DEFAULT_SCHEMA_KEY : null);
  if (!readKey) {
    return { ...createEmptyConnectionSchema(), connectionId: null };
  }
  const schema = schemas.get(readKey);
  if (!schema) {
    return { ...createEmptyConnectionSchema(), connectionId: activeConnectionId };
  }
  return { ...schema, connectionId: activeConnectionId };
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
  current: { schemas: Map<string, ConnectionSchemaState>; activeConnectionId: string | null },
  partial: Partial<SchemaStore>,
): Pick<SchemaStore, 'schemas' | 'activeConnectionId'> &
  ConnectionSchemaState & { connectionId: string | null } {
  let schemas = partial.schemas ?? current.schemas;
  let activeConnectionId = current.activeConnectionId;

  if ('activeConnectionId' in partial) {
    activeConnectionId = partial.activeConnectionId ?? null;
  } else if ('connectionId' in partial) {
    activeConnectionId = partial.connectionId ?? null;
  }

  if (activeConnectionId && !schemas.has(activeConnectionId)) {
    schemas = new Map(schemas);
    schemas.set(activeConnectionId, createEmptyConnectionSchema());
  }

  const schemaPatch = extractSchemaPatch(partial);

  const mutationKey =
    activeConnectionId ?? (Object.keys(schemaPatch).length > 0 ? DEFAULT_SCHEMA_KEY : null);

  if (mutationKey && Object.keys(schemaPatch).length > 0) {
    schemas = new Map(schemas);
    const prev = schemas.get(mutationKey) ?? createEmptyConnectionSchema();
    schemas.set(mutationKey, { ...prev, ...schemaPatch });
  }

  return {
    schemas,
    activeConnectionId,
    ...activeFlatten(schemas, activeConnectionId),
  };
}

function patchConnectionSchema(
  schemas: Map<string, ConnectionSchemaState>,
  connectionId: string,
  patch: Partial<ConnectionSchemaState>,
): Map<string, ConnectionSchemaState> {
  const next = new Map(schemas);
  const prev = next.get(connectionId) ?? createEmptyConnectionSchema();
  next.set(connectionId, { ...prev, ...patch });
  return next;
}

function resolveTargetConnectionId(
  state: { activeConnectionId: string | null },
  override?: string,
): string {
  return override ?? state.activeConnectionId ?? DEFAULT_SCHEMA_KEY;
}

function resolveRealConnectionId(
  state: { activeConnectionId: string | null },
  override?: string,
): string | null {
  return override ?? state.activeConnectionId;
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
  activeConnectionId: string | null;
  /** Alias for `activeConnectionId` (backward compatible). */
  connectionId: string | null;

  loadForConnection: (connectionId: string, options?: LoadForConnectionOptions) => Promise<void>;
  loadTables: (database: string, connectionId?: string) => Promise<void>;
  setLoadedTables: (database: string, all: TableInfo[], connectionId?: string) => void;
  removeRelation: (name: string, connectionId?: string) => void;
  mergeNamespace: (
    segments: string[],
    kind: NamespaceMergeKind,
    names: string[],
    connectionId?: string,
  ) => void;
  cachePathItems: (fetchPath: string, items: TableInfo[], connectionId?: string) => void;
  registerPathAliases: (entries: { name: string; id: string }[], connectionId?: string) => void;
  ensureNamespacePath: (segments: string[], connectionId?: string) => Promise<void>;
  ensureColumns: (tableNames: string[], connectionId?: string) => Promise<void>;
  loadColumnMap: (connectionId?: string) => Promise<void>;
  toggleExpand: (id: string, connectionId?: string) => void;
  setSelected: (id: string | null, connectionId?: string) => void;
  reset: () => void;
  setActiveConnection: (connectionId: string | null) => void;
  removeConnection: (connectionId: string) => void;
  getConnectionSchema: (connectionId: string) => ConnectionSchemaState | undefined;
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
    connectionId: string,
    patch: Partial<ConnectionSchemaState>,
    options?: { activate?: boolean },
  ) => {
    setSynced((state) => {
      const schemas = patchConnectionSchema(state.schemas, connectionId, patch);
      const activeConnectionId = options?.activate ? connectionId : state.activeConnectionId;
      return {
        schemas,
        activeConnectionId,
        ...activeFlatten(schemas, activeConnectionId),
      };
    });
  };

  const empty = createEmptyConnectionSchema();

  return {
    schemas: new Map(),
    activeConnectionId: null,
    connectionId: null,
    ...empty,

    setActiveConnection: (connectionId) => {
      setSynced((state) => ({
        activeConnectionId: connectionId,
        ...activeFlatten(state.schemas, connectionId),
      }));
    },

    removeConnection: (connectionId) => {
      setSynced((state) => {
        const schemas = new Map(state.schemas);
        schemas.delete(connectionId);
        const activeConnectionId =
          state.activeConnectionId === connectionId ? null : state.activeConnectionId;
        return {
          schemas,
          activeConnectionId,
          ...activeFlatten(schemas, activeConnectionId),
        };
      });
    },

    getConnectionSchema: (connectionId) => get().schemas.get(connectionId),

    loadForConnection: async (connectionId, options) => {
      commitConnectionPatch(
        connectionId,
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
        commitConnectionPatch(
          connectionId,
          { databases, isMultiDatabase, loading: false, currentDatabase: preferred },
          { activate: true },
        );
        if (isMultiDatabase) {
          get().mergeNamespace([], 'branch', databases, connectionId);
        }
        if (options?.skipLoadTables) return;
        if (preferred) {
          await get().loadTables(preferred, connectionId);
          get().setSelected(`db:${preferred}`, connectionId);
        }
      } catch (e) {
        commitConnectionPatch(
          connectionId,
          {
            loading: false,
            error: e instanceof Error ? e.message : t('schema.loadDbFailed'),
            isMultiDatabase: false,
          },
          { activate: true },
        );
      }
    },

    loadTables: async (database, connectionIdOverride) => {
      const connectionId = resolveRealConnectionId(get(), connectionIdOverride);
      if (!connectionId) return;
      commitConnectionPatch(connectionId, { loading: true, error: null });
      try {
        await databaseCommands.useDatabase(connectionId, database);
        const all = await databaseCommands.getTables(connectionId, database);
        get().setLoadedTables(database, all, connectionId);
        const schema = get().schemas.get(connectionId);
        commitConnectionPatch(connectionId, {
          loading: false,
          schemaEpoch: (schema?.schemaEpoch ?? 0) + 1,
        });
      } catch (e) {
        commitConnectionPatch(connectionId, {
          loading: false,
          error: e instanceof Error ? e.message : t('schema.loadTablesFailed'),
        });
      }
    },

    mergeNamespace: (segments, kind, names, connectionIdOverride) => {
      const connectionId = resolveTargetConnectionId(get(), connectionIdOverride);
      const schema = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
      commitConnectionPatch(connectionId, {
        namespaceTree: mergeNamespacePath(schema.namespaceTree, segments, kind, names),
        loadedPaths: new Set(schema.loadedPaths).add(pathKey(segments)),
      });
    },

    cachePathItems: (fetchPath, items, connectionIdOverride) => {
      if (!fetchPath) return;
      const connectionId = resolveTargetConnectionId(get(), connectionIdOverride);
      const schema = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
      commitConnectionPatch(connectionId, {
        pathItems: { ...schema.pathItems, [fetchPath]: items },
      });
    },

    registerPathAliases: (entries, connectionIdOverride) => {
      const connectionId = resolveTargetConnectionId(get(), connectionIdOverride);
      const schema = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
      const nextIds = { ...schema.pathAliases };
      const names: string[] = [];
      for (const { name, id } of entries) {
        nextIds[name] = id;
        names.push(name);
      }
      commitConnectionPatch(connectionId, {
        pathAliases: nextIds,
        namespaceOwnedByPlugin: true,
      });
      get().mergeNamespace([], 'branch', names, connectionId);
    },

    ensureNamespacePath: async (segments, connectionIdOverride) => {
      const connectionId = resolveRealConnectionId(get(), connectionIdOverride);
      if (!connectionId) return;
      const schema = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
      const deps = {
        connectionId,
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
          get().mergeNamespace(segs, kind, names, connectionId),
        cachePathItems: (fetchPath: string, items: TableInfo[]) =>
          get().cachePathItems(fetchPath, items, connectionId),
        registerPathAliases: (entries: { name: string; id: string }[]) =>
          get().registerPathAliases(entries, connectionId),
        getDatabases: databaseCommands.getDatabases,
        getTables: databaseCommands.getTables,
        useDatabase: databaseCommands.useDatabase,
      };
      const pending = namespaceEnsurePending(segments, deps);
      if (pending) {
        commitConnectionPatch(connectionId, { ensuringCount: schema.ensuringCount + 1 });
      }
      try {
        await ensureNamespacePathImpl(segments, deps);
      } finally {
        if (pending) {
          const latest = get().schemas.get(connectionId);
          commitConnectionPatch(connectionId, {
            ensuringCount: Math.max(0, (latest?.ensuringCount ?? 1) - 1),
          });
        }
      }
    },

    setLoadedTables: (database, all, connectionIdOverride) => {
      const connectionId = resolveTargetConnectionId(get(), connectionIdOverride);
      const schema = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
      const { databaseType, isMultiDatabase, namespaceTree, loadedPaths, namespaceOwnedByPlugin } =
        schema;
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
          for (const [schemaName, names] of bySchema) {
            const segments = isMultiDatabase ? [database, schemaName] : [schemaName];
            nextTree = mergeNamespacePath(nextTree, segments, 'tables', names, { replace: true });
            nextLoadedPaths.add(pathKey(segments));
          }
        } else {
          const names = all.map((item) => item.name);
          nextTree = mergeNamespacePath(nextTree, [database], 'tables', names, { replace: true });
          nextLoadedPaths = new Set(loadedPaths).add(pathKey([database]));
        }
      }

      commitConnectionPatch(connectionId, {
        tables,
        views,
        currentDatabase: database,
        columnMap: {},
        namespaceTree: nextTree,
        loadedPaths: nextLoadedPaths,
      });
    },

    removeRelation: (name, connectionIdOverride) => {
      const connectionId = resolveTargetConnectionId(get(), connectionIdOverride);
      const schema = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
      const nextSelected =
        schema.selectedId === `table:${name}` || schema.selectedId === `view:${name}`
          ? null
          : schema.selectedId;
      commitConnectionPatch(connectionId, {
        tables: schema.tables.filter((item) => item.name !== name),
        views: schema.views.filter((item) => item.name !== name),
        namespaceTree: omitTableLeaf(schema.namespaceTree, name),
        selectedId: nextSelected,
      });
    },

    ensureColumns: async (tableNames, connectionIdOverride) => {
      const connectionId = resolveRealConnectionId(get(), connectionIdOverride);
      if (!connectionId) return;
      const schema = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
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
      commitConnectionPatch(connectionId, { columnInflight: nextInflight });

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
        const latest = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
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
          commitConnectionPatch(connectionId, {
            columnMap: nextColumnMap,
            columnInflight: clearedInflight,
          });
        } else {
          commitConnectionPatch(connectionId, { columnInflight: clearedInflight });
        }
      } catch {
        const latest = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
        const clearedInflight = new Set(latest.columnInflight);
        for (const name of missing) clearedInflight.delete(name);
        commitConnectionPatch(connectionId, { columnInflight: clearedInflight });
      }
    },

    loadColumnMap: async (connectionIdOverride) => {
      const connectionId = resolveRealConnectionId(get(), connectionIdOverride);
      if (!connectionId) return;
      const schema = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
      const allNames = [...schema.tables, ...schema.views].map((item) => item.name);
      await get().ensureColumns(allNames, connectionId);
    },

    toggleExpand: (id, connectionIdOverride) => {
      const connectionId = resolveTargetConnectionId(get(), connectionIdOverride);
      const schema = get().schemas.get(connectionId) ?? createEmptyConnectionSchema();
      const next = new Set(schema.expanded);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      commitConnectionPatch(connectionId, { expanded: next });
    },

    setSelected: (id, connectionIdOverride) => {
      const connectionId = resolveTargetConnectionId(get(), connectionIdOverride);
      commitConnectionPatch(connectionId, { selectedId: id });
    },

    reset: () => {
      const fresh = createEmptyConnectionSchema();
      setSynced({
        schemas: new Map(),
        activeConnectionId: null,
        connectionId: null,
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
 * Read a field from the keyed per-connection schema store.
 * Falls back to the global (active-connection) field when the keyed entry
 * is not yet populated, keeping backward compatibility for callers that
 * only have a single connection open.
 */
export function useConnectionSchemaField<K extends keyof ConnectionSchemaState>(
  connectionId: string,
  field: K,
): ConnectionSchemaState[K] {
  return useSchemaStore((s) => {
    const entry = s.schemas.get(connectionId);
    if (entry) return entry[field];
    return (s as unknown as ConnectionSchemaState)[field];
  });
}
