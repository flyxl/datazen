import { queryCommands } from '../commands/query';
import { emitCrossWindow } from '../lib/crossWindowBus';
import { applyQueryStreamEvent } from '../lib/queryStream';
import { resolvePostQueryViewMode } from '../lib/chart/postQueryView';
import {
  sqlContainsSchemaChangingDdl,
  sqlMayMutateSchema,
  sqlContainsDataModifying,
} from '../lib/schemaChangingSql';
import { invalidateSchemaCache } from '../lib/schemaCache';
import { useTableDataStore } from './tableDataStore';
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
  if (sqlMayMutateSchema(sql)) {
    invalidateSchemaCache(dbSessionId);
  }
  if (sqlContainsDataModifying(sql)) {
    // The table-data grid caches rows per session; a write from the query tab
    // must drop that cache or a reopened table tab shows stale rows.
    useTableDataStore.getState().invalidateCachedData(dbSessionId);
  }
  if (!sqlContainsSchemaChangingDdl(sql)) return;
  await emitCrossWindow('datazen:refresh-connection', { dbSessionId });
}

let streamRunCounter = 0;

/**
 * `key` is a `queryExec` map key, i.e. a **pane** key (`paneKey(panelId, paneId)`
 * — for a panel's default pane it is just the panel id; see `./paneKeys`).
 */
export function patchExec(
  current: Map<string, QueryExecState>,
  key: string,
  patch: Partial<QueryExecState>,
): Map<string, QueryExecState> {
  const next = new Map(current);
  const prev = current.get(key) ?? emptyQueryExecState();
  next.set(key, { ...prev, ...patch });
  return next;
}

function transitionExec(
  current: Map<string, QueryExecState>,
  key: string,
  transition: QueryExecutionTransition,
): Map<string, QueryExecState> {
  const exec = current.get(key);
  if (!exec) return current;
  return patchExec(current, key, reduceQueryExecutionState(exec, transition));
}

function queryErrorTransition(exec: QueryExecState, message: string): QueryExecutionTransition {
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

/**
 * Stream a query into the pane identified by `paneKey`.
 *
 * `paneKey` is captured once, up front, and every asynchronous write below uses
 * that captured value. It must stay stable for the whole stream: re-resolving
 * the focused pane inside `onEvent` would let a focus change between two chunks
 * divert the rest of the result set into a different pane. Staleness is instead
 * detected by `streamRunId`, which a newer run of the *same* pane bumps.
 */
export async function runStreamingQuery(
  paneKey: string,
  dbSessionId: string,
  sql: string,
  getExec: () => Map<string, QueryExecState>,
  setExec: (exec: Map<string, QueryExecState>) => void,
  /** Panel's selected database — pinned on the backend before execution. */
  database?: string | null,
  /** Panel's PG-family schema target — drivers inline it when supported. */
  schema?: string | null,
  /** Bound values use this same execution-handle stream. */
  params?: BindParams,
): Promise<void> {
  const runId = ++streamRunCounter;
  const currentExec = getExec().get(paneKey);
  const pinnedResults = (currentExec?.results ?? []).filter((r) => r.pinned);
  const baseOffset = pinnedResults.length;

  setExec(
    transitionExec(
      patchExec(getExec(), paneKey, {
        results: pinnedResults,
        activeResultIdx: baseOffset,
        streamRunId: runId,
        executionTimeMs: null,
        executionId: null,
      }),
      paneKey,
      { type: 'start' },
    ),
  );

  const onEvent = (event: QueryStreamEvent) => {
    const exec = getExec().get(paneKey);
    if (!exec || exec.streamRunId !== runId) return;
    const updated = applyQueryStreamEvent(exec, event, baseOffset);
    setExec(patchExec(getExec(), paneKey, updated));
  };

  try {
    const streamOptions = {
      database: database ?? null,
      schema: schema ?? null,
      ...(params && Object.keys(params).length > 0 ? { params } : {}),
    };
    await queryCommands.executeQueryStream(dbSessionId, sql, onEvent, streamOptions);
    const exec = getExec().get(paneKey);
    if (exec && exec.streamRunId === runId) {
      const viewMode = resolvePostQueryViewMode(exec.results[0]);
      const withViewMode = patchExec(getExec(), paneKey, { resultViewMode: viewMode });
      setExec(transitionExec(withViewMode, paneKey, { type: 'succeeded' }));
      if (!exec.error) {
        await notifySchemaChangedIfNeeded(dbSessionId, sql);
      }
    }
  } catch (e) {
    const exec = getExec().get(paneKey);
    if (exec && exec.streamRunId === runId) {
      const message = extractError(e);
      setExec(transitionExec(getExec(), paneKey, queryErrorTransition(exec, message)));
    }
  }
}

export async function runBoundQuery(
  paneKey: string,
  dbSessionId: string,
  sql: string,
  params: BindParams,
  getExec: () => Map<string, QueryExecState>,
  setExec: (exec: Map<string, QueryExecState>) => void,
  /** Panel's selected database — pinned on the backend before execution. */
  database?: string | null,
  /** Panel's PG-family schema target — drivers inline it when supported. */
  schema?: string | null,
): Promise<void> {
  await runStreamingQuery(paneKey, dbSessionId, sql, getExec, setExec, database, schema, params);
}
