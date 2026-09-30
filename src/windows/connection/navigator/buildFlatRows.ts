import { effectiveExpanded, searchedRowLevels } from '@datazen/ui';
import { isLeaf } from '../../../lib/sqlNamespace';
import { formatGroupLabel } from '../../../lib/connectionGroups';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import {
  filterTableItems,
  getObjectFilter,
  matchesTableNameFilter,
  shouldShowDatabase,
  shouldShowSchema,
} from '../../../lib/objectFilter';
import {
  rankConnections,
  PINNED_GROUP_KEY,
  RECENT_GROUP_KEY,
  connectionExpandKey,
  type ConnectionLocatorUsageState,
} from '../../../lib/connectionLocator';
import type { ConnectionEntry } from '../../../stores/activeConnectionStore';
import type { I18nKey } from '../../../locales';
import type { ConnectionConfig, DatabaseObject, TableInfo } from '../../../types';
import type { ObjectFilterPrefs } from '../../../lib/objectFilter';
import type { ConnectionSchemaState } from '../../../stores/schemaStore';
import { shouldUseMultiDatabaseTree } from './utils';
import { getCategoriesForDriver } from '../schema-tree/schemaTreeCategories';
import type { UnifiedRow } from './types';
import {
  CONNECTION_CHILD_DEPTH,
  DATABASE_CHILD_DEPTH,
  flattenNamespaceTree,
  groupBySchema,
} from './utils';

/**
 * Whether the tree should mark `dbName` as open.
 *
 * Two things make a database open: the user opened it in the tree, or the
 * driver still holds a per-database resource that "close database connection"
 * could release (PostgreSQL caches one pool per foreign database). Drivers
 * without such resources — MySQL serves every database from one pool — report
 * nothing, and the tree's own expand state is then the only source of truth.
 *
 * Note this deliberately does NOT consult `isDbExpanded`: a global object
 * search force-expands matching databases, which must not read as "open".
 */
function isDatabaseOpen(
  expandedDbs: Set<string>,
  openDbs: Record<string, Set<string>>,
  dbSessionId: string,
  dbName: string,
  connId: string,
): boolean {
  if (expandedDbs.has(`${connId}::${dbName}`)) return true;
  return openDbs[dbSessionId]?.has(dbName) ?? false;
}

export interface BuildNavigatorFlatRowsParams {
  grouped: { group: string; connections: ConnectionConfig[] }[];
  expandedGroups: Set<string>;
  expandedConnections: Set<string>;
  expandedDbs: Set<string>;
  expandedSchemas: Set<string>;
  expandedCats: Set<string>;
  activeConnections: Record<string, ConnectionEntry | undefined>;
  activeConnectionId: string | null;
  schemas: Map<string, ConnectionSchemaState>;
  dbTablesMap: Record<string, TableInfo[]>;
  dbObjectsMap: Record<string, DatabaseObject[]>;
  loadingDbs: Set<string>;
  /** Per-session set of databases with an open backend resource. */
  openDbs: Record<string, Set<string>>;
  query: string;
  t: (key: I18nKey, params?: Record<string, string | number>) => string;
  usageState?: ConnectionLocatorUsageState;
}

export function buildNavigatorFlatRows(params: BuildNavigatorFlatRowsParams): UnifiedRow[] {
  const {
    grouped,
    expandedGroups,
    expandedConnections,
    expandedDbs,
    expandedSchemas,
    expandedCats,
    activeConnections,
    activeConnectionId,
    schemas,
    dbTablesMap,
    dbObjectsMap,
    loadingDbs,
    openDbs,
    query,
    t,
  } = params;

  const rows: UnifiedRow[] = [];

  // A search force-expands every branch and drops the header above a
  // connection, so "is this branch open" and "how deep does it announce" are
  // two halves of the same fact. Both used to be spelled out by hand at eight
  // sites each — `has(key) || !!query` and a depth that quietly stayed put —
  // which is exactly how they drifted apart. `levels` and `isOpen` are the one
  // spelling now, shared with the Redis key tree.
  const searching = query !== '';
  const levels = (depth: number) => searchedRowLevels(depth, CONNECTION_CHILD_DEPTH, searching);
  const isOpen = (own: boolean) => effectiveExpanded(own, searching);

  const addCategories = (
    allItems: TableInfo[],
    connectionId: string,
    dbSessionId: string,
    dbName: string,
    schemaName: string | undefined,
    baseDepth: number,
    dbType: string,
    objectFilter: ObjectFilterPrefs,
  ) => {
    const filteredItems = filterTableItems(allItems, objectFilter);
    const realItems = filteredItems.filter((i) => i.name !== '');
    const tblItems = realItems.filter(
      (i) => i.tableType === 'table' || i.tableType === 'systemTable',
    );
    const viewItems = realItems.filter(
      (i) => i.tableType === 'view' || i.tableType === 'materializedView',
    );

    for (const cat of getCategoriesForDriver(dbType)) {
      const catKey = schemaName
        ? `${connectionId}::${dbName}::${schemaName}::${cat.id}`
        : `${connectionId}::${dbName}::${cat.id}`;
      const isExpanded = isOpen(expandedCats.has(catKey));
      let categoryObjects: DatabaseObject[] = [];

      let count = 0;
      if (cat.id === 'tables') {
        const filtered = query
          ? tblItems.filter((i) => i.name.toLowerCase().includes(query))
          : tblItems;
        count = filtered.length;
      } else if (cat.id === 'views') {
        const filtered = query
          ? viewItems.filter((i) => i.name.toLowerCase().includes(query))
          : viewItems;
        count = filtered.length;
      } else {
        const loadedObjects = dbObjectsMap[catKey] ?? [];
        // Driver commands list a whole database's objects. When the navigator
        // groups tables under schemas, each schema category has its own cache
        // key, so scope that shared command result to the category's owner.
        // Otherwise expanding one schema duplicates every routine in every
        // other schema and makes a leaf appear under the wrong parent. Keep
        // objects with no schema metadata: some drivers do not expose it.
        categoryObjects = schemaName
          ? loadedObjects.filter((object) => object.schema == null || object.schema === schemaName)
          : loadedObjects;
        const filtered = query
          ? categoryObjects.filter((o) => o.name.toLowerCase().includes(query))
          : categoryObjects;
        count = filtered.length;
      }

      if (query && count === 0) continue;

      rows.push({
        type: 'category',
        key: catKey,
        dbName,
        cat,
        count,
        expanded: isExpanded,
        ...levels(baseDepth),
      });

      if (isExpanded) {
        let items: TableInfo[] = [];
        let objs: DatabaseObject[] = [];
        if (cat.id === 'tables') items = tblItems;
        else if (cat.id === 'views') items = viewItems;
        else objs = categoryObjects;
        if (
          objectFilter.hideSystemSchemas ||
          objectFilter.tableNameInclude ||
          objectFilter.tableNameExclude
        ) {
          objs = objs.filter((o) => matchesTableNameFilter(o.name, objectFilter));
        }

        if (query) {
          items = items.filter((i) => i.name.toLowerCase().includes(query));
          objs = objs.filter((o) => o.name.toLowerCase().includes(query));
        }

        if (items.length > 0) {
          for (const item of items) {
            rows.push({
              type: 'table',
              item,
              ...levels(baseDepth + 1),
              catId: cat.id,
              isSelected: false,
              connectionId,
              dbSessionId,
              dbName,
            });
          }
        } else if (objs.length > 0) {
          for (const obj of objs) {
            // Carries its owner tuple, exactly as the `table` variant does: two
            // connections can each hold an object with the same name, and a key
            // built from `(catId, name)` alone would collide across them.
            rows.push({
              type: 'object',
              obj,
              ...levels(baseDepth + 1),
              catId: cat.id,
              connectionId,
              dbName,
              ...(schemaName === undefined ? {} : { schemaName }),
            });
          }
        }
      }
    }
  };

  const uniqueConnections = new Map<string, ConnectionConfig>();
  for (const section of grouped) {
    for (const connection of section.connections) {
      if (!uniqueConnections.has(connection.id)) uniqueConnections.set(connection.id, connection);
    }
  }
  const usageState = params.usageState ?? {
    activeConnections,
    schemas,
    dbTablesMap,
    dbObjectsMap,
  };
  const locatorResults = rankConnections([...uniqueConnections.values()], query, usageState);
  const matchesById = new Map(locatorResults.map((result) => [result.connection.id, result]));
  const sections = query
    ? [{ group: '', connections: locatorResults.map((result) => result.connection) }]
    : grouped;

  if (sections.length === 0) {
    rows.push({ type: 'no-connections', depth: 0, levelDepth: 0 });
    return rows;
  }
  for (const { group: groupName, connections: groupConns } of sections) {
    const filteredConns = groupConns;
    if (query && filteredConns.length === 0) continue;

    const isPinnedSection = groupName === PINNED_GROUP_KEY;
    const isRecentSection = groupName === RECENT_GROUP_KEY;
    const expanded = isOpen(expandedGroups.has(groupName));
    const displayName = isPinnedSection
      ? t('main.ctx.pinConnection')
      : isRecentSection
        ? t('context.recent')
        : groupName
          ? formatGroupLabel(groupName, t)
          : t('main.ungrouped');

    if (isPinnedSection || isRecentSection) {
      rows.push({
        type: 'section',
        depth: 0,
        levelDepth: 0,
        section: isPinnedSection ? 'pinned' : 'recent',
        displayName,
        count: filteredConns.length,
        expanded,
      });
    } else if (!query) {
      rows.push({
        type: 'group',
        depth: 0,
        levelDepth: 0,
        groupName,
        displayName,
        count: filteredConns.length,
        expanded,
      });
    }

    if (!expanded) continue;

    if (filteredConns.length === 0) {
      rows.push({ type: 'empty-group', groupName, depth: 1, levelDepth: 1 });
      continue;
    }

    for (const conn of filteredConns) {
      const objectFilter = getObjectFilter(conn);
      const entry = activeConnections[conn.id];
      const status = entry?.status ?? 'idle';
      const isConnected = status === 'connected';
      const isConnecting = status === 'connecting';
      const isExpanded = isOpen(expandedConnections.has(connectionExpandKey(groupName, conn.id)));

      rows.push({
        type: 'connection',
        conn,
        sectionGroup: groupName,
        isSelected: activeConnectionId === conn.id,
        status,
        expanded: isOpen(isExpanded && (isConnected || isConnecting)),
        ...levels(query ? 0 : CONNECTION_CHILD_DEPTH - 1),
        match: matchesById.get(conn.id)?.match ?? undefined,
      });

      if ((!isConnected && !isConnecting) || (!isExpanded && !query)) continue;

      if (isConnecting) {
        // The connection is still dialling: this spinner *is* the connection row.
        rows.push({
          type: 'db-loading',
          ...levels(CONNECTION_CHILD_DEPTH),
          ownerKey: `conn:${conn.id}`,
        });
        continue;
      }

      const dbSessionId = entry!.dbSessionId!;
      const schemaData = schemas.get(dbSessionId);
      if (!schemaData) {
        // The session exists but its schema has not arrived: owned by the session.
        rows.push({
          type: 'db-loading',
          ...levels(CONNECTION_CHILD_DEPTH),
          ownerKey: `session:${dbSessionId}`,
        });
        continue;
      }

      const meta = DB_REGISTRY[conn.databaseType];

      if (meta?.schemaTreeMode === 'custom' || meta?.namespaceEnsure === 'path-hierarchy') {
        const tree = schemaData.namespaceTree;
        const treeEmpty = isLeaf(tree) || Object.keys(tree).length === 0;
        if (treeEmpty) {
          if (!query && (schemaData.loading || schemaData.ensuringCount > 0)) {
            rows.push({
              type: 'db-loading',
              ...levels(CONNECTION_CHILD_DEPTH),
              ownerKey: `session:${dbSessionId}`,
            });
          }
          continue;
        }
        const typeMap = new Map<string, TableInfo['tableType']>();
        for (const tbl of schemaData.tables) typeMap.set(tbl.name, tbl.tableType);
        flattenNamespaceTree(
          tree,
          conn.id,
          dbSessionId,
          CONNECTION_CHILD_DEPTH,
          rows,
          expandedDbs,
          query,
          typeMap,
          schemaData.loadedPaths,
        );
        continue;
      }

      if (meta?.isKeyValue) {
        const dbs = schemaData.databases;
        if (schemaData.loading && dbs.length === 0) {
          // The key-value db list itself is still loading: owned by the session.
          rows.push({
            type: 'db-loading',
            ...levels(CONNECTION_CHILD_DEPTH),
            ownerKey: `session:${dbSessionId}`,
          });
        } else {
          const filteredDbs = query ? dbs.filter((d) => d.toLowerCase().includes(query)) : dbs;
          for (const dbName of filteredDbs) {
            rows.push({
              type: 'kv-db',
              connectionId: conn.id,
              dbSessionId,
              dbName,
              ...levels(CONNECTION_CHILD_DEPTH),
              isSelected: false,
              dbCountsCommand: meta.dbCountsCommand,
            });
          }
        }
        continue;
      }

      const isMultiDb = shouldUseMultiDatabaseTree(meta, conn.database);

      if (isMultiDb) {
        let dbs = query
          ? schemaData.databases.filter((d) => {
              if (d.toLowerCase().includes(query)) return true;
              const tblKey = `${dbSessionId}::${d}`;
              const tbls = dbTablesMap[tblKey];
              return tbls?.some((tbl) => tbl.name.toLowerCase().includes(query)) ?? false;
            })
          : schemaData.databases;
        dbs = dbs.filter((d) => shouldShowDatabase(d, objectFilter));

        for (const dbName of dbs) {
          const dbKey = `${conn.id}::${dbName}`;
          const tableKey = `${dbSessionId}::${dbName}`;
          const isDbExpanded = isOpen(expandedDbs.has(dbKey));
          const isLoading = loadingDbs.has(tableKey);

          rows.push({
            type: 'db',
            connectionId: conn.id,
            dbSessionId,
            dbName,
            expanded: isDbExpanded,
            loading: isLoading,
            isOpen: isDatabaseOpen(expandedDbs, openDbs, dbSessionId, dbName, conn.id),
            ...levels(CONNECTION_CHILD_DEPTH),
          });

          if (!isDbExpanded) continue;
          if (isLoading) {
            // This database's tables are loading: owned by that database.
            rows.push({
              type: 'db-loading',
              ...levels(DATABASE_CHILD_DEPTH),
              ownerKey: `db:${conn.id}::${dbName}`,
            });
            continue;
          }

          const allItems = dbTablesMap[tableKey] ?? [];
          const dbSchemaNames = [
            ...new Set(allItems.map((i) => i.schema).filter((s): s is string => !!s)),
          ];
          let schemaGroups = groupBySchema(allItems, dbSchemaNames);

          if (schemaGroups) {
            const schemaKeys = [...schemaGroups.keys()];
            if (schemaKeys.length === 1 && schemaKeys[0] === dbName) {
              schemaGroups = null;
            }
          }

          if (schemaGroups) {
            const preferred = DB_REGISTRY[conn.databaseType]?.defaultSchema;
            const sortedSchemas = [...schemaGroups.keys()].sort((a, b) => {
              if (preferred) {
                if (a === preferred) return -1;
                if (b === preferred) return 1;
              }
              return a.localeCompare(b);
            });

            for (const schemaName of sortedSchemas) {
              if (!shouldShowSchema(schemaName, objectFilter)) continue;
              const schemaKey = `${conn.id}::${dbName}::${schemaName}`;
              const schemaItems = schemaGroups.get(schemaName) ?? [];
              const schemaExpanded = isOpen(expandedSchemas.has(schemaKey));

              rows.push({
                type: 'schema',
                connectionId: conn.id,
                dbName,
                schemaName,
                expanded: schemaExpanded,
                ...levels(DATABASE_CHILD_DEPTH),
              });

              if (schemaExpanded) {
                addCategories(
                  schemaItems,
                  conn.id,
                  dbSessionId,
                  dbName,
                  schemaName,
                  DATABASE_CHILD_DEPTH + 1,
                  conn.databaseType,
                  objectFilter,
                );
              }
            }
          } else {
            addCategories(
              allItems,
              conn.id,
              dbSessionId,
              dbName,
              undefined,
              DATABASE_CHILD_DEPTH,
              conn.databaseType,
              objectFilter,
            );
          }
        }
      } else {
        const dbName = schemaData.currentDatabase ?? conn.database ?? '';
        if (!dbName) continue;

        const dbKey = `${conn.id}::${dbName}`;
        const isDbExpanded = expandedDbs.has(dbKey);

        rows.push({
          type: 'db',
          connectionId: conn.id,
          dbSessionId,
          dbName,
          expanded: isDbExpanded,
          loading: schemaData.loading && schemaData.tables.length === 0,
          isOpen: isDatabaseOpen(expandedDbs, openDbs, dbSessionId, dbName, conn.id),
          ...levels(CONNECTION_CHILD_DEPTH),
        });

        if (!isDbExpanded) continue;

        if (schemaData.loading && schemaData.tables.length === 0) {
          rows.push({
            type: 'db-loading',
            ...levels(DATABASE_CHILD_DEPTH),
            ownerKey: `db:${conn.id}::${dbName}`,
          });
          continue;
        }

        const allItems = [...schemaData.tables, ...schemaData.views];
        const schemaGroups = groupBySchema(allItems, schemaData.schemaNames);

        if (schemaGroups) {
          const preferred = DB_REGISTRY[conn.databaseType]?.defaultSchema;
          const sortedSchemas = [...schemaGroups.keys()].sort((a, b) => {
            if (preferred) {
              if (a === preferred) return -1;
              if (b === preferred) return 1;
            }
            return a.localeCompare(b);
          });

          for (const schemaName of sortedSchemas) {
            if (!shouldShowSchema(schemaName, objectFilter)) continue;
            const schemaKey = `${conn.id}::${dbName}::${schemaName}`;
            const schemaItems = schemaGroups.get(schemaName) ?? [];
            const schemaExpanded = isOpen(expandedSchemas.has(schemaKey));

            rows.push({
              type: 'schema',
              connectionId: conn.id,
              dbName,
              schemaName,
              expanded: schemaExpanded,
              ...levels(DATABASE_CHILD_DEPTH),
            });

            if (schemaExpanded) {
              addCategories(
                schemaItems,
                conn.id,
                dbSessionId,
                dbName,
                schemaName,
                DATABASE_CHILD_DEPTH + 1,
                conn.databaseType,
                objectFilter,
              );
            }
          }
        } else {
          addCategories(
            allItems,
            conn.id,
            dbSessionId,
            dbName,
            undefined,
            DATABASE_CHILD_DEPTH,
            conn.databaseType,
            objectFilter,
          );
        }
      }
    }
  }

  return rows;
}
