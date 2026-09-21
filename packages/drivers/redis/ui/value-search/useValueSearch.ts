import { useCallback, useEffect, useRef, useState } from 'react';
import {
  invokeScanAbort,
  invokeScanValues,
  redisCommandInvoke,
  type RedisInvokeFn,
  type ValueMatchHit,
} from '../shared/redisInvoke';

export type ValueSearchStatus = 'idle' | 'running' | 'done' | 'cancelled';

export interface ValueSearchState {
  status: ValueSearchStatus;
  hits: ValueMatchHit[];
  scanned: number;
  maxKeys: number;
  limitHit: boolean;
  error: string | null;
}

export interface ValueSearchParams {
  mode: 'key' | 'value' | 'all';
  query: string;
  pattern: string;
  maxKeys?: number;
  count?: number;
}

const DEFAULT_MAX_KEYS = 50_000;
const MAX_HITS = 5_000;

const IDLE: ValueSearchState = {
  status: 'idle',
  hits: [],
  scanned: 0,
  maxKeys: DEFAULT_MAX_KEYS,
  limitHit: false,
  error: null,
};

function mergeHits(prev: ValueMatchHit[], next: ValueMatchHit[]): ValueMatchHit[] {
  if (next.length === 0) return prev;
  const seen = new Set(prev.map((h) => `${h.matchedIn}:${h.key}`));
  const out = prev.slice();
  for (const hit of next) {
    const id = `${hit.matchedIn}:${hit.key}`;
    if (seen.has(id)) continue;
    seen.add(id);
    out.push(hit);
    if (out.length >= MAX_HITS) break;
  }
  return out;
}

/**
 * Drives one guarded `scan_values` task as a cancellable, incremental poll loop.
 *
 * State machine (AGENTS three-element rule):
 *   enter  — `start()` from the workbench (mode != key + non-empty query)
 *   within — batches scheduled via setTimeout(0) until done/limit/cancel
 *   exit   — cursor done, guard limit hit, `cancel()` (fires scan_abort),
 *            or `start()`/unmount superseding the current run via a seq guard
 */
export function useValueSearch({
  dbSessionId,
  dbIndex,
  invoke = redisCommandInvoke,
}: {
  dbSessionId: string;
  dbIndex: number;
  invoke?: RedisInvokeFn;
}) {
  const [state, setState] = useState<ValueSearchState>(IDLE);
  const seqRef = useRef(0);
  const cursorRef = useRef(0);
  const taskIdRef = useRef<string | undefined>(undefined);
  const runningRef = useRef(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const paramsRef = useRef<ValueSearchParams>({ mode: 'value', query: '', pattern: '*' });

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const runBatch = useCallback(
    async (seq: number) => {
      if (seq !== seqRef.current) return;
      const { mode, query, pattern, maxKeys, count } = paramsRef.current;
      try {
        const batch = await invokeScanValues(
          dbSessionId,
          dbIndex,
          {
            pattern,
            query,
            mode,
            cursor: cursorRef.current,
            taskId: taskIdRef.current,
            maxKeys,
            count,
          },
          invoke,
        );
        if (seq !== seqRef.current) return; // superseded by a newer run

        taskIdRef.current = batch.taskId;
        cursorRef.current = batch.cursor;
        const finished = batch.done || batch.cancelled || batch.limitHit;

        setState((prev) => ({
          status: finished ? (batch.cancelled ? 'cancelled' : 'done') : 'running',
          hits: mergeHits(prev.hits, batch.matched),
          scanned: batch.scannedKeys,
          maxKeys: maxKeys ?? prev.maxKeys,
          limitHit: batch.limitHit,
          error: null,
        }));

        if (!finished) {
          timerRef.current = setTimeout(() => void runBatch(seq), 0);
        } else {
          runningRef.current = false;
        }
      } catch (err) {
        if (seq !== seqRef.current) return;
        runningRef.current = false;
        setState((prev) => ({
          ...prev,
          status: 'done',
          error: err instanceof Error ? err.message : String(err),
        }));
      }
    },
    [dbSessionId, dbIndex, invoke],
  );

  const start = useCallback(
    (params: ValueSearchParams) => {
      paramsRef.current = params;
      clearTimer();
      seqRef.current += 1;
      const seq = seqRef.current;
      cursorRef.current = 0;
      taskIdRef.current = undefined;
      runningRef.current = true;
      setState({
        status: 'running',
        hits: [],
        scanned: 0,
        maxKeys: params.maxKeys ?? DEFAULT_MAX_KEYS,
        limitHit: false,
        error: null,
      });
      void runBatch(seq);
    },
    [clearTimer, runBatch],
  );

  const cancel = useCallback(() => {
    if (!runningRef.current) return;
    seqRef.current += 1;
    clearTimer();
    runningRef.current = false;
    setState((prev) => ({ ...prev, status: 'cancelled' }));
    if (taskIdRef.current) {
      void invokeScanAbort(dbSessionId, taskIdRef.current, invoke).catch(() => {
        /* best-effort abort */
      });
    }
  }, [clearTimer, dbSessionId, invoke]);

  const reset = useCallback(() => {
    seqRef.current += 1;
    clearTimer();
    runningRef.current = false;
    cursorRef.current = 0;
    taskIdRef.current = undefined;
    setState(IDLE);
  }, [clearTimer]);

  useEffect(() => {
    return () => {
      seqRef.current += 1;
      if (timerRef.current !== null) clearTimeout(timerRef.current);
    };
  }, []);

  return { state, start, cancel, reset };
}
