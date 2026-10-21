import type { TargetCommandPhaseHook } from '../lib/session/targetCommandPhase';
import {
  databaseDiscoveryTarget,
  executeTargetCommand,
  metadataTarget,
} from '../lib/session/targetCommand';
import { invoke } from '@tauri-apps/api/core';
import { useBackendClient } from '@datazen/backend-client';
import type {
  ForeignKeyTargetResolution,
  NamedView,
  NamedViewScope,
  ResolveForeignKeyTargetsRequest,
} from '@datazen/backend-client';
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

  /**
   * Read one page of a table.
   *
   * The read goes through the bound `BackendClient`, so the desktop and a
   * browser build take the same command path; the request shape the backend
   * sees is unchanged.
   */
  getTableData: async (params: {
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
  }): Promise<TableDataResult> => {
    const client = useBackendClient();
    return client.getTableData({
      context: params.context,
      manualHandle: params.manualHandle ?? null,
      page: params.page,
      pageSize: params.pageSize,
      filters: params.filters,
      sorts: params.sorts,
      skipCount: params.skipCount,
      filterLogic: params.filterLogic,
    });
  },

  executeSQL: (dbSessionId: string, sql: string) => queryCommands.executeQuery(dbSessionId, sql),

  /** Generate a serialisable plan only; the backend does not execute SQL. */
  previewPendingChanges: async (params: PreviewPendingChangesRequest): Promise<RowChangePlan> => {
    const client = useBackendClient();
    return client.previewPendingChanges({
      context: params.context,
      changes: params.changes,
    });
  },

  /** Commit the exact plan returned by preview, guarded by its fingerprint. */
  commitPendingChanges: async (
    params: CommitPendingChangesRequest,
  ): Promise<CommitPendingChangesResponse> => {
    const client = useBackendClient();
    return client.commitPendingChanges({
      dbSessionId: params.dbSessionId,
      plan: params.plan,
      manualHandle: params.manualHandle ?? null,
      fingerprint: params.fingerprint,
    });
  },

  /**
   * The estimated row count of a table, or `null` when the driver cannot
   * estimate it. `null` is an answer, not a failure: falling back to an exact
   * count is the caller's decision.
   *
   * **Not wired to any UI yet.** The command belongs to 09 (count strategy),
   * and the desktop does not register it yet, so the desktop transport
   * declares it unsupported and this call fails with
   * `CapabilityUnsupported`. The wrapper exists because the port registers
   * the method and a wrapper that does not exist cannot be called by mistake;
   * the UI entry point arrives with 09, and this call then starts working.
   */
  estimateTableRows: async (
    dbSessionId: string,
    table: string,
    database?: string | null,
    schema?: string | null,
  ): Promise<number | null> => {
    const client = useBackendClient();
    return client.estimateTableRows({ dbSessionId, table, database, schema });
  },

  /**
   * Resolve the foreign key a local column points at, and whether the
   * referenced row still exists.
   *
   * The desktop **does** register the command (07), so this call reaches the
   * host. The UI entry point is still to come: the grid's foreign-key arrow
   * is the next wave's work, and until then nothing calls this method.
   */
  resolveForeignKeyTargets: async (
    params: ResolveForeignKeyTargetsRequest,
  ): Promise<ForeignKeyTargetResolution> => {
    const client = useBackendClient();
    return client.resolveForeignKeyTargets(params);
  },

  /**
   * The saved views of one scope (or of every scope when `scope` is omitted).
   *
   * Named views are **desktop-only** for now: their storage is the desktop
   * host's persistent store, which a browser build has no counterpart for. A
   * backend that cannot serve them fails this call with
   * `CapabilityUnsupported` instead of answering with an empty list that reads
   * like "you have no saved views".
   *
   * **Not wired to any UI yet.** The command belongs to 08, and the desktop
   * does not register it yet either, so the desktop transport declares it
   * unsupported. The menu entry arrives with 08.
   */
  listNamedViews: async (scope?: NamedViewScope): Promise<readonly NamedView[]> => {
    const client = useBackendClient();
    return client.listNamedViews({ scope });
  },

  /**
   * Save a named view; the backend mints the id and stamps the timestamps.
   *
   * **Not wired to any UI yet** — see `listNamedViews`.
   */
  saveNamedView: async (view: NamedView): Promise<NamedView> => {
    const client = useBackendClient();
    return client.saveNamedView({ view });
  },

  /**
   * Delete a named view; the remaining views come back with the receipt.
   *
   * **Not wired to any UI yet** — see `listNamedViews`.
   */
  deleteNamedView: async (id: string): Promise<readonly NamedView[]> => {
    const client = useBackendClient();
    return client.deleteNamedView({ id });
  },

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
