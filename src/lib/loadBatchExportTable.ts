import { databaseCommands } from '../commands/database';
import type {
  ColumnSchema,
  DatabaseType,
  FilterCondition,
  SortCondition,
  TableDataResult,
  TableSchema,
  Value,
} from '../types';
import type { BatchExportTableInput } from './batchExport';
import { getCachedDDL, getCachedTableSchema } from './schemaCache';
import { useSchemaStore } from '../stores/schemaStore';
import { getSqlDialect, type SqlDialectStrategy } from './sqlDialects';

const DEFAULT_MAX_ROWS = 100_000;
const DEFAULT_PAGE_SIZE = 500;

export interface LoadBatchExportTableDeps {
  getSchema: (dbSessionId: string, tableName: string, database: string) => Promise<TableSchema>;
  getDdl: (
    dbSessionId: string,
    tableName: string,
    sql: string,
    resultExtractor: (rows: unknown[][]) => string,
    database: string,
  ) => Promise<string>;
  getTableData: (params: {
    dbSessionId: string;
    table: string;
    page: number;
    pageSize: number;
    filters?: FilterCondition[];
    sorts?: SortCondition[];
    skipCount?: boolean;
  }) => Promise<TableDataResult>;
  /** Defaults to getSqlDialect; injectable so unit tests need no DRIVER_DB_ENTRIES. */
  getDialect: (databaseType: string) => SqlDialectStrategy | null;
  /**
   * Schema the table lives in (PostgreSQL family), or null when the driver has
   * no schema level. The backend needs it to resolve the name against the right
   * namespace instead of the session default.
   */
  getRelationSchema: (dbSessionId: string, tableName: string) => string | null;
}

const defaultDeps: LoadBatchExportTableDeps = {
  getSchema: getCachedTableSchema,
  getDdl: getCachedDDL,
  getTableData: databaseCommands.getTableData,
  getDialect: (databaseType) => getSqlDialect(databaseType as DatabaseType),
  getRelationSchema: (dbSessionId, tableName) =>
    useSchemaStore.getState().schemaOfRelation(tableName, dbSessionId),
};

/** Convert 2D row arrays to named records (same logic as tableDataStore). */
export function rowsToRecords(
  columns: ColumnSchema[],
  rows: (Value | null)[][],
): Record<string, unknown>[] {
  return rows.map((row) => {
    const record: Record<string, unknown> = {};
    columns.forEach((col, i) => {
      record[col.name] = row[i] ?? null;
    });
    return record;
  });
}

function extractDdlString(rows: unknown[][], extractColumnIndex: number): string {
  const row = rows[0];
  if (!Array.isArray(row)) return '';
  const val = row[extractColumnIndex];
  return typeof val === 'string' ? val : val != null ? String(val) : '';
}

async function loadDdl(
  dbSessionId: string,
  tableName: string,
  database: string,
  databaseType: string | undefined,
  getDdl: LoadBatchExportTableDeps['getDdl'],
  getDialect: LoadBatchExportTableDeps['getDialect'],
): Promise<string | null> {
  if (!databaseType) return null;

  const dialect = getDialect(databaseType);
  if (!dialect?.ddl?.getTableDdlQuery) return null;

  try {
    const { sql, extractColumnIndex } = dialect.ddl.getTableDdlQuery(tableName);
    const ddl = await getDdl(
      dbSessionId,
      tableName,
      sql,
      (rows) => extractDdlString(rows, extractColumnIndex),
      database,
    );
    return ddl.trim() !== '' ? ddl : null;
  } catch {
    return null;
  }
}

async function loadAllRows(
  dbSessionId: string,
  tableName: string,
  pageSize: number,
  maxRows: number,
  getTableData: LoadBatchExportTableDeps['getTableData'],
): Promise<Record<string, unknown>[]> {
  const allRows: Record<string, unknown>[] = [];
  let page = 0;
  let totalRows: number | undefined;

  while (allRows.length < maxRows) {
    const res = await getTableData({
      dbSessionId: dbSessionId,
      table: tableName,
      page,
      pageSize,
      filters: [],
      sorts: [],
      skipCount: page > 0,
    });

    if (res.totalRows != null) {
      totalRows = res.totalRows;
    }

    const pageRecords = rowsToRecords(res.columns, res.rows);
    const remaining = maxRows - allRows.length;
    if (pageRecords.length > remaining) {
      allRows.push(...pageRecords.slice(0, remaining));
      break;
    }
    allRows.push(...pageRecords);

    if (pageRecords.length < pageSize) break;
    if (totalRows != null && allRows.length >= totalRows) break;

    page += 1;
  }

  return allRows;
}

export async function loadBatchExportTableData(params: {
  dbSessionId: string;
  tableName: string;
  database: string;
  databaseType?: string;
  /** max rows to pull (default 100_000); stop early if hit */
  maxRows?: number;
  pageSize?: number; // default 500
  /** When false, skip paging rows into memory (streaming export supplies data). */
  includeRows?: boolean;
  deps?: Partial<LoadBatchExportTableDeps>;
}): Promise<BatchExportTableInput> {
  const {
    dbSessionId,
    tableName,
    database,
    databaseType,
    maxRows = DEFAULT_MAX_ROWS,
    pageSize = DEFAULT_PAGE_SIZE,
    includeRows = true,
    deps: depsOverride,
  } = params;

  const deps: LoadBatchExportTableDeps = {
    ...defaultDeps,
    ...depsOverride,
  };

  const schema = await deps.getSchema(dbSessionId, tableName, database);
  const ddl = await loadDdl(
    dbSessionId,
    tableName,
    database,
    databaseType,
    deps.getDdl,
    deps.getDialect,
  );
  const rows = includeRows
    ? await loadAllRows(dbSessionId, tableName, pageSize, maxRows, deps.getTableData)
    : [];

  return {
    tableName,
    // The schema the backend must resolve this table against. PostgreSQL
    // cannot cross databases in one statement, so a bare name would read the
    // session's own database instead of the one the user selected.
    schema: deps.getRelationSchema(dbSessionId, tableName),
    ddl,
    columns: schema.columns.map((c) => ({ name: c.name })),
    rows,
  };
}
