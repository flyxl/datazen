import type {
  DatabaseObject,
  DatabaseObjectKind,
  DatabaseType,
  TableInfo,
  TableType,
} from '../types';
import type { TableSqlActionKind } from './tableSqlActions';

/** Object kinds exposed by the desktop-wide schema search. */
export type ObjectSearchObjectType =
  | 'table'
  | 'view'
  | 'column'
  | 'function'
  | 'routine'
  | 'procedure'
  | 'trigger'
  | 'sequence'
  | 'type';

/** One loaded database/schema slice of the existing schema cache. */
export interface SchemaObjectIndexEntry {
  connectionId: string;
  dbSessionId: string;
  databaseType: DatabaseType;
  connectionName?: string;
  database?: string | null;
  schema?: string | null;
  /** `TableInfo.tableType` decides whether each relation is a table or view. */
  tables?: readonly TableInfo[];
  /** Optional convenience for callers that already split tables and views. */
  views?: readonly TableInfo[];
  objects?: readonly DatabaseObject[];
  /** Existing schemaStore shape: table name (or qualified table name) → columns. */
  columns?: Readonly<Record<string, readonly string[]>>;
  /** Alias accepted for callers passing the schemaStore field verbatim. */
  columnMap?: Readonly<Record<string, readonly string[]>>;
}

/** Object form for callers that keep all loaded slices under an `entries` field. */
export interface SchemaObjectIndex {
  entries: readonly SchemaObjectIndexEntry[];
}

export type SchemaObjectSearchIndex =
  | readonly SchemaObjectIndexEntry[]
  | SchemaObjectIndex;

export interface ObjectSearchFilters {
  connectionId?: string;
  connectionIds?: readonly string[];
  database?: string | null;
  databases?: readonly (string | null | undefined)[];
  schema?: string | null;
  schemas?: readonly (string | null | undefined)[];
  objectType?: ObjectSearchObjectType;
  objectTypes?: readonly ObjectSearchObjectType[];
  /** Short alias useful for select controls. */
  types?: readonly ObjectSearchObjectType[];
}

export interface ObjectSearchResult {
  /** Stable identity for React/list consumers; it is not a database identifier. */
  id: string;
  connectionId: string;
  dbSessionId: string;
  databaseType: DatabaseType;
  connectionName?: string;
  database?: string;
  schema?: string;
  objectType: ObjectSearchObjectType;
  name: string;
  /** Explicit alias for consumers that call the field objectName. */
  objectName: string;
  /** Present for column hits so the caller can open/highlight the owning table. */
  tableName?: string;
  /** Original driver/schema kind when it is more specific than objectType. */
  sourceKind?: DatabaseObjectKind | TableType;
  /** Stable action ids; construction of executable UI actions stays separate. */
  actions: readonly TableSqlActionKind[];
}

export interface ObjectSearchResultGroup {
  key: string;
  connectionId: string;
  connectionName?: string;
  database?: string;
  schema?: string;
  objectType: ObjectSearchObjectType;
  results: ObjectSearchResult[];
}

const TABLE_OBJECT_ACTIONS: readonly TableSqlActionKind[] = [
  'openData',
  'select',
  'insert',
  'update',
  'ddl',
];

const ROUTINE_ACTIONS: readonly TableSqlActionKind[] = ['ddl'];

function normalizeText(value: string | null | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function lower(value: string | null | undefined): string {
  return value?.toLocaleLowerCase() ?? '';
}

function relationObjectType(tableType: TableType): 'table' | 'view' {
  return tableType === 'view' || tableType === 'materializedView' ? 'view' : 'table';
}

function objectObjectType(kind: DatabaseObjectKind): ObjectSearchObjectType {
  return kind === 'procedure' ? 'routine' : kind;
}

function resultId(result: Pick<
  ObjectSearchResult,
  'connectionId' | 'database' | 'schema' | 'objectType' | 'name' | 'tableName'
>): string {
  return JSON.stringify([
    result.connectionId,
    result.database ?? null,
    result.schema ?? null,
    result.objectType,
    result.tableName ?? null,
    result.name,
  ]);
}

function columnNamesFor(
  entry: SchemaObjectIndexEntry,
  table: TableInfo,
  database: string | undefined,
  schema: string | undefined,
): string[] {
  const maps = [entry.columns, entry.columnMap];
  const keys = [
    database && schema ? `${database}.${schema}.${table.name}` : undefined,
    schema ? `${schema}.${table.name}` : undefined,
    table.name,
  ].filter((key): key is string => Boolean(key));

  for (const map of maps) {
    if (!map) continue;
    for (const key of keys) {
      const names = map[key];
      if (names) return [...new Set(names.map((name) => name.trim()).filter(Boolean))];
    }
  }
  return [];
}

function normalizeIndex(index: SchemaObjectSearchIndex): readonly SchemaObjectIndexEntry[] {
  if (Array.isArray(index)) return index as readonly SchemaObjectIndexEntry[];
  return (index as SchemaObjectIndex).entries;
}

function filterValueMatches(
  actual: string | undefined,
  exact?: string | null,
  values?: readonly (string | null | undefined)[],
): boolean {
  const normalizedActual = normalizeText(actual);
  if (exact !== undefined && normalizedActual !== normalizeText(exact)) return false;
  if (
    values &&
    values.length > 0 &&
    !values.some((value) => normalizedActual === normalizeText(value))
  ) {
    return false;
  }
  return true;
}

function typeMatches(
  result: ObjectSearchResult,
  types: readonly ObjectSearchObjectType[] | undefined,
): boolean {
  if (!types || types.length === 0) return true;
  return types.some((type) => {
    if (type === 'routine') {
      return result.objectType === 'function' || result.objectType === 'routine';
    }
    if (type === 'procedure') return result.sourceKind === 'procedure';
    return result.objectType === type;
  });
}

function matchesFilter(result: ObjectSearchResult, filters: ObjectSearchFilters): boolean {
  if (filters.connectionId !== undefined && result.connectionId !== filters.connectionId) {
    return false;
  }
  if (
    filters.connectionIds &&
    filters.connectionIds.length > 0 &&
    !filters.connectionIds.includes(result.connectionId)
  ) {
    return false;
  }
  if (!filterValueMatches(result.database, filters.database, filters.databases)) return false;
  if (!filterValueMatches(result.schema, filters.schema, filters.schemas)) return false;
  const types = [
    ...(filters.objectType ? [filters.objectType] : []),
    ...(filters.objectTypes ?? []),
    ...(filters.types ?? []),
  ];
  return typeMatches(result, types.length > 0 ? types : undefined);
}

function resultMatchesQuery(result: ObjectSearchResult, query: string): boolean {
  if (!query) return true;
  const q = lower(query);
  return [
    result.connectionName,
    result.database,
    result.schema,
    result.name,
    result.tableName,
  ].some((value) => lower(value).includes(q));
}

function compareResults(a: ObjectSearchResult, b: ObjectSearchResult): number {
  const context = [
    lower(a.connectionName).localeCompare(lower(b.connectionName)),
    lower(a.connectionId).localeCompare(lower(b.connectionId)),
    lower(a.database).localeCompare(lower(b.database)),
    lower(a.schema).localeCompare(lower(b.schema)),
  ];
  for (const comparison of context) {
    if (comparison !== 0) return comparison;
  }

  const typeOrder: ObjectSearchObjectType[] = [
    'table',
    'view',
    'column',
    'function',
    'routine',
    'procedure',
    'trigger',
    'sequence',
    'type',
  ];
  const typeComparison = typeOrder.indexOf(a.objectType) - typeOrder.indexOf(b.objectType);
  if (typeComparison !== 0) return typeComparison;
  return lower(a.tableName ?? a.name).localeCompare(lower(b.tableName ?? b.name)) ||
    lower(a.name).localeCompare(lower(b.name));
}

/**
 * Search only loaded schema slices. No command, credential, or SQL execution is performed.
 * Empty query returns every result that passes the structural filters.
 */
export function searchSchemaObjects(
  index: SchemaObjectSearchIndex,
  query: string,
  filters: ObjectSearchFilters = {},
): ObjectSearchResult[] {
  const results: ObjectSearchResult[] = [];

  for (const entry of normalizeIndex(index)) {
    const database = normalizeText(entry.database);
    const entrySchema = normalizeText(entry.schema);
    const relations = [...(entry.tables ?? []), ...(entry.views ?? [])];
    const relationKeys = new Set<string>();

    for (const table of relations) {
      const tableName = normalizeText(table.name);
      if (!tableName) continue;
      const schema = normalizeText(table.schema) ?? entrySchema;
      const objectType = relationObjectType(table.tableType);
      const relationKey = JSON.stringify([database, schema, objectType, tableName]);
      if (relationKeys.has(relationKey)) continue;
      relationKeys.add(relationKey);

      const relationResult: ObjectSearchResult = {
        id: '',
        connectionId: entry.connectionId,
        dbSessionId: entry.dbSessionId,
        databaseType: entry.databaseType,
        connectionName: normalizeText(entry.connectionName),
        database,
        schema,
        objectType,
        name: tableName,
        objectName: tableName,
        sourceKind: table.tableType,
        actions: TABLE_OBJECT_ACTIONS,
      };
      relationResult.id = resultId(relationResult);
      if (matchesFilter(relationResult, filters) && resultMatchesQuery(relationResult, query.trim())) {
        results.push(relationResult);
      }

      for (const columnName of columnNamesFor(entry, table, database, schema)) {
        const columnResult: ObjectSearchResult = {
          id: '',
          connectionId: entry.connectionId,
          dbSessionId: entry.dbSessionId,
          databaseType: entry.databaseType,
          connectionName: normalizeText(entry.connectionName),
          database,
          schema,
          objectType: 'column',
          name: columnName,
          objectName: columnName,
          tableName,
          actions: TABLE_OBJECT_ACTIONS,
        };
        columnResult.id = resultId(columnResult);
        if (matchesFilter(columnResult, filters) && resultMatchesQuery(columnResult, query.trim())) {
          results.push(columnResult);
        }
      }
    }

    for (const object of entry.objects ?? []) {
      const objectName = normalizeText(object.name);
      if (!objectName) continue;
      const schema = normalizeText(object.schema) ?? entrySchema;
      const objectType = objectObjectType(object.kind);
      const objectResult: ObjectSearchResult = {
        id: '',
        connectionId: entry.connectionId,
        dbSessionId: entry.dbSessionId,
        databaseType: entry.databaseType,
        connectionName: normalizeText(entry.connectionName),
        database,
        schema,
        objectType,
        name: objectName,
        objectName,
        sourceKind: object.kind,
        actions: ROUTINE_ACTIONS,
      };
      objectResult.id = resultId(objectResult);
      if (matchesFilter(objectResult, filters) && resultMatchesQuery(objectResult, query.trim())) {
        results.push(objectResult);
      }
    }
  }

  return results.sort(compareResults);
}

/** Pure object-type filter, including `routine` as function/procedure. */
export function filterSchemaObjectsByType(
  results: readonly ObjectSearchResult[],
  objectTypes: readonly ObjectSearchObjectType[],
): ObjectSearchResult[] {
  return results.filter((result) => typeMatches(result, objectTypes));
}

export const filterObjectSearchResultsByType = filterSchemaObjectsByType;

/** Group results by the complete connection/database/schema/object-type path. */
export function groupSchemaObjects(
  results: readonly ObjectSearchResult[],
): ObjectSearchResultGroup[] {
  const groups = new Map<string, ObjectSearchResultGroup>();
  for (const result of results) {
    const key = JSON.stringify([
      result.connectionId,
      result.database ?? null,
      result.schema ?? null,
      result.objectType,
    ]);
    const group = groups.get(key);
    if (group) {
      group.results.push(result);
      continue;
    }
    groups.set(key, {
      key,
      connectionId: result.connectionId,
      connectionName: result.connectionName,
      database: result.database,
      schema: result.schema,
      objectType: result.objectType,
      results: [result],
    });
  }

  return [...groups.values()]
    .map((group) => ({ ...group, results: [...group.results].sort(compareResults) }))
    .sort((a, b) => compareResults(a.results[0]!, b.results[0]!));
}

export const groupObjectSearchResults = groupSchemaObjects;

/** Match schema objects by table/view name or column name. */

export function tableMatchesObjectSearch(
  tableName: string,
  query: string,
  columns?: readonly string[] | null,
): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  if (tableName.toLowerCase().includes(q)) return true;
  if (q.length < 2 || !columns?.length) return false;
  return columns.some((col) => col.toLowerCase().includes(q));
}

/** Columns that matched the query (for UI hints). */
export function matchingColumns(
  query: string,
  columns?: readonly string[] | null,
): string[] {
  const q = query.trim().toLowerCase();
  if (q.length < 2 || !columns?.length) return [];
  return columns.filter((col) => col.toLowerCase().includes(q));
}
