import { invoke } from '@tauri-apps/api/core';
import type {
  FilterCondition,
  SortCondition,
  DatabaseObject,
  PrivilegeGrant,
  TableDataResult,
  TableInfo,
  TableSchema,
  Value,
} from '../types';
import type {
  CommitPendingChangesResponse,
  PendingRowChange,
  RowChangePlan,
  TableChangeContext,
} from '../lib/tableChanges';
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

export interface PreviewPendingChangesRequest {
  context: TableChangeContext;
  changes: PendingRowChange[];
}

export interface CommitPendingChangesRequest {
  dbSessionId: string;
  plan: RowChangePlan;
  fingerprint: string;
}

export const databaseCommands = {
  getDatabases: (dbSessionId: string) => invoke<string[]>('get_databases', { dbSessionId }),

  getTables: (dbSessionId: string, database: string) =>
    invoke<TableInfo[]>('get_tables', { dbSessionId, database }),

  getColumns: (dbSessionId: string, table: string) =>
    invoke<string[]>('get_columns', { dbSessionId, table }),

  getTableSchema: (dbSessionId: string, table: string) =>
    invoke<TableSchema>('get_table_schema', { dbSessionId, table }),

  getErData: (dbSessionId: string, database: string) =>
    invoke<TableSchema[]>('get_er_data', { dbSessionId, database }),

  getTableData: (params: {
    dbSessionId: string;
    table: string;
    page: number;
    pageSize: number;
    filters?: FilterCondition[];
    sorts?: SortCondition[];
    skipCount?: boolean;
    filterLogic?: 'and' | 'or';
    /** F1: optional explicit target database — host pins the session before reading. */
    database?: string | null;
  }) =>
    invoke<TableDataResult>('get_table_data', {
      dbSessionId: params.dbSessionId,
      table: params.table,
      page: params.page,
      pageSize: params.pageSize,
      filters: params.filters,
      sorts: params.sorts,
      skipCount: params.skipCount,
      filterLogic: params.filterLogic,
      database: params.database ?? null,
    }),

  executeSQL: (dbSessionId: string, sql: string) => queryCommands.executeQuery(dbSessionId, sql),

  commitRowUpdates: (dbSessionId: string, table: string, updates: RowUpdateBatch[]) =>
    invoke<void>('commit_row_updates', { dbSessionId, table, updates }),

  commitRowDeletes: (dbSessionId: string, table: string, deletes: RowDeleteBatch[]) =>
    invoke<void>('commit_row_deletes', { dbSessionId, table, deletes }),

  /** Generate a serialisable plan only; the backend does not execute SQL. */
  previewPendingChanges: (params: PreviewPendingChangesRequest) =>
    invoke<RowChangePlan>('preview_pending_changes', {
      context: params.context,
      changes: params.changes,
    }),

  /** Commit the exact plan returned by preview, guarded by its fingerprint. */
  commitPendingChanges: (params: CommitPendingChangesRequest) =>
    invoke<CommitPendingChangesResponse>('commit_pending_changes', {
      dbSessionId: params.dbSessionId,
      plan: params.plan,
      fingerprint: params.fingerprint,
    }),

  getDatabaseObjects: (dbSessionId: string, kind: string) =>
    invoke<DatabaseObject[]>('get_database_objects', { dbSessionId, kind }),

  getObjectDdl: (dbSessionId: string, kind: string, name: string, schema?: string | null) =>
    invoke<string>('get_object_ddl', { dbSessionId, kind, name, schema: schema ?? null }),

  getPrivileges: (dbSessionId: string) =>
    invoke<PrivilegeGrant[]>('get_privileges', { dbSessionId }),
};
