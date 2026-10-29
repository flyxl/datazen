import { useCallback, useRef, useState } from 'react';
import { queryCommands } from '../commands/query';
import type { QueryStreamEvent, Value } from '../types';
import {
  buildColumnStatsSql,
  isAveragableType,
  parseColumnStatsRow,
  type ColumnStats,
  type ColumnStatsTable,
} from '../lib/columnStats';

/**
 * Column statistics lifecycle (DB-12): the header menu asks for one column,
 * the hook runs the single-statement aggregate **on the connection's query
 * path** and reports the six values the dialog renders.
 *
 * It streams with `recordHistory: false` — the SQL is machine-generated, and
 * a user's query history must stay theirs — and `applyResultLimit: false`:
 * an aggregate returns exactly one row, and result-limit rewriting must not
 * touch a statement whose shape is known.
 */
export interface ColumnStatsRequest extends ColumnStatsTable {
  dbSessionId: string;
  /** The logical database the table lives in (routing, never a session `USE`). */
  database: string;
}

export type ColumnStatsStatus = 'idle' | 'loading' | 'ready' | 'error';

export interface ColumnStatsState {
  status: ColumnStatsStatus;
  /** The column the numbers belong to; `null` while idle. */
  column: string | null;
  stats: ColumnStats | null;
  error: string | null;
}

type StreamQueryFn = (
  dbSessionId: string,
  sql: string,
  onEvent: (event: QueryStreamEvent) => void,
  options?: {
    applyResultLimit?: boolean;
    recordHistory?: boolean;
    database?: string | null;
    schema?: string | null;
  },
) => Promise<void>;

export interface UseColumnStatsOptions {
  /** Test seam: the streamer defaults to the real query command. */
  streamQuery?: StreamQueryFn;
}

const IDLE: ColumnStatsState = { status: 'idle', column: null, stats: null, error: null };

export function useColumnStats(options: UseColumnStatsOptions = {}) {
  const streamQuery = options.streamQuery ?? queryCommands.executeQueryStream;
  const [state, setState] = useState<ColumnStatsState>(IDLE);
  /**
   * The revision of the latest `open`: a slow first column must not land
   * over a second one the user asked for while it was still flying.
   */
  const revisionRef = useRef(0);

  const open = useCallback(
    (request: ColumnStatsRequest, column: string, columnType: string | null | undefined) => {
      const includeAvg = isAveragableType(columnType);
      const sql = buildColumnStatsSql(request, column, { includeAvg });
      const revision = revisionRef.current + 1;
      revisionRef.current = revision;

      setState({ status: 'loading', column, stats: null, error: null });

      void streamQuery(
        request.dbSessionId,
        sql,
        (event) => {
          if (event.type !== 'rows') return;
          if (revisionRef.current !== revision) return;
          const firstRow: readonly (Value | null)[] | null = event.rows[0] ?? null;
          if (!firstRow) return;
          setState({
            status: 'ready',
            column,
            stats: parseColumnStatsRow(firstRow, { includeAvg }),
            error: null,
          });
        },
        {
          applyResultLimit: false,
          recordHistory: false,
          database: request.database,
          schema: request.schema,
        },
      )
        .then(() => {
          if (revisionRef.current !== revision) return;
          setState((prev) => {
            if (prev.status === 'ready' && prev.column === column) return prev;
            // The stream finished without a single row: the server's own
            // contract says an aggregate always answers one — anything else
            // is a failure the user should see, not zeros.
            return {
              status: 'error',
              column,
              stats: null,
              error: 'empty aggregate result',
            };
          });
        })
        .catch((error: unknown) => {
          if (revisionRef.current !== revision) return;
          setState({
            status: 'error',
            column,
            stats: null,
            error: error instanceof Error ? error.message : String(error),
          });
        });
    },
    [streamQuery],
  );

  const dismiss = useCallback(() => {
    // Retiring the request also drops any in-flight resolution: a late
    // answer after close reopens nothing.
    revisionRef.current += 1;
    setState(IDLE);
  }, []);

  return { state, open, dismiss };
}
