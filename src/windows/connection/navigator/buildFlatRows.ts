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
import { shouldUseMultiDatabaseTree } from '../schema-tree/SchemaTree';
import { getCategoriesForDriver } from '../schema-tree/schemaTreeCategories';
import type { UnifiedRow } from './types';
import { flattenNamespaceTree, groupBySchema } from './utils';

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
    query,
    t,
  } = params;

  const rows: UnifiedRow[] = [];

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
      const isExpanded = expandedCats.has(catKey);

      let count = 0;
      if (cat.id === 'tables') count = tblItems.length;
      else if (cat.id === 'views') count = viewItems.length;
      else count = (dbObjectsMap[catKey] ?? []).length;

      rows.push({
        type: 'category',
        key: catKey,
        cat,
        count,
        expanded: isExpanded,
        depth: baseDepth,
      });

      if (isExpanded) {
        let items: TableInfo[] = [];
        let objs: DatabaseObject[] = [];
        if (cat.id === 'tables') items = tblItems;
        else if (cat.id === 'views') items = viewItems;
        else objs = dbObjectsMap[catKey] ?? [];
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
              depth: baseDepth + 1,
              catId: cat.id,
              isSelected: false,
              connectionId,
              dbSessionId,
              dbName,
            });
          }
        } else if (objs.length > 0) {
          for (const obj of objs) {
            rows.push({ type: 'object', obj, depth: baseDepth + 1, catId: cat.id });
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
    rows.push({ type: 'no-connections' });
    return rows;
  }
  for (const { group: groupName, connections: groupConns } of sections) {
    const filteredConns = groupConns;
    if (query && filteredConns.length === 0) continue;

    const isPinnedSection = groupName === PINNED_GROUP_KEY;
    const isRecentSection = groupName === RECENT_GROUP_KEY;
    const expanded = isPinnedSection || isRecentSection || expandedGroups.has(groupName) || !!query;
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
        section: isPinnedSection ? 'pinned' : 'recent',
        displayName,
        count: filteredConns.length,
      });
    } else if (!query) {
      rows.push({
        type: 'group',
        groupName,
        displayName,
        count: filteredConns.length,
        expanded,
      });
    }

    if (!expanded) continue;

    if (filteredConns.length === 0) {
      rows.push({ type: 'empty-group' });
      continue;
    }

    for (const conn of filteredConns) {
      const objectFilter = getObjectFilter(conn);
      const entry = activeConnections[conn.id];
      const status = entry?.status ?? 'idle';
      const isConnected = status === 'connected';
      const isExpanded =
        expandedConnections.has(connectionExpandKey(groupName, conn.id)) || !!query;

      rows.push({
        type: 'connection',
        conn,
        sectionGroup: groupName,
        isSelected: activeConnectionId === conn.id,
        status,
        expanded: (isExpanded && isConnected) || !!query,
        depth: query ? 0 : 1,
        match: matchesById.get(conn.id)?.match ?? undefined,
      });

      if (!isConnected || (!isExpanded && !query)) continue;

      const dbSessionId = entry!.dbSessionId!;
      const schemaData = schemas.get(dbSessionId);
      if (!schemaData) {
        rows.push({ type: 'db-loading', depth: 2 });
        continue;
      }

      const meta = DB_REGISTRY[conn.databaseType];

      if (meta?.schemaTreeMode === 'custom' || meta?.namespaceEnsure === 'path-hierarchy') {
        const tree = schemaData.namespaceTree;
        const treeEmpty = isLeaf(tree) || Object.keys(tree).length === 0;
        if (treeEmpty) {
          if (!query && (schemaData.loading || schemaData.ensuringCount > 0)) {
            rows.push({ type: 'db-loading', depth: 2 });
          }
          continue;
        }
        const typeMap = new Map<string, TableInfo['tableType']>();
        for (const tbl of schemaData.tables) typeMap.set(tbl.name, tbl.tableType);
        flattenNamespaceTree(
          tree,
          conn.id,
          dbSessionId,
          2,
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
          rows.push({ type: 'db-loading', depth: 2 });
        } else {
          const filteredDbs = query ? dbs.filter((d) => d.toLowerCase().includes(query)) : dbs;
          for (const dbName of filteredDbs) {
            rows.push({
              type: 'kv-db',
              connectionId: conn.id,
              dbSessionId,
              dbName,
              depth: 2,
              isSelected: false,
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
          const isDbExpanded = expandedDbs.has(dbKey) || !!query;
          const isLoading = loadingDbs.has(tableKey);

          rows.push({
            type: 'db',
            connectionId: conn.id,
            dbSessionId,
            dbName,
            expanded: isDbExpanded,
            loading: isLoading,
            depth: 2,
          });

          if (!isDbExpanded) continue;
          if (isLoading) {
            rows.push({ type: 'db-loading', depth: 3 });
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
              const schemaExpanded = expandedSchemas.has(schemaKey) || !!query;

              rows.push({
                type: 'schema',
                connectionId: conn.id,
                dbName,
                schemaName,
                expanded: schemaExpanded,
                depth: 3,
              });

              if (schemaExpanded) {
                addCategories(
                  schemaItems,
                  conn.id,
                  dbSessionId,
                  dbName,
                  schemaName,
                  4,
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
              3,
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
          depth: 2,
        });

        if (!isDbExpanded) continue;

        if (schemaData.loading && schemaData.tables.length === 0) {
          rows.push({ type: 'db-loading', depth: 3 });
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
            const schemaExpanded = expandedSchemas.has(schemaKey) || !!query;

            rows.push({
              type: 'schema',
              connectionId: conn.id,
              dbName,
              schemaName,
              expanded: schemaExpanded,
              depth: 3,
            });

            if (schemaExpanded) {
              addCategories(
                schemaItems,
                conn.id,
                dbSessionId,
                dbName,
                schemaName,
                4,
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
            3,
            conn.databaseType,
            objectFilter,
          );
        }
      }
    }
  }

  return rows;
}
