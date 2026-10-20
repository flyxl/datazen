import type { TargetCommandPhaseHook } from '../lib/session/targetCommandPhase';
import {
  databaseDiscoveryTarget,
  executeTargetCommand,
  metadataTarget,
} from '../lib/session/targetCommand';
import { invoke } from '@tauri-apps/api/core';
import type {
  FilterCondition,
  SortCondition,
  DatabaseObject,
  PrivilegeGrant,
  TableDataResult,
  TableInfo,
  TableSchema,
} from '../types';
import type {
  CommitPendingChangesResponse,
  PendingRowChange,
  RowChangePlan,
  TableChangeContext,
} from '../lib/tableChanges';
import { queryCommands } from './query';

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

const databaseDiscoveryInFlight = new Map<string, Promise<string[]>>();

export const databaseCommands = {
  getDatabases: (dbSessionId: string, connectionId?: string): Promise<string[]> => {
    // Dedicated endpoint sessions share connection-scoped discovery, while
    // callers without an explicit owner retain the active-session boundary.
    const key = JSON.stringify(
      connectionId === undefined ? ['session', dbSessionId] : ['connection', connectionId],
    );
    const existing = databaseDiscoveryInFlight.get(key);
    if (existing) return existing;
    const discovery = Promise.resolve()
      .then(
        async () =>
          (
            (await executeTargetCommand(databaseDiscoveryTarget(dbSessionId, connectionId), {
              command: 'list_databases',
              input: {},
            })) as { databases: string[] }
          ).databases,
      )
      .finally(() => {
        if (databaseDiscoveryInFlight.get(key) === discovery) {
          databaseDiscoveryInFlight.delete(key);
        }
      });
    databaseDiscoveryInFlight.set(key, discovery);
    return discovery;
  },

  /**
   * List a database's tables. `schema` is optional and only meaningful for
   * schema-aware engines (PostgreSQL, SQL Server); omit it to list every schema.
   */
  listCatalog: async (
    dbSessionId: string,
    database: string,
    schema?: string | null,
    connectionId?: string,
    onPhase?: TargetCommandPhaseHook,
  ) => {
    const catalog = (await executeTargetCommand(
      metadataTarget(dbSessionId, database, schema ?? null, undefined, connectionId),
      {
        command: 'list_catalog',
        input: { database, schema: schema ?? null },
      },
      'commandResult',
      onPhase,
    )) as import('@datazen/driver-sdk').ListCatalogOutput;
    if (!Array.isArray(catalog.relations) || !Array.isArray(catalog.schemas))
      throw new Error('Invalid catalog result');
    return catalog;
  },

  listTables: async (
    dbSessionId: string,
    database: string,
    schema?: string | null,
    connectionId?: string,
    onPhase?: TargetCommandPhaseHook,
  ): Promise<TableInfo[]> => {
    const catalog = await databaseCommands.listCatalog(
      dbSessionId,
      database,
      schema,
      connectionId,
      onPhase,
    );
    return catalog.relations.map((relation) => ({
      name: relation.ref.name,
      schema: relation.ref.schema,
      tableType: relation.kind,
      rowCount: relation.rowCount,
    }));
  },

  getErData: async (
    dbSessionId: string,
    database: string,
    schema?: string | null,
  ): Promise<TableSchema[]> => {
    // Capture ownership once, then carry exact catalog identities through every
    // read. ER metadata must never borrow the query editor's session resource.
    const target = metadataTarget(dbSessionId, database, schema ?? null);
    const catalog = (await executeTargetCommand(target, {
      command: 'list_catalog',
      input: { database, schema: schema ?? null },
    })) as import('@datazen/driver-sdk').ListCatalogOutput;
    if (!Array.isArray(catalog.relations)) throw new Error('Invalid catalog result');
    const schemas: TableSchema[] = [];
    for (const relation of catalog.relations) {
      const ref = relation.ref;
      const data = (await executeTargetCommand(
        {
          ...target,
          namespace: { ...target.namespace, database: ref.database, schema: ref.schema },
          object: { kind: 'relation', name: ref.name, signature: null },
        },
        {
          command: 'get_table_schema',
          input: { database: ref.database, schema: ref.schema, table: ref.name },
        },
      )) as { schema?: TableSchema } | null;
      if (!data?.schema || !Array.isArray(data.schema.columns))
        throw new Error('Invalid schema result');
      schemas.push(data.schema);
    }
    return schemas;
  },

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
        context: params.context,
        manualHandle: params.manualHandle ?? null,
        page: params.page,
        pageSize: params.pageSize,
        filters: params.filters,
        sorts: params.sorts,
        skipCount: params.skipCount,
        filterLogic: params.filterLogic,
      },
    }),

  executeSQL: (dbSessionId: string, sql: string) => queryCommands.executeQuery(dbSessionId, sql),

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

  getDatabaseObjects: async (
    dbSessionId: string,
    kind: string,
    database?: string | null,
    schema?: string | null,
    connectionId?: string,
  ): Promise<DatabaseObject[]> => {
    const result = (await executeTargetCommand(
      metadataTarget(dbSessionId, database ?? null, schema ?? null, undefined, connectionId),
      {
        command: 'list_objects',
        input: { kind, database: database ?? null, schema: schema ?? null },
      },
    )) as { objects?: DatabaseObject[] };
    if (!Array.isArray(result.objects)) throw new Error('Invalid object catalog result');
    return result.objects;
  },

  getObjectDdl: async (
    dbSessionId: string,
    kind: string,
    name: string,
    schema?: string | null,
    signature?: string | null,
    targetSchema?: string | null,
    targetName?: string | null,
    database?: string | null,
  ): Promise<string> => {
    const result = (await executeTargetCommand(
      metadataTarget(dbSessionId, database ?? null, schema ?? null, name),
      {
        command: 'get_object_ddl',
        input: {
          kind,
          name,
          schema: schema ?? null,
          signature: signature ?? null,
          targetSchema: targetSchema ?? null,
          targetName: targetName ?? null,
          database: database ?? null,
        },
      },
    )) as { ddl?: string };
    if (typeof result.ddl !== 'string') throw new Error('Invalid object DDL result');
    return result.ddl;
  },

  getPrivileges: (dbSessionId: string) =>
    invoke<PrivilegeGrant[]>('get_privileges', { dbSessionId }),
};
