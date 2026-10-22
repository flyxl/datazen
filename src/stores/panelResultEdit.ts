/**
 * The verdict cache for the query result grid.
 *
 * The **table-level** verdict — "can this result be written back at all?" —
 * belongs to the back end, because only the driver knows where the rows came
 * from. This store keeps that verdict per result, together with the state of
 * the request that fetched it.
 *
 * The state machine is deliberately small:
 *
 * * `resolving` — the verdict is being fetched. The grid stays read-only
 *   meanwhile: a verdict that has not arrived yet is not a permission.
 * * `verdicts` — the verdict, once it arrived.
 * * `errors` — why the request failed. A failed request leaves the result
 *   **read-only**; it never turns into "writable".
 *
 * Nothing here decides anything about SQL. The rules live in the shared
 * layer; this store only remembers their answer.
 */

import { useEffect } from 'react';
import { create } from 'zustand';

import { queryCommands } from '../commands/query';
import { t } from '../locales/t';
import { gridReadOnlyMessageKey, type ServerEditability } from '../lib/resultEditability';

export { gridReadOnlyMessageKey } from '../lib/resultEditability';

/**
 * The key a verdict is stored under.
 *
 * The key names the **session and the result index**, because that is what
 * both surfaces of a result can compute: the row-detail drawer knows the
 * panel's session, the result grid knows the session the result was read
 * from. Two surfaces of the same result therefore share one verdict instead
 * of fetching it twice.
 */
export function resultEditKey(dbSessionId: string, resultIdx: number): string {
  return `${dbSessionId}#${resultIdx}`;
}

/** What the back end needs to answer the verdict for one result. */
export interface ResultOriginRequest {
  dbSessionId: string;
  database: string | null;
  schema: string | null;
  sql: string;
  resultColumns: string[];
}

export interface PanelResultEditState {
  /** The verdict per result. `null` means "not known (yet)". */
  verdicts: Record<string, ServerEditability | null>;
  /** Whether a verdict is being fetched, per result. */
  resolving: Record<string, boolean>;
  /** The failure of the last attempt, per result. */
  errors: Record<string, string | null>;
  /**
   * The generation of the newest request per result.
   *
   * A request that is answered after the result was replaced must not write
   * its verdict: the answer would be about rows that are no longer on
   * screen. The generation tells a late answer that it was superseded.
   */
  generations: Record<string, number>;
  /** Fetch the verdict for one result. */
  resolveForResult: (key: string, request: ResultOriginRequest) => Promise<void>;
  /** Forget one result's verdict — the result was replaced or closed. */
  clearForResult: (key: string) => void;
  /** Forget every verdict. */
  clearAll: () => void;
}

export const usePanelResultEdit = create<PanelResultEditState>((set, get) => ({
  verdicts: {},
  resolving: {},
  errors: {},
  generations: {},

  resolveForResult: async (key, request) => {
    // Two surfaces of the same result ask for the same verdict. The second
    // one waits for the first instead of sending a second request.
    if (get().resolving[key]) return;
    const generation = (get().generations[key] ?? 0) + 1;
    set((state) => ({
      generations: { ...state.generations, [key]: generation },
      resolving: { ...state.resolving, [key]: true },
      errors: { ...state.errors, [key]: null },
    }));
    try {
      const verdict = await queryCommands.resolveResultOrigin({
        dbSessionId: request.dbSessionId,
        database: request.database,
        schema: request.schema,
        sql: request.sql,
        resultColumns: request.resultColumns,
      });
      if (get().generations[key] !== generation) return;
      set((state) => ({ verdicts: { ...state.verdicts, [key]: verdict } }));
    } catch (error) {
      if (get().generations[key] !== generation) return;
      // A verdict that could not be fetched is not a permission: the result
      // stays read-only and the reason is "the origin is unknown".
      set((state) => ({
        verdicts: { ...state.verdicts, [key]: null },
        errors: {
          ...state.errors,
          [key]: error instanceof Error ? error.message : String(error),
        },
      }));
    } finally {
      if (get().generations[key] === generation) {
        set((state) => ({ resolving: { ...state.resolving, [key]: false } }));
      }
    }
  },

  clearForResult: (key) =>
    set((state) => {
      const verdicts = { ...state.verdicts };
      const errors = { ...state.errors };
      const resolving = { ...state.resolving };
      delete verdicts[key];
      delete errors[key];
      delete resolving[key];
      return {
        verdicts,
        errors,
        resolving,
        // A request that is still in flight is now stale: its answer would be
        // about rows that are no longer on screen.
        generations: { ...state.generations, [key]: (state.generations[key] ?? 0) + 1 },
      };
    }),

  clearAll: () =>
    set({ verdicts: {}, resolving: {}, errors: {}, generations: {} }),
}));

/**
 * The verdict of one result, without fetching it.
 *
 * The surface that *shows* the grid reads the verdict; the surface that owns
 * the request (the row-detail drawer) fetches it. Both use the same key, so
 * the answer is fetched once and read by both.
 */
export function useResultVerdict(key: string | null): {
  verdict: ServerEditability | null;
  resolving: boolean;
} {
  const verdict = usePanelResultEdit((state) => (key ? state.verdicts[key] ?? null : null));
  const resolving = usePanelResultEdit((state) => (key ? state.resolving[key] === true : false));
  return { verdict, resolving };
}

/**
 * The verdict of one result, and the state of the request that fetches it.
 *
 * The hook owns the request: it asks for the verdict when the result changes
 * and drops it again when the result is replaced — a verdict never outlives
 * the rows it was made for.
 */
export function useResultEditability(options: {
  /** The key the verdict is stored under; `null` when there is no result. */
  key: string | null;
  /** The session the result was read from. */
  dbSessionId: string;
  /** The database the result was read from. */
  database: string | null;
  /** The schema the result was read from. */
  schema: string | null;
  /** The statement the result came from. */
  sql: string;
  /** The column names the result reports. */
  resultColumns: readonly string[];
}): { verdict: ServerEditability | null; resolving: boolean } {
  const { key, dbSessionId, database, schema, sql, resultColumns } = options;
  const verdict = usePanelResultEdit((state) => (key ? state.verdicts[key] ?? null : null));
  const resolving = usePanelResultEdit((state) => (key ? state.resolving[key] === true : false));
  // The column list is a dependency of the request, and its identity changes
  // on every render — so the *content* is the dependency, not the array.
  const columnsKey = JSON.stringify(resultColumns);
  useEffect(() => {
    if (!key) return;
    void usePanelResultEdit.getState().resolveForResult(key, {
      dbSessionId,
      database,
      schema,
      sql,
      resultColumns: JSON.parse(columnsKey) as string[],
    });
    return () => {
      usePanelResultEdit.getState().clearForResult(key);
    };
  }, [key, dbSessionId, database, schema, sql, columnsKey]);
  return { verdict, resolving };
}

/**
 * Refuse a write to a result the driver has already refused.
 *
 * The interface is not the security boundary: a caller that skipped it must
 * not be able to write through a read-only result either. A result nobody has
 * judged yet is not refused here — the judgement arrives on its own, and the
 * interface stays read-only until it does.
 */
export function assertResultWritable(dbSessionId: string, resultIdx: number): void {
  const verdict = usePanelResultEdit.getState().verdicts[resultEditKey(dbSessionId, resultIdx)];
  if (verdict && !verdict.writable) {
    throw new Error(t(gridReadOnlyMessageKey(verdict.reason ?? 'unknownOrigin')));
  }
}
