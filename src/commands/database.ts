import { invoke } from '@tauri-apps/api/core';
import type {
  FilterCondition,
  KeyDetail,
  KeyScanResult,
  SortCondition,
  DatabaseObject,
  PrivilegeGrant,
  TableDataResult,
  TableInfo,
  TableSchema,
  Value,
} from '../types';
import { queryCommands } from './query';

export interface CellUpdate {
  column: string;
  value: Value | null;
}

export interface RowUpdateBatch {
  setColumns: CellUpdate[];
  pkColumns: CellUpdate[];
}

export interface RowDeleteBatch {
  pkColumns: CellUpdate[];
}

export const databaseCommands = {
  getDatabases: (connectionId: string) => invoke<string[]>('get_databases', { connectionId }),

  /** Switch active database for subsequent unqualified queries (MySQL/MariaDB). */
  useDatabase: (connectionId: string, database: string) =>
    invoke<void>('use_database', { connectionId, database }),

  getTables: (connectionId: string, database: string) =>
    invoke<TableInfo[]>('get_tables', { connectionId, database }),

  getColumns: (connectionId: string, table: string) =>
    invoke<string[]>('get_columns', { connectionId, table }),

  getTableSchema: (connectionId: string, table: string) =>
    invoke<TableSchema>('get_table_schema', { connectionId, table }),

  getErData: (connectionId: string, database: string) =>
    invoke<TableSchema[]>('get_er_data', { connectionId, database }),

  getTableData: (params: {
    connectionId: string;
    table: string;
    page: number;
    pageSize: number;
    filters?: FilterCondition[];
    sorts?: SortCondition[];
    skipCount?: boolean;
    filterLogic?: 'and' | 'or';
  }) =>
    invoke<TableDataResult>('get_table_data', {
      connectionId: params.connectionId,
      table: params.table,
      page: params.page,
      pageSize: params.pageSize,
      filters: params.filters,
      sorts: params.sorts,
      skipCount: params.skipCount,
      filterLogic: params.filterLogic,
    }),

  executeSQL: (connectionId: string, sql: string) => queryCommands.executeQuery(connectionId, sql),

  kvScanKeys: (
    connectionId: string,
    dbIndex: number,
    pattern: string,
    cursor: number,
    count: number,
  ) => invoke<KeyScanResult>('kv_scan_keys', { connectionId, dbIndex, pattern, cursor, count }),

  kvGetKey: (connectionId: string, dbIndex: number, key: string) =>
    invoke<KeyDetail>('kv_get_key', { connectionId, dbIndex, key }),

  commitRowUpdates: (connectionId: string, table: string, updates: RowUpdateBatch[]) =>
    invoke<void>('commit_row_updates', { connectionId, table, updates }),

  commitRowDeletes: (connectionId: string, table: string, deletes: RowDeleteBatch[]) =>
    invoke<void>('commit_row_deletes', { connectionId, table, deletes }),

  getDatabaseObjects: (connectionId: string, kind: string) =>
    invoke<DatabaseObject[]>('get_database_objects', { connectionId, kind }),

  getObjectDdl: (connectionId: string, kind: string, name: string, schema?: string | null) =>
    invoke<string>('get_object_ddl', { connectionId, kind, name, schema: schema ?? null }),

  getPrivileges: (connectionId: string) =>
    invoke<PrivilegeGrant[]>('get_privileges', { connectionId }),
};
