import { queryCommands } from '../commands/query';
import { emitCrossWindow } from '../lib/crossWindowBus';
import { applyQueryStreamEvent } from '../lib/queryStream';
import { resolvePostQueryViewMode } from '../lib/chart/postQueryView';
import { sqlContainsSchemaChangingDdl } from '../lib/schemaChangingSql';
import { t } from '../locales/t';
import type { QueryStreamEvent, StatementResult } from '../types';
import type { ChartConfig } from '../types/chart';
import {
  isCancellationError,
  reduceQueryExecutionState,
  type QueryExecutionTransition,
} from '../lib/queryExecutionViewModel';

export type BindParams = Record<string, string | number | boolean | null>;

export type QueryCancelState = 'idle' | 'requested' | 'failed';
export type QueryTerminalState = 'succeeded' | 'failed' | 'cancelled' | 'unknown';

export interface QueryExecState {
  sql: string;
  results: StatementResult[];
  activeResultIdx: number;
  error: string | null;
  running: boolean;
  executionTimeMs: number | null;
  cancelState: QueryCancelState;
  cancelError: string | null;
  terminalState: QueryTerminalState | null;
  chartConfig?: ChartConfig;
  resultViewMode?: 'table' | 'chart';
  executionId: string | null;
  streamRunId?: number;
  resultDetailRowIndex: number | null;
}

export const EMPTY_QUERY_EXEC: QueryExecState = Object.freeze({
  sql: '',
  results: [],
  activeResultIdx: 0,
  error: null,
  running: false,
  executionTimeMs: null,
  cancelState: 'idle',
  cancelError: null,
  terminalState: null,
  executionId: null,
  resultDetailRowIndex: null,
});

export function emptyQueryExecState(): QueryExecState {
  return { ...EMPTY_QUERY_EXEC };
}

function extractError(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return t('common.executionFailed');
}

async function notifySchemaChangedIfNeeded(dbSessionId: string, sql: string): Promise<void> {
  if (!sqlContainsSchemaChangingDdl(sql)) return;
  await emitCrossWindow('datazen:refresh-connection', { dbSessionId });
}

let streamRunCounter = 0;

export function patchExec(
  current: Map<string, QueryExecState>,
  panelId: string,
  patch: Partial<QueryExecState>,
): Map<string, QueryExecState> {
  const next = new Map(current);
  const prev = current.get(panelId) ?? emptyQueryExecState();
  next.set(panelId, { ...prev, ...patch });
  return next;
}

function transitionExec(
  current: Map<string, QueryExecState>,
  panelId: string,
  transition: QueryExecutionTransition,
): Map<string, QueryExecState> {
  const exec = current.get(panelId);
  if (!exec) return current;
  return patchExec(current, panelId, reduceQueryExecutionState(exec, transition));
}

function queryErrorTransition(
  exec: QueryExecState,
  message: string,
): QueryExecutionTransition {
  if (exec.cancelState === 'requested' && isCancellationError(message)) {
    return { type: 'cancelled' };
  }
  // A failed cancel request makes a later cancellation-looking stream error
  // ambiguous: the database outcome cannot be safely called Cancelled.
  if (exec.cancelState === 'failed' && isCancellationError(message)) {
    return { type: 'outcome_unknown', error: message };
  }
  return { type: 'failed', error: message };
}

export async function runStreamingQuery(
  panelId: string,
  dbSessionId: string,
  sql: string,
  getExec: () => Map<string, QueryExecState>,
  setExec: (exec: Map<string, QueryExecState>) => void,
  /** F1: panel's selected database — pinned on the backend before execution. */
  database?: string | null,
  /** F7: panel's PG-family schema target — drivers inline it when supported. */
  schema?: string | null,
  /** Bound values use this same execution-handle stream. */
  params?: BindParams,
): Promise<void> {
  const runId = ++streamRunCounter;
  setExec(
    transitionExec(
      patchExec(getExec(), panelId, {
        results: [],
        activeResultIdx: 0,
        streamRunId: runId,
        executionTimeMs: null,
        executionId: null,
      }),
      panelId,
      { type: 'start' },
    ),
  );

  const onEvent = (event: QueryStreamEvent) => {
    const exec = getExec().get(panelId);
    if (!exec || exec.streamRunId !== runId) return;
    const updated = applyQueryStreamEvent(exec, event);
    setExec(patchExec(getExec(), panelId, updated));
  };

  try {
    const streamOptions = {
      database: database ?? null,
      schema: schema ?? null,
      ...(params && Object.keys(params).length > 0 ? { params } : {}),
    };
    await queryCommands.executeQueryStream(dbSessionId, sql, onEvent, streamOptions);
    const exec = getExec().get(panelId);
    if (exec && exec.streamRunId === runId) {
      const viewMode = resolvePostQueryViewMode(exec.results[0]);
      const withViewMode = patchExec(getExec(), panelId, { resultViewMode: viewMode });
      setExec(transitionExec(withViewMode, panelId, { type: 'succeeded' }));
      if (!exec.error) {
        await notifySchemaChangedIfNeeded(dbSessionId, sql);
      }
    }
  } catch (e) {
    const exec = getExec().get(panelId);
    if (exec && exec.streamRunId === runId) {
      const message = extractError(e);
      setExec(transitionExec(getExec(), panelId, queryErrorTransition(exec, message)));
    }
  }
}

export async function runBoundQuery(
  panelId: string,
  dbSessionId: string,
  sql: string,
  params: BindParams,
  getExec: () => Map<string, QueryExecState>,
  setExec: (exec: Map<string, QueryExecState>) => void,
  /** F1: panel's selected database — pinned on the backend before execution. */
  database?: string | null,
  /** F7: panel's PG-family schema target — drivers inline it when supported. */
  schema?: string | null,
): Promise<void> {
  await runStreamingQuery(
    panelId,
    dbSessionId,
    sql,
    getExec,
    setExec,
    database,
    schema,
    params,
  );
}
