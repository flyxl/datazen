/**
 * The candidate values of the enumeration columns (DB-05).
 *
 * The cache is **per panel and per column**, and it is filled only when the
 * user asks for it: opening an editor never queries anything. The panel
 * scope matters because the candidates belong to a table, and two tabs on
 * two tables must not see each other's values.
 */
import type { TableState } from './types';

export interface EnumCandidateStoreSlice {
  /** Remember the candidates of one column. */
  setEnumCandidates: (panelId: string, column: string, values: readonly string[]) => void;
  /** Forget the candidates of the panel (page change, table change). */
  clearEnumCandidates: (panelId: string) => void;
}

export function createEnumCandidateSlice(
  patch: (panelId: string, updater: (ts: TableState) => Partial<TableState>) => void,
): EnumCandidateStoreSlice {
  return {
    setEnumCandidates: (panelId, column, values) => {
      patch(panelId, (ts) => {
        const enumCandidates = new Map(ts.enumCandidates);
        enumCandidates.set(column, [...values]);
        return { enumCandidates };
      });
    },

    clearEnumCandidates: (panelId) => {
      patch(panelId, (ts) =>
        ts.enumCandidates.size === 0 ? {} : { enumCandidates: new Map() },
      );
    },
  };
}
