import { invoke } from '@tauri-apps/api/core';
import type {
  FilterCondition,
  MultiQueryResult,
  SortCondition,
  TableDataResult,
  TableInfo,
  TableSchema,
} from '../types';

export const databaseCommands = {
  getDatabases: (connectionId: string) =>
    invoke<string[]>('get_databases', { connectionId }),

  getTables: (connectionId: string, database: string) =>
    invoke<TableInfo[]>('get_tables', { connectionId, database }),

  getTableSchema: (connectionId: string, table: string) =>
    invoke<TableSchema>('get_table_schema', { connectionId, table }),

  getTableData: (params: {
    connectionId: string;
    table: string;
    page: number;
    pageSize: number;
    filters?: FilterCondition[];
    sorts?: SortCondition[];
  }) =>
    invoke<TableDataResult>('get_table_data', {
      connectionId: params.connectionId,
      table: params.table,
      page: params.page,
      pageSize: params.pageSize,
      filters: params.filters,
      sorts: params.sorts,
    }),

  executeSQL: (connectionId: string, sql: string) =>
    invoke<MultiQueryResult>('execute_query', { connectionId, sql }),
};
