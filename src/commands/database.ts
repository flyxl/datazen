import { invoke } from '@tauri-apps/api/core';
import type {
  FilterCondition,
  KeyDetail,
  KeyScanResult,
  MultiQueryResult,
  SortCondition,
  TableDataResult,
  TableInfo,
  TableSchema,
  Value,
} from '../types';

export interface CellUpdate {
  column: string;
  value: Value | null;
}

export interface RowUpdateBatch {
  setColumns: CellUpdate[];
  pkColumns: CellUpdate[];
}

export const databaseCommands = {
  getDatabases: (connectionId: string) =>
    invoke<string[]>('get_databases', { connectionId }),

  getTables: (connectionId: string, database: string) =>
    invoke<TableInfo[]>('get_tables', { connectionId, database }),

  getColumns: (connectionId: string, table: string) =>
    invoke<string[]>('get_columns', { connectionId, table }),

  getTableSchema: (connectionId: string, table: string) =>
    invoke<TableSchema>('get_table_schema', { connectionId, table }),

  getTableData: (params: {
    connectionId: string;
    table: string;
    page: number;
    pageSize: number;
    filters?: FilterCondition[];
    sorts?: SortCondition[];
    skipCount?: boolean;
  }) =>
    invoke<TableDataResult>('get_table_data', {
      connectionId: params.connectionId,
      table: params.table,
      page: params.page,
      pageSize: params.pageSize,
      filters: params.filters,
      sorts: params.sorts,
      skipCount: params.skipCount,
    }),

  executeSQL: (connectionId: string, sql: string) =>
    invoke<MultiQueryResult>('execute_query', { connectionId, sql }),

  kvScanKeys: (connectionId: string, dbIndex: number, pattern: string, cursor: number, count: number) =>
    invoke<KeyScanResult>('kv_scan_keys', { connectionId, dbIndex, pattern, cursor, count }),

  kvGetKey: (connectionId: string, dbIndex: number, key: string) =>
    invoke<KeyDetail>('kv_get_key', { connectionId, dbIndex, key }),

  commitRowUpdates: (connectionId: string, table: string, updates: RowUpdateBatch[]) =>
    invoke<void>('commit_row_updates', { connectionId, table, updates }),
};
