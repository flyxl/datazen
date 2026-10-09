import { schemaClient } from '@datazen/driver-sdk';
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
  getDatabases: async (dbSessionId: string) =>
    (await schemaClient.listDatabases(dbSessionId)).databases,

  /**
   * List a database's tables. `schema` is optional and only meaningful for
   * schema-aware engines (PostgreSQL, SQL Server); omit it to list every schema.
   */
  listTables: async (
    dbSessionId: string,
    database: string,
    schema?: string | null,
  ): Promise<TableInfo[]> => {
    const catalog = await schemaClient.listCatalog(dbSessionId, {
      database,
      schema: schema ?? null,
    });
    const tables: TableInfo[] = catalog.relations.map((relation) => ({
      name: relation.ref.name,
      schema: relation.ref.schema,
      tableType: relation.kind,
      rowCount: relation.rowCount,
    }));
    // The legacy namespace tree still represents empty schemas with a sentinel.
    const populated = new Set(tables.map((table) => table.schema));
    for (const schema of catalog.schemas) {
      if (!populated.has(schema))
        tables.push({ name: '', schema, tableType: 'systemTable', rowCount: null });
    }
    return tables;
  },

  getErData: (dbSessionId: string, database: string, schema?: string | null) =>
    invoke<TableSchema[]>('get_er_data', { dbSessionId, database, schema: schema ?? null }),

  getTableData: (params: {
    dbSessionId: string;
    table: string;
    page: number;
    pageSize: number;
    filters?: FilterCondition[];
    sorts?: SortCondition[];
    skipCount?: boolean;
    filterLogic?: 'and' | 'or';
    /** Explicit target database; the host reads it without switching the session. */
    database?: string | null;
    /** Explicit target schema for schema-aware engines. */
    schema?: string | null;
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
      schema: params.schema ?? null,
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

  getDatabaseObjects: (dbSessionId: string, kind: string, database?: string | null) =>
    invoke<DatabaseObject[]>('get_database_objects', {
      dbSessionId,
      kind,
      database: database ?? null,
    }),

  getObjectDdl: (
    dbSessionId: string,
    kind: string,
    name: string,
    schema?: string | null,
    signature?: string | null,
    targetSchema?: string | null,
    targetName?: string | null,
    database?: string | null,
  ) =>
    invoke<string>('get_object_ddl', {
      dbSessionId,
      kind,
      name,
      schema: schema ?? null,
      signature: signature ?? null,
      targetSchema: targetSchema ?? null,
      targetName: targetName ?? null,
      database: database ?? null,
    }),

  getPrivileges: (dbSessionId: string) =>
    invoke<PrivilegeGrant[]>('get_privileges', { dbSessionId }),
};
