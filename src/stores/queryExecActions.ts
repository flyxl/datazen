import { queryCommands } from '../commands/query';
import { emitCrossWindow } from '../lib/crossWindowBus';
import { applyQueryStreamEvent } from '../lib/queryStream';
import { resolvePostQueryViewMode } from '../lib/chart/postQueryView';
import { sqlContainsSchemaChangingDdl } from '../lib/schemaChangingSql';
import { t } from '../locales/t';
import type { QueryStreamEvent, StatementResult } from '../types';
import type { ChartConfig } from '../types/chart';
import { isCancellationError } from '../lib/queryExecutionViewModel';

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
): Promise<void> {
  const runId = ++streamRunCounter;
  setExec(
    patchExec(getExec(), panelId, {
      running: true,
      error: null,
      results: [],
      activeResultIdx: 0,
      streamRunId: runId,
      executionTimeMs: null,
      cancelState: 'idle',
      cancelError: null,
      terminalState: null,
    }),
  );

  const onEvent = (event: QueryStreamEvent) => {
    const exec = getExec().get(panelId);
    if (!exec || exec.streamRunId !== runId) return;
    const updated = applyQueryStreamEvent(exec, event);
    setExec(patchExec(getExec(), panelId, updated));
  };

  try {
    await queryCommands.executeQueryStream(dbSessionId, sql, onEvent, {
      database: database ?? null,
      schema: schema ?? null,
    });
    const exec = getExec().get(panelId);
    if (exec && exec.streamRunId === runId) {
      const viewMode = resolvePostQueryViewMode(exec.results[0]);
      setExec(
        patchExec(getExec(), panelId, {
          resultViewMode: viewMode,
          running: false,
          cancelState: 'idle',
          cancelError: null,
          terminalState: 'succeeded',
        }),
      );
      if (!exec.error) {
        await notifySchemaChangedIfNeeded(dbSessionId, sql);
      }
    }
  } catch (e) {
    const exec = getExec().get(panelId);
    if (exec && exec.streamRunId === runId) {
      const message = extractError(e);
      const cancelled = exec.cancelState === 'requested' && isCancellationError(message);
      setExec(
        patchExec(getExec(), panelId, {
          running: false,
          error: cancelled ? null : message,
          cancelState: 'idle',
          cancelError: null,
          terminalState: cancelled ? 'cancelled' : 'failed',
        }),
      );
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
  setExec(
    patchExec(getExec(), panelId, {
      running: true,
      error: null,
      cancelState: 'idle',
      cancelError: null,
      terminalState: null,
    }),
  );

  try {
    const multi = await queryCommands.executeQuery(
      dbSessionId,
      sql,
      params,
      database ?? null,
      schema ?? null,
    );
    const viewMode = resolvePostQueryViewMode(multi.results[0]);
    setExec(
      patchExec(getExec(), panelId, {
        running: false,
        results: multi.results,
        activeResultIdx: 0,
        error: null,
        executionTimeMs: multi.totalTimeMs ?? null,
        resultViewMode: viewMode,
        cancelState: 'idle',
        cancelError: null,
        terminalState: 'succeeded',
      }),
    );
    await notifySchemaChangedIfNeeded(dbSessionId, sql);
  } catch (e) {
    const exec = getExec().get(panelId);
    const message = extractError(e);
    const cancelled = exec?.cancelState === 'requested' && isCancellationError(message);
    setExec(
      patchExec(getExec(), panelId, {
        running: false,
        error: cancelled ? null : message,
        results: [],
        activeResultIdx: 0,
        cancelState: 'idle',
        cancelError: null,
        terminalState: cancelled ? 'cancelled' : 'failed',
      }),
    );
  }
}
