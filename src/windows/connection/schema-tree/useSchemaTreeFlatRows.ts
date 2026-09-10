import { useCallback, useMemo } from 'react';
import { matchingColumns, tableMatchesObjectSearch } from '../../../lib/schemaObjectSearch';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import type { DatabaseType } from '../../../extensions/generated';
import type { DatabaseObject, TableInfo } from '../../../types';
import type { SchemaTreeCategoryDef } from './schemaTreeCategories';
import type { FlatRow } from './SchemaTreeRow';

interface UseSchemaTreeFlatRowsOptions {
  isSingleDbMode: boolean;
  currentDatabase: string | null;
  databases: string[];
  dbExpanded: boolean;
  effectiveCategories: SchemaTreeCategoryDef[];
  expandedCats: Set<string>;
  tables: TableInfo[];
  views: TableInfo[];
  dbObjects: Record<string, DatabaseObject[]>;
  dbObjLoading: Set<string>;
  loading: boolean;
  trimmedQuery: string;
  query: string;
  columnMap: Record<string, string[]>;
  dbTables: Record<string, TableInfo[]>;
  expandedDbs: Set<string>;
  expandedSchemas: Set<string>;
  dbLoading: Set<string>;
  databaseType: DatabaseType;
}

function groupBySchema(items: TableInfo[]): Map<string, TableInfo[]> | null {
  const hasAnySchema = items.some((i) => !!i.schema);
  if (!hasAnySchema) return null;

  const map = new Map<string, TableInfo[]>();
  for (const item of items) {
    const key = item.schema ?? '';
    const arr = map.get(key);
    if (arr) arr.push(item);
    else map.set(key, [item]);
  }
  return map;
}

export function useSchemaTreeFlatRows(options: UseSchemaTreeFlatRowsOptions): FlatRow[] {
  const {
    isSingleDbMode,
    currentDatabase,
    databases,
    dbExpanded,
    effectiveCategories,
    expandedCats,
    tables,
    views,
    dbObjects,
    dbObjLoading,
    loading,
    trimmedQuery,
    query,
    columnMap,
    dbTables,
    expandedDbs,
    expandedSchemas,
    dbLoading,
    databaseType,
  } = options;

  const filterTableItems = useCallback(
    (items: TableInfo[]) => {
      if (!isSingleDbMode || !trimmedQuery) {
        return query ? items.filter((tbl) => tbl.name.toLowerCase().includes(query)) : items;
      }
      return items.filter((tbl) =>
        tableMatchesObjectSearch(tbl.name, trimmedQuery, columnMap[tbl.name]),
      );
    },
    [isSingleDbMode, trimmedQuery, query, columnMap],
  );

  const filterObjectItems = useCallback(
    (objs: DatabaseObject[]) => {
      if (!query) return objs;
      return objs.filter((o) => o.name.toLowerCase().includes(query));
    },
    [query],
  );

  const flatRows = useMemo<FlatRow[]>(() => {
    if (isSingleDbMode) {
      const rows: FlatRow[] = [];
      const dbName = currentDatabase ?? databases[0];
      if (dbName) {
        rows.push({ type: 'db', dbName, expanded: dbExpanded, loading: false, depth: 0 });
      }

      if (!dbExpanded || !dbName) {
        if (rows.length === 0 && !loading) rows.push({ type: 'empty', depth: 0 });
        return rows;
      }

      for (const cat of effectiveCategories) {
        const catKey = cat.id;
        const isExpanded = expandedCats.has(catKey);
        let count = 0;
        let tableItems: TableInfo[] = [];
        let objectItems: DatabaseObject[] = [];

        if (cat.id === 'tables') {
          tableItems = filterTableItems(tables);
          count = tableItems.length;
        } else if (cat.id === 'views') {
          tableItems = filterTableItems(views);
          count = tableItems.length;
        } else {
          objectItems = filterObjectItems(dbObjects[catKey] ?? []);
          count = objectItems.length;
        }

        rows.push({
          type: 'category',
          key: catKey,
          cat,
          count,
          expanded: isExpanded,
          depth: 1,
        });

        if (isExpanded) {
          if (tableItems.length > 0) {
            for (const item of tableItems) {
              const colHits =
                trimmedQuery.length >= 2 ? matchingColumns(trimmedQuery, columnMap[item.name]) : [];
              rows.push({ type: 'table', item, depth: 2, colHits });
            }
          } else if (objectItems.length > 0) {
            for (const obj of objectItems) rows.push({ type: 'object', obj, depth: 2 });
          } else if (!loading && !dbObjLoading.has(catKey)) {
            rows.push({ type: 'cat-empty', depth: 2 });
          }
        }
      }

      if (rows.length === 0 && !loading) rows.push({ type: 'empty', depth: 0 });
      return rows;
    }

    const filteredDbs = query
      ? databases.filter((d) => {
          if (d.toLowerCase().includes(query)) return true;
          const tbls = dbTables[d];
          return tbls?.some((tbl) => tbl.name.toLowerCase().includes(query)) ?? false;
        })
      : databases;

    const rows: FlatRow[] = [];

    const addCategoriesForItems = (
      allItems: TableInfo[],
      dbName: string,
      schemaName: string | undefined,
      baseDepth: number,
    ) => {
      const tblItems = allItems.filter(
        (i) => i.tableType === 'table' || i.tableType === 'systemTable',
      );
      const viewItems = allItems.filter(
        (i) => i.tableType === 'view' || i.tableType === 'materializedView',
      );

      for (const cat of effectiveCategories) {
        const catKey = schemaName ? `${dbName}::${schemaName}::${cat.id}` : `${dbName}::${cat.id}`;
        const isExpanded = expandedCats.has(catKey);
        let count = 0;

        if (cat.id === 'tables') count = tblItems.length;
        else if (cat.id === 'views') count = viewItems.length;
        else count = (dbObjects[catKey] ?? []).length;

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
          else objs = dbObjects[catKey] ?? [];

          if (items.length > 0) {
            for (const item of items) {
              const enriched = schemaName && !item.schema ? { ...item, schema: schemaName } : item;
              rows.push({ type: 'table', item: enriched, depth: baseDepth + 1, colHits: [] });
            }
          } else if (objs.length > 0) {
            for (const obj of objs) rows.push({ type: 'object', obj, depth: baseDepth + 1 });
          } else {
            rows.push({ type: 'cat-empty', depth: baseDepth + 1 });
          }
        }
      }
    };

    for (const dbName of filteredDbs) {
      const allItems = dbTables[dbName] ?? [];
      const dbNameMatches = query && dbName.toLowerCase().includes(query);
      const filteredDbItems =
        query && !dbNameMatches
          ? allItems.filter((tbl) => tbl.name.toLowerCase().includes(query))
          : allItems;
      const hasTableMatch = !!(query && filteredDbItems.length > 0);
      const isExpanded = expandedDbs.has(dbName) || hasTableMatch;
      const isLoading = dbLoading.has(dbName);

      rows.push({ type: 'db', dbName, expanded: isExpanded, loading: isLoading, depth: 0 });

      if (!isExpanded) continue;

      if (isLoading) {
        rows.push({ type: 'db-loading', dbName, depth: 1 });
        continue;
      }

      const schemaGroups = groupBySchema(filteredDbItems);

      if (!schemaGroups) {
        addCategoriesForItems(filteredDbItems, dbName, undefined, 1);
      } else {
        const preferred = DB_REGISTRY[databaseType]?.defaultSchema;
        const sortedSchemas = [...schemaGroups.keys()].sort((a, b) => {
          if (preferred) {
            if (a === preferred) return -1;
            if (b === preferred) return 1;
          }
          return a.localeCompare(b);
        });

        for (const schemaName of sortedSchemas) {
          const schemaKey = `${dbName}::${schemaName}`;
          const schemaItems = schemaGroups.get(schemaName) ?? [];
          const schemaExpanded = expandedSchemas.has(schemaKey) || !!query;

          rows.push({
            type: 'schema',
            dbName,
            schemaName,
            expanded: schemaExpanded,
            depth: 1,
          });

          if (schemaExpanded) {
            addCategoriesForItems(schemaItems, dbName, schemaName, 2);
          }
        }
      }
    }

    if (rows.length === 0 && !loading) {
      rows.push({ type: 'empty', depth: 0 });
    }

    return rows;
  }, [
    isSingleDbMode,
    currentDatabase,
    databases,
    dbExpanded,
    effectiveCategories,
    expandedCats,
    tables,
    views,
    dbObjects,
    dbObjLoading,
    loading,
    filterTableItems,
    filterObjectItems,
    trimmedQuery,
    columnMap,
    query,
    dbTables,
    expandedDbs,
    expandedSchemas,
    dbLoading,
    databaseType,
  ]);

  return flatRows;
}
