import type { FilterCondition } from '../../types';
import type { TableChangeContext } from '../../lib/tableChanges';
import { cloneFilters } from './filterUtils';
import { emptyTableState } from './connectionState';
import type { TableState } from './types';

/**
 * The first-frame seed of a panel (DB-07).
 *
 * A foreign-key jump opens a tab that has to show the referenced row **right
 * away** — with the condition visible in the filter bar, not after a second
 * round trip.
 *
 * The obvious way to do that (`setFilters`) cannot work here, and the reason
 * is worth writing down:
 *
 * - `setFilters` patches the panel and reloads it immediately;
 * - `patchPanel` is a **no-op for a panel that has no slice yet** — and a
 *   freshly created tab has no slice;
 * - so "create the tab, then set the filters" silently does nothing, the tab
 *   loads the whole table, and the user sees a full table flash before the
 *   filtered one arrives.
 *
 * `seedPanel` therefore **creates** the slice with the conditions already in
 * it and does **not** trigger a load: the panel's own first load (the one
 * `TableView` starts anyway) picks the conditions up and the table is fetched
 * exactly once — with the filter.
 */
export interface PanelSeed {
  /** The conditions the first fetch has to carry. */
  filters: FilterCondition[];
  /** How the conditions combine; `and` is the default. */
  filterLogic?: 'and' | 'or';
  /** Whether the filter panel starts open (the conditions stay visible). */
  openFilterPanel?: boolean;
}

export interface PanelSeedSlice {
  /**
   * Write the first-frame conditions of a panel.
   *
   * Creates the panel's slice when it has none. It never bumps
   * `requestRevision` and never reloads: the panel's own first load is the
   * one fetch, and it carries the conditions.
   */
  seedPanel: (panelId: string, context: TableChangeContext, seed: PanelSeed) => void;
}

/** The slice factory: the store only wires it, the logic lives here. */
export function createPanelSeedSlice(
  write: (panelId: string, state: TableState) => void,
): PanelSeedSlice {
  return {
    seedPanel: (panelId, context, seed) => {
      const filters = cloneFilters(seed.filters);
      const logic = seed.filterLogic ?? 'and';
      write(panelId, {
        ...emptyTableState(context),
        filters,
        draftFilters: cloneFilters(filters),
        filterLogic: logic,
        draftFilterLogic: logic,
        filterPanelOpen: seed.openFilterPanel ?? true,
      });
    },
  };
}
