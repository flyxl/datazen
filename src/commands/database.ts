import { databaseDiscoveryTarget, executeTargetCommand, metadataTarget } from '../lib/session/targetCommand';
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
  manualHandle?: import('@datazen/backend-client').SessionHandle | null;
  dbSessionId: string;
  plan: RowChangePlan;
  fingerprint: string;
}

export const databaseCommands = {
  getDatabases: async (dbSessionId: string) =>
    (await executeTargetCommand(databaseDiscoveryTarget(dbSessionId), {
      command: 'list_databases', input: {},
    }) as { databases: string[] }).databases,

  /**
   * List a database's tables. `schema` is optional and only meaningful for
   * schema-aware engines (PostgreSQL, SQL Server); omit it to list every schema.
   */
  listCatalog: async (dbSessionId: string, database: string, schema?: string | null) => {
    const catalog = await executeTargetCommand(metadataTarget(dbSessionId, database, schema ?? null), {
      command: 'list_catalog', input: { database, schema: schema ?? null },
    }) as import('@datazen/driver-sdk').ListCatalogOutput;
    if (!Array.isArray(catalog.relations) || !Array.isArray(catalog.schemas)) throw new Error('Invalid catalog result');
    return catalog;
  },

  listTables: async (dbSessionId: string, database: string, schema?: string | null): Promise<TableInfo[]> => {
    const catalog = await databaseCommands.listCatalog(dbSessionId, database, schema);
    return catalog.relations.map((relation) => ({ name: relation.ref.name, schema: relation.ref.schema,
      tableType: relation.kind, rowCount: relation.rowCount }));
  },

  getErData: (dbSessionId: string, database: string, schema?: string | null) =>
    invoke<TableSchema[]>('get_er_data', { dbSessionId, database, schema: schema ?? null }),

  getTableData: (params: {
    dbSessionId: string;
    context: TableChangeContext;
    manualHandle?: import('@datazen/backend-client').SessionHandle | null;
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
    invoke<TableDataResult>('get_table_data_at_target', {
      request: {
        context: params.context, manualHandle: params.manualHandle ?? null, page: params.page, pageSize: params.pageSize,
        filters: params.filters, sorts: params.sorts, skipCount: params.skipCount,
        filterLogic: params.filterLogic,
      },
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
      manualHandle: params.manualHandle ?? null,
      fingerprint: params.fingerprint,
    }),

  getDatabaseObjects: async (dbSessionId: string, kind: string, database?: string | null): Promise<DatabaseObject[]> => {
    const result = await executeTargetCommand(metadataTarget(dbSessionId, database ?? null, null), {
      command: 'list_objects', input: { kind, database: database ?? null },
    }) as { objects?: DatabaseObject[] };
    if (!Array.isArray(result.objects)) throw new Error('Invalid object catalog result');
    return result.objects;
  },

  getObjectDdl: async (
    dbSessionId: string, kind: string, name: string, schema?: string | null,
    signature?: string | null, targetSchema?: string | null, targetName?: string | null,
    database?: string | null,
  ): Promise<string> => {
    const result = await executeTargetCommand(metadataTarget(dbSessionId, database ?? null, schema ?? null, name), {
      command: 'get_object_ddl', input: { kind, name, schema: schema ?? null, signature: signature ?? null,
        targetSchema: targetSchema ?? null, targetName: targetName ?? null, database: database ?? null },
    }) as { ddl?: string };
    if (typeof result.ddl !== 'string') throw new Error('Invalid object DDL result');
    return result.ddl;
  },

  getPrivileges: (dbSessionId: string) =>
    invoke<PrivilegeGrant[]>('get_privileges', { dbSessionId }),
};
