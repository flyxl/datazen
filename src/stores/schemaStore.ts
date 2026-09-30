import { create } from 'zustand';
import { useMemo } from 'react';
import { projectRelationColumns } from './schemaColumnLoader';
import { SchemaRequestTracker } from './schemaRequestTracker';
import { loadRelationColumns, resolveColumnRelations } from './schemaColumnLoader';
import { databaseCommands } from '../commands/database';
import { DB_REGISTRY } from '../lib/databaseTypes';
import {
  ensureNamespacePath as ensureNamespacePathImpl,
  namespaceEnsurePending,
} from '../lib/ensureNamespace';
import {
  isSchemaGroupingSchema,
  mergeNamespacePath,
  omitTableLeaf,
  pathKey,
  type NamespaceMergeKind,
} from '../lib/sqlNamespace';
import { t } from '../locales/t';
import type { DatabaseType, TableInfo } from '../types';
import { bindSchemaStore, relationKey } from '@datazen/driver-sdk';
import {
  computeIsMultiDatabase,
  knownTableNames,
  parsePathHierarchyDatabaseEntry,
  resolvePreferredDatabase,
  resolveVisibleDatabases,
  type LoadForConnectionOptions,
} from './schemaStoreHelpers';
import {
  createEmptyConnectionSchema,
  EMPTY_NAMESPACE,
  patchConnectionSchema,
  type ConnectionSchemaState,
} from './schemaStoreState';

export {
  computeIsMultiDatabase,
  knownTableNames,
  parsePathHierarchyDatabaseEntry,
  resolvePreferredDatabase,
  resolveVisibleDatabases,
  type LoadForConnectionOptions,
} from './schemaStoreHelpers';
export type { ConnectionSchemaState } from './schemaStoreState';

export interface SchemaStore {
  /** Keyed per-connection schema cache. */
  schemas: Map<string, ConnectionSchemaState>;
  /** Runtime DB session id of the active session. */
  activeDbSessionId: string | null;

  loadForConnection: (dbSessionId: string, options?: LoadForConnectionOptions) => Promise<void>;
  loadTables: (database: string, dbSessionId: string) => Promise<void>;
  /**
   * Switch the session to a different logical database and refresh the editor
   * context (tables/namespace/currentDatabase) for it. Unlike `loadTables`,
   * this does NOT bump `schemaEpoch`, so sidebar/query-panel listeners that
   * treat epoch bumps as schema-wide invalidations keep their per-database
   * cache. It is meant for lightweight context switches (e.g. the Query Panel
   * database dropdown) where the connection schema itself did not change.
   */
  switchDatabase: (database: string, dbSessionId: string) => Promise<void>;
  /** F7: pin/clear the PG-family current schema (local UI state; sent as the
   * `schema` envelope field on query executions). */
  setCurrentSchema: (schema: string | null, dbSessionId: string) => void;
  /** Sync the session `currentDatabase` to the ACTIVE panel's bound database so
   * the store reflects the tab the user is on (prevents drift to the first
   * database after a reload, e.g. Settings round-trip). */
  setCurrentDatabase: (database: string | null, dbSessionId: string) => void;
  setLoadedTables: (
    database: string,
    all: TableInfo[],
    dbSessionId: string,
    options?: { pinCurrentDatabase?: boolean },
  ) => void;
  removeRelation: (name: string, dbSessionId: string) => void;
  mergeNamespace: (
    segments: string[],
    kind: NamespaceMergeKind,
    names: string[],
    dbSessionId: string,
  ) => void;
  cachePathItems: (fetchPath: string, items: TableInfo[], dbSessionId: string) => void;
  registerPathAliases: (entries: { name: string; id: string }[], dbSessionId: string) => void;
  ensureNamespacePath: (segments: string[], dbSessionId: string) => Promise<void>;
  ensureColumns: (
    tableNames: string[],
    dbSessionId: string,
    database: string,
    options?: { schema?: string | null },
  ) => Promise<void>;
  ensureDatabaseColumns: (dbSessionId: string, database: string) => Promise<void>;
  toggleExpand: (id: string, dbSessionId: string) => void;
  setSelected: (id: string | null, dbSessionId: string) => void;
  reset: () => void;
  setActiveConnection: (dbSessionId: string | null) => void;
  removeConnection: (dbSessionId: string) => void;
  getConnectionSchema: (dbSessionId: string) => ConnectionSchemaState | undefined;
  /**
   * The schema a *loaded* relation lives in, from its `TableInfo.schema`.
   * Schema-aware engines (PostgreSQL, SQL Server) need it for every per-table
   * metadata read; `null` means "unknown/not applicable" and is what
   * schema-less engines expect.
   */
  schemaOfRelation: (name: string, dbSessionId: string) => string | null;
}

export const useSchemaStore = create<SchemaStore>((set, get) => {
  const commitConnectionPatch = (
    dbSessionId: string,
    patch: Partial<ConnectionSchemaState>,
    options?: { activate?: boolean },
  ) => {
    if (!dbSessionId.trim()) return;
    set((state) => {
      const schemas = patchConnectionSchema(state.schemas, dbSessionId, patch);
      const activeDbSessionId = options?.activate ? dbSessionId : state.activeDbSessionId;
      return {
        schemas,
        activeDbSessionId,
      };
    });
  };

  const requests = new SchemaRequestTracker();
  const reloadTables = async (database: string, dbSessionId: string, refresh: boolean) => {
    if (!dbSessionId) return;
    const isCurrent = requests.begin(dbSessionId, 'tables');
    commitConnectionPatch(dbSessionId, { loading: true, error: null });
    try {
      const all = await databaseCommands.listTables(dbSessionId, database);
      if (!isCurrent()) return;
      get().setLoadedTables(database, all, dbSessionId);
      const state = get().schemas.get(dbSessionId);
      commitConnectionPatch(dbSessionId, {
        loading: false,
        ...(refresh ? { schemaEpoch: (state?.schemaEpoch ?? 0) + 1 } : {}),
      });
    } catch (e) {
      if (!isCurrent()) return;
      commitConnectionPatch(dbSessionId, {
        loading: false,
        error: e instanceof Error ? e.message : t('schema.loadTablesFailed'),
      });
    }
  };

  return {
    schemas: new Map(),
    activeDbSessionId: null,

    setActiveConnection: (dbSessionId) => {
      set({ activeDbSessionId: dbSessionId });
    },

    removeConnection: (dbSessionId) => {
      requests.invalidate(dbSessionId);
      set((state) => {
        const schemas = new Map(state.schemas);
        schemas.delete(dbSessionId);
        const activeDbSessionId =
          state.activeDbSessionId === dbSessionId ? null : state.activeDbSessionId;
        return {
          schemas,
          activeDbSessionId,
        };
      });
    },

    getConnectionSchema: (dbSessionId) => get().schemas.get(dbSessionId),

    schemaOfRelation: (name, dbSessionId) => {
      const state = get().schemas.get(dbSessionId);
      if (!state) return null;
      const target = name.trim();
      if (!target) return null;
      const candidates = [
        ...state.tables,
        ...state.views,
        ...Object.values(state.pathItems).flat(),
      ];
      const owners = new Set(
        candidates.filter((item) => item.name === target).map((item) => item.schema ?? null),
      );
      return owners.size === 1 ? ([...owners][0] ?? null) : null;
    },

    loadForConnection: async (dbSessionId, options) => {
      requests.invalidate(dbSessionId);
      const isCurrent = requests.begin(dbSessionId, 'connection');
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
        { activate: options?.activate !== false },
      );
      try {
        const allDatabases = await databaseCommands.getDatabases(dbSessionId);
        if (!isCurrent()) return;
        const meta = options?.databaseType
          ? DB_REGISTRY[options.databaseType as DatabaseType]
          : undefined;
        const isPathHierarchy =
          meta?.schemaTreeMode === 'custom' || meta?.namespaceEnsure === 'path-hierarchy';
        const usesPluginDbList =
          isPathHierarchy &&
          (meta?.namespaceOwnedByPlugin || meta?.schemaTreeMode === 'custom') &&
          allDatabases.length > 0;

        let databases: string[];
        let preferred: string | null;
        let lockedToConfigured: boolean;

        if (usesPluginDbList) {
          const configured = options?.preferredDatabase?.trim();
          const displayNames = allDatabases.map(
            (entry) => parsePathHierarchyDatabaseEntry(entry).name,
          );
          if (configured && allDatabases.includes(configured)) {
            databases = [parsePathHierarchyDatabaseEntry(configured).name];
            preferred = parsePathHierarchyDatabaseEntry(configured).name;
            lockedToConfigured = true;
          } else {
            databases = displayNames;
            preferred = resolvePreferredDatabase(displayNames, configured || undefined);
            lockedToConfigured = false;
          }
        } else {
          const resolved = resolveVisibleDatabases(allDatabases, options?.preferredDatabase);
          databases = resolved.databases;
          preferred = resolved.preferred;
          lockedToConfigured = resolved.lockedToConfigured;
        }

        const isMultiDatabase =
          !lockedToConfigured && computeIsMultiDatabase(meta?.hasMultiDatabase, databases.length);

        // Preserve the user-selected database across component remounts (e.g.
        // opening Settings and navigating back re-triggers loadForConnection).
        // Without this the session's currentDatabase reverts to the first
        // database in the dropdown, so a query pinned to a non-first database
        // would then fail with "table does not exist". Only fall back to the
        // configured/preferred database when the previous selection is no
        // longer present in the freshly listed databases.
        const previousDatabase = get().schemas.get(dbSessionId)?.currentDatabase ?? null;
        const currentDatabase =
          previousDatabase && databases.includes(previousDatabase) ? previousDatabase : preferred;

        commitConnectionPatch(
          dbSessionId,
          { databases, isMultiDatabase, loading: false, currentDatabase },
          { activate: false },
        );
        if (usesPluginDbList) {
          const aliasEntries = allDatabases.map(parsePathHierarchyDatabaseEntry);
          get().registerPathAliases(aliasEntries, dbSessionId);
        } else if (isMultiDatabase && !isPathHierarchy) {
          get().mergeNamespace([], 'branch', databases, dbSessionId);
        }
        if (options?.skipLoadTables) return;
        if (currentDatabase) {
          await get().loadTables(currentDatabase, dbSessionId);
          if (isCurrent()) get().setSelected(`db:${currentDatabase}`, dbSessionId);
        }
      } catch (e) {
        if (!isCurrent()) return;
        commitConnectionPatch(
          dbSessionId,
          {
            loading: false,
            error: e instanceof Error ? e.message : t('schema.loadDbFailed'),
            isMultiDatabase: false,
          },
          { activate: false },
        );
      }
    },

    loadTables: (database, dbSessionId) => reloadTables(database, dbSessionId, true),
    switchDatabase: (database, dbSessionId) => reloadTables(database, dbSessionId, false),

    /**
     * F7: set the PG-family current schema (pure local UI state, like
     * `currentDatabase` in F1 — no IPC). Query executions carry it as the
     * `schema` envelope field; rewrite-capable drivers inline it as
     * `"schema"."t"`. `null` clears the pin.
     */
    setCurrentSchema: (schema, dbSessionId) => {
      if (!dbSessionId) return;
      const normalized = schema?.trim() ? schema.trim() : null;
      commitConnectionPatch(dbSessionId, {
        currentSchema: normalized,
      });
    },

    // Session-local database pointer. When the active (query/table) panel is
    // switched or brought back into focus, the host drives this from the
    // panel's bound database so `currentDatabase` always reflects the tab the
    // user is on — otherwise loadForConnection/schema-tree defaults can
    // re-pin it to the first database after a reload (e.g. Settings round-trip)
    // even though the panel is still targeting `tradingdb`.
    setCurrentDatabase: (database, dbSessionId) => {
      if (!dbSessionId) return;
      const normalized = database?.trim() || null;
      if (!normalized) return;
      const session = get().schemas.get(dbSessionId);
      const databases = session?.databases ?? [];
      // Only adopt a database the session actually knows about; before the
      // database list is loaded (empty) we still allow the panel pointer.
      if (databases.length > 0 && !databases.includes(normalized)) return;
      commitConnectionPatch(dbSessionId, { currentDatabase: normalized });
    },

    mergeNamespace: (segments, kind, names, dbSessionId) => {
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      commitConnectionPatch(dbSessionId, {
        namespaceTree: mergeNamespacePath(schema.namespaceTree, segments, kind, names),
        loadedPaths: new Set(schema.loadedPaths).add(pathKey(segments)),
      });
    },

    cachePathItems: (fetchPath, items, dbSessionId) => {
      if (!fetchPath) return;
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      commitConnectionPatch(dbSessionId, {
        pathItems: { ...schema.pathItems, [fetchPath]: items },
      });
    },

    registerPathAliases: (entries, dbSessionId) => {
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

    ensureNamespacePath: async (segments, dbSessionId) => {
      if (!dbSessionId) return;
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      const isCurrent = requests.capture(dbSessionId);
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
        mergeNamespace: (segs: string[], kind: NamespaceMergeKind, names: string[]) => {
          if (isCurrent()) get().mergeNamespace(segs, kind, names, dbSessionId);
        },
        cachePathItems: (fetchPath: string, items: TableInfo[]) => {
          if (isCurrent()) get().cachePathItems(fetchPath, items, dbSessionId);
        },
        registerPathAliases: (entries: { name: string; id: string }[]) => {
          if (isCurrent()) get().registerPathAliases(entries, dbSessionId);
        },
        getDatabases: databaseCommands.getDatabases,
        listTables: databaseCommands.listTables,
      };
      const pending = namespaceEnsurePending(segments, deps);
      if (pending) {
        commitConnectionPatch(dbSessionId, { ensuringCount: schema.ensuringCount + 1 });
      }
      try {
        await ensureNamespacePathImpl(segments, deps);
      } finally {
        if (pending && isCurrent()) {
          const latest = get().schemas.get(dbSessionId);
          commitConnectionPatch(dbSessionId, {
            ensuringCount: Math.max(0, (latest?.ensuringCount ?? 1) - 1),
          });
        }
      }
    },

    setLoadedTables: (database, all, dbSessionId, options) => {
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      const tableCatalogs = { ...schema.tableCatalogs, [database]: all };
      if (options?.pinCurrentDatabase === false && schema.currentDatabase !== database) {
        commitConnectionPatch(dbSessionId, { tableCatalogs });
        return;
      }
      // Detach every earlier column load when this directory is replaced.
      requests.begin(dbSessionId, 'columns');
      // Background cache refreshes (expanded-db fan-out, session refresh) pass
      // { pinCurrentDatabase: false } so a late-completing reload cannot
      // overwrite whichever database the user last selected in the tree —
      // last-finish-wins used to fight the click itself (non-reproducible
      // wrong-ER symptoms).
      const pinCurrentDatabase = options?.pinCurrentDatabase !== false;
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
        tableCatalogs,
        views,
        schemaNames,
        columnInflight: new Set(),
        relationColumns: Object.fromEntries(
          Object.entries(schema.relationColumns).filter(
            ([, value]) => value.ref.database !== database,
          ),
        ),
        ...(pinCurrentDatabase ? { currentDatabase: database } : {}),
        namespaceTree: nextTree,
        loadedPaths: nextLoadedPaths,
      });
    },

    removeRelation: (name, dbSessionId) => {
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

    ensureColumns: async (tableNames, dbSessionId, database, options) => {
      if (!dbSessionId || !database.trim()) return;
      const schema = get().schemas.get(dbSessionId);
      if (!schema) return;
      const isCurrent = requests.captureSlot(dbSessionId, 'columns');
      const refs = resolveColumnRelations(
        schema,
        tableNames,
        dbSessionId,
        database,
        options?.schema,
      ).filter((ref) => {
        const key = relationKey({ ...ref, dbSessionId });
        return !schema.columnInflight.has(key) && !(key in schema.relationColumns);
      });
      if (refs.length === 0) return;
      commitConnectionPatch(dbSessionId, {
        columnInflight: new Set([
          ...schema.columnInflight,
          ...refs.map((ref) => relationKey({ ...ref, dbSessionId })),
        ]),
      });
      try {
        const values = await loadRelationColumns(dbSessionId, refs);
        if (!isCurrent()) return;
        const latest = get().schemas.get(dbSessionId);
        if (!latest) return;
        const relationColumns = { ...latest.relationColumns };
        for (const value of values) {
          relationColumns[relationKey({ ...value.ref, dbSessionId })] = value;
        }
        commitConnectionPatch(dbSessionId, { relationColumns });
      } catch {
        // No successful result is fabricated; later calls may retry.
      } finally {
        if (isCurrent()) {
          const latest = get().schemas.get(dbSessionId);
          if (latest)
            commitConnectionPatch(dbSessionId, {
              columnInflight: new Set(
                [...latest.columnInflight].filter(
                  (key) => !refs.some((ref) => relationKey({ ...ref, dbSessionId }) === key),
                ),
              ),
            });
        }
      }
    },

    ensureDatabaseColumns: async (dbSessionId, database) => {
      if (!dbSessionId || !database?.trim()) return;
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      // Collect table names from ALL sources: tables array (normal drivers),
      // namespaceTree (path-hierarchy / multi-db), and pathItems.
      const allNames = [
        ...knownTableNames(schema.namespaceTree, schema.tables, schema.views, schema.pathItems),
      ];
      await get().ensureColumns(allNames, dbSessionId, database);
    },

    toggleExpand: (id, dbSessionId) => {
      const schema = get().schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
      const next = new Set(schema.expanded);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      commitConnectionPatch(dbSessionId, { expanded: next });
    },

    setSelected: (id, dbSessionId) => {
      commitConnectionPatch(dbSessionId, { selectedId: id });
    },

    reset: () => {
      requests.reset();
      set({
        schemas: new Map(),
        activeDbSessionId: null,
      });
    },
  };
});

const emptyConnectionSchema = createEmptyConnectionSchema();

/**
 * Read a field from the keyed per-session schema store.
 * Missing sessions read an empty snapshot, never another session's data.
 */
export function useConnectionSchemaField<K extends keyof ConnectionSchemaState>(
  dbSessionId: string,
  field: K,
): ConnectionSchemaState[K] {
  return useSchemaStore((s) => {
    const entry = s.schemas.get(dbSessionId);
    if (entry) return entry[field];
    return emptyConnectionSchema[field];
  });
}

/** Derive editor columns from the canonical relation cache for one session/context. */
export function useConnectionColumnMaps(dbSessionId: string) {
  const entry = useSchemaStore((state) => state.schemas.get(dbSessionId));
  return useMemo(
    () =>
      projectRelationColumns(
        entry ?? emptyConnectionSchema,
        dbSessionId,
        entry?.currentDatabase ?? '',
        entry?.currentSchema ?? undefined,
      ),
    [entry, dbSessionId],
  );
}

if (import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__schemaStore = useSchemaStore;
}

bindSchemaStore(useSchemaStore);
