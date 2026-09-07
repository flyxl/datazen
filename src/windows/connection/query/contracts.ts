import type { ConnectionSchemaState } from '../../../stores/schemaStore';
import type { QueryExecState } from '../../../stores/queryExecActions';
import type { SqlEditorHandle } from '../../../components/SqlEditor';
import type { MutableRefObject } from 'react';
import { usePanelStore, type QueryPanel as QueryPanelState } from '../../../stores/panelStore';
import { useActiveConnectionStore } from '../../../stores/activeConnectionStore';
import { useSchemaStore } from '../../../stores/schemaStore';
import { buildQueryDiagnosisContext, type RetryValidationInput } from '../../../lib/aiQueryActions';
import { parseSqlParams, paramsToPayload } from '../../../lib/sqlBindParams';
import type { ContentViewCallbacks } from './aiDraftBridge';

export interface QueryPanelProps {
  panelId: string;
  /** Live database session id used for every query this panel issues. */
  dbSessionId: string;
  /** Persistent saved-connection ID (stable across restarts). */
  connectionId: string;
  databaseType?: string;
  connectionName?: string;
  database?: string;
  schema?: string;
  namespacePath?: string[];
  /** S3-B2: optional navigation/AI-draft callbacks threaded from ContentView. */
  callbacks?: ContentViewCallbacks;
}

export type QueryDiagnosisSchemaState = Pick<
  ConnectionSchemaState,
  'currentDatabase' | 'currentSchema' | 'tables' | 'views' | 'columnMap'
>;

export type QueryDiagnosisExecution = Pick<QueryExecState, 'sql' | 'error'>;

export interface QueryPanelDiagnosisContextInput {
  execution: QueryDiagnosisExecution;
  connectionId: string;
  dbSessionId: string;
  databaseType?: string;
  connectionName?: string;
  database?: string;
  schema?: string;
  serverVersion?: string;
  schemaState: QueryDiagnosisSchemaState;
}

export type ExecuteKind = 'full' | 'selection';

export interface PendingExecute {
  kind: ExecuteKind;
  sql?: string;
}

export interface QueryExecutionGateRefs {
  editorRef: MutableRefObject<SqlEditorHandle | null>;
  pendingExecuteRef: MutableRefObject<PendingExecute | null>;
}

export function hasSuspiciousPostgresDoubleQuotedLiteral(sql: string): boolean {
  return /(?:=|<>|!=|\bLIKE\b|\bILIKE\b)\s*"[^"]+"/i.test(sql);
}

export function buildQueryPanelDiagnosisContext({
  execution,
  connectionId,
  dbSessionId,
  databaseType,
  connectionName,
  database,
  schema,
  serverVersion,
  schemaState,
}: QueryPanelDiagnosisContextInput) {
  return buildQueryDiagnosisContext({
    sql: execution.sql,
    error: execution.error,
    connectionId,
    dbSessionId,
    databaseType,
    database: database ?? schemaState.currentDatabase,
    schema: schema ?? schemaState.currentSchema,
    connectionContext: {
      connectionId,
      dbSessionId,
      databaseType,
      name: connectionName,
      serverVersion,
    },
    schemaContext: {
      tables: schemaState.tables,
      views: schemaState.views,
      columns: schemaState.columnMap,
    },
  });
}

export function readCurrentQueryPanelRetryValidationInput(
  panelId: string,
  paramValues: Record<string, string>,
): RetryValidationInput | null {
  const panelStoreState = usePanelStore.getState();
  const panel = panelStoreState.panels.find(
    (candidate): candidate is QueryPanelState =>
      candidate.id === panelId && candidate.type === 'query',
  );
  const execution = panel ? panelStoreState.queryExec.get(panelId) : undefined;
  if (!panel || !execution) return null;

  const activeConnections = useActiveConnectionStore.getState().connections;
  const hasMappedActiveConnection = Object.prototype.hasOwnProperty.call(
    activeConnections,
    panel.connectionId,
  );
  const activeConnection = hasMappedActiveConnection
    ? activeConnections[panel.connectionId]
    : undefined;
  const dbSessionId = panel.dbSessionId;
  const panelHasSession = typeof dbSessionId === 'string' && dbSessionId.trim().length > 0;
  const activeConnectionHasSession =
    typeof activeConnection?.dbSessionId === 'string' &&
    activeConnection.dbSessionId.trim().length > 0;
  const activeConnectionMatchesPanel =
    activeConnection !== undefined &&
    activeConnection.connectionId === panel.connectionId &&
    activeConnection.status === 'connected' &&
    panelHasSession &&
    activeConnectionHasSession &&
    activeConnection.dbSessionId === dbSessionId;
  if (!activeConnectionMatchesPanel) return null;

  const schemaStoreState = useSchemaStore.getState();
  const schemaState = schemaStoreState.schemas.get(dbSessionId) ?? schemaStoreState;
  const latestContext = buildQueryPanelDiagnosisContext({
    execution,
    connectionId: panel.connectionId,
    dbSessionId,
    databaseType: panel.databaseType,
    connectionName: panel.connectionName,
    database:
      panel.database ??
      schemaState.currentDatabase ??
      activeConnection?.currentDatabase ??
      undefined,
    schema: panel.schema,
    serverVersion: activeConnection?.serverInfo?.serverVersion,
    schemaState,
  });
  if (!latestContext.ok) return null;

  const params = parseSqlParams(execution.sql);
  const boundParams = params.length > 0 ? paramsToPayload(params, paramValues) : {};

  return {
    sql: execution.sql,
    contextFingerprint: latestContext.context.contextFingerprint,
    boundParams,
  };
}
