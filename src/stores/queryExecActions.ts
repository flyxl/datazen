import { queryCommands } from '../commands/query';
import { applyQueryStreamEvent } from '../lib/queryStream';
import { resolvePostQueryViewMode } from '../lib/chart/postQueryView';
import { t } from '../locales/t';
import type { QueryStreamEvent, StatementResult } from '../types';
import type { ChartConfig } from '../types/chart';

export type BindParams = Record<string, string | number | boolean | null>;

export interface QueryExecState {
  sql: string;
  results: StatementResult[];
  activeResultIdx: number;
  error: string | null;
  running: boolean;
  executionTimeMs: number | null;
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
  resultDetailRowIndex: null,
});

export function emptyQueryExecState(): QueryExecState {
  return { ...EMPTY_QUERY_EXEC };
}

function extractError(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return t('query.executeFailed');
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
  connectionId: string,
  sql: string,
  getExec: () => Map<string, QueryExecState>,
  setExec: (exec: Map<string, QueryExecState>) => void,
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
    }),
  );

  const onEvent = (event: QueryStreamEvent) => {
    const exec = getExec().get(panelId);
    if (!exec || exec.streamRunId !== runId) return;
    const updated = applyQueryStreamEvent(exec, event);
    setExec(patchExec(getExec(), panelId, updated));
  };

  try {
    await queryCommands.executeQueryStream(connectionId, sql, onEvent);
    const exec = getExec().get(panelId);
    if (exec && exec.streamRunId === runId) {
      const viewMode = resolvePostQueryViewMode(exec.results[0]);
      setExec(patchExec(getExec(), panelId, { resultViewMode: viewMode, running: false }));
    }
  } catch (e) {
    const exec = getExec().get(panelId);
    if (exec && exec.streamRunId === runId) {
      setExec(patchExec(getExec(), panelId, { running: false, error: extractError(e) }));
    }
  }
}

export async function runBoundQuery(
  panelId: string,
  connectionId: string,
  sql: string,
  params: BindParams,
  getExec: () => Map<string, QueryExecState>,
  setExec: (exec: Map<string, QueryExecState>) => void,
): Promise<void> {
  setExec(patchExec(getExec(), panelId, { running: true, error: null }));

  try {
    const multi = await queryCommands.executeQuery(connectionId, sql, params);
    const viewMode = resolvePostQueryViewMode(multi.results[0]);
    setExec(
      patchExec(getExec(), panelId, {
        running: false,
        results: multi.results,
        activeResultIdx: 0,
        error: null,
        executionTimeMs: multi.totalTimeMs ?? null,
        resultViewMode: viewMode,
      }),
    );
  } catch (e) {
    setExec(
      patchExec(getExec(), panelId, {
        running: false,
        error: extractError(e),
        results: [],
        activeResultIdx: 0,
      }),
    );
  }
}
