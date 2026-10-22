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

import { create } from 'zustand';

import { queryCommands } from '../commands/query';
import { t } from '../locales/t';
import { gridReadOnlyMessageKey, type ServerEditability } from '../lib/resultEditability';
import { paneKey } from './paneKeys';

export { gridReadOnlyMessageKey } from '../lib/resultEditability';

/**
 * The key a verdict is stored under.
 *
 * The pane is part of the key because one panel may own several editors once
 * it is split, and each of them has its own result.
 */
export function resultEditKey(
  panelId: string,
  paneId: string | undefined,
  resultIdx: number,
): string {
  return `${paneKey(panelId, paneId)}:${resultIdx}`;
}

/** What the back end needs to answer the verdict for one result. */
export interface ResultOriginRequest {
  connectionId: string;
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
  /** Fetch the verdict for one result. */
  resolveForResult: (key: string, request: ResultOriginRequest) => Promise<void>;
  /** Forget one result's verdict — the result was replaced or closed. */
  clearForResult: (key: string) => void;
  /** Forget every verdict. */
  clearAll: () => void;
}

export const usePanelResultEdit = create<PanelResultEditState>((set) => ({
  verdicts: {},
  resolving: {},
  errors: {},

  resolveForResult: async (key, request) => {
    set((state) => ({
      resolving: { ...state.resolving, [key]: true },
      errors: { ...state.errors, [key]: null },
    }));
    try {
      const verdict = await queryCommands.resolveResultOrigin({
        connectionId: request.connectionId,
        dbSessionId: request.dbSessionId,
        database: request.database,
        schema: request.schema,
        sql: request.sql,
        resultColumns: request.resultColumns,
      });
      set((state) => ({ verdicts: { ...state.verdicts, [key]: verdict } }));
    } catch (error) {
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
      set((state) => ({ resolving: { ...state.resolving, [key]: false } }));
    }
  },

  clearForResult: (key) =>
    set((state) => {
      const verdicts = { ...state.verdicts };
      const resolving = { ...state.resolving };
      const errors = { ...state.errors };
      delete verdicts[key];
      delete resolving[key];
      delete errors[key];
      return { verdicts, resolving, errors };
    }),

  clearAll: () => set({ verdicts: {}, resolving: {}, errors: {} }),
}));

/**
 * Refuse a write to a result the driver has already refused.
 *
 * The interface is not the security boundary: a caller that skipped it must
 * not be able to write through a read-only result either. A result nobody has
 * judged yet is not refused here — the judgement arrives on its own, and the
 * interface stays read-only until it does.
 */
export function assertResultWritable(
  panelId: string,
  paneId: string | undefined,
  resultIdx: number,
): void {
  const verdict = usePanelResultEdit.getState().verdicts[resultEditKey(panelId, paneId, resultIdx)];
  if (verdict && !verdict.writable) {
    throw new Error(t(gridReadOnlyMessageKey(verdict.reason ?? 'unknownOrigin')));
  }
}
