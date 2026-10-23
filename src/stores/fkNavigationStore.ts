import { create } from 'zustand';
import { FK_NAV_MAX_DEPTH, type GridViewport } from '../lib/fkNavigation';

/**
 * The foreign-key navigation stack (DB-07).
 *
 * Following a foreign key opens a **new tab**, and the tab bar is not a
 * history: the user cannot tell which of the four open tables came from where.
 * This store is that missing history — a back stack and a forward stack of the
 * jumps, plus the viewport snapshot each tab left behind.
 *
 * Three rules run through the store:
 *
 * - **The stack is memory only.** A restart has no history, which is the
 *   product decision (07 §3.5), not an omission.
 * - **A jump is recorded once.** The cycle rule (A → B → A) reuses the tab that
 *   is already open instead of stacking a third one, so the stack can never
 *   grow without bound; `FK_NAV_MAX_DEPTH` is only the second line of defence.
 * - **A dead tab leaves the stack.** The stack is pruned against the tabs that
 *   still exist, so "back" never activates a panel the user closed.
 *
 * It is a store of its own rather than a slice of `panelStore`: that file is
 * already at its size limit, and a navigation stack that depends on the panel
 * store would be a cycle (the panel store would have to know the stack).
 */

/** One recorded jump: the tab it opened and where it came from. */
export interface FkNavEntry {
  /** Entry id; unique per session, never persisted. */
  id: string;
  /** The tab this jump opened. */
  panelId: string;
  /** `fkNavKey(target, values)` — the cycle key and the de-duplication key. */
  navKey: string;
  /** The minimum the breadcrumb and the restore need. */
  origin: {
    /** The tab the jump started from (the "back" target). */
    sourcePanelId: string;
    /** The table the jump started from. */
    sourceTable: string;
    /** The local columns of the foreign key, in pairing order. */
    localColumns: string[];
    /** The table the jump landed on. */
    targetTable: string;
    /** The referenced columns, in pairing order. */
    targetColumns: string[];
  };
}

/** The stack itself. The top of the stack is the last element of the array. */
export interface FkNavigationState {
  /** The back stack; the top is the last element. */
  back: FkNavEntry[];
  /** The forward stack; produced by `goBack`, consumed by `goForward`. */
  forward: FkNavEntry[];
  /** True when entries were dropped because the stack exceeded its depth. */
  truncated: boolean;
  /**
   * The viewport each tab left behind, keyed by the tab's id.
   *
   * The snapshot belongs to the tab, **not** to a stack entry: coming back
   * restores the tab the user jumped *from*, and that tab has no entry of its
   * own (an entry points at the tab the jump *opened*). The map is therefore
   * the only place a viewport can live — and it is written when a tab is left,
   * read once when it is entered again.
   */
  viewports: Map<string, GridViewport>;
}

export interface FkNavigationActions {
  /** Record a new jump; clears the forward stack. */
  push: (entry: Omit<FkNavEntry, 'id'>) => FkNavEntry;
  /**
   * Step back one jump.
   *
   * Entries whose tab no longer exists are skipped and dropped. Returns the
   * entry the caller has to act on, or `null` when there is nothing to go back
   * to.
   *
   * **The tab to activate is `entry.origin.sourcePanelId`** — the tab the jump
   * started from. (`entry.panelId` is the tab the jump *opened*, which is what
   * `goForward` activates.)
   */
  goBack: (existingPanelIds: ReadonlySet<string>) => FkNavEntry | null;
  /**
   * Step forward one jump; the inverse of `goBack`.
   *
   * **The tab to activate is `entry.panelId`** — the tab the jump opened.
   */
  goForward: (existingPanelIds: ReadonlySet<string>) => FkNavEntry | null;
  /** Cut the back stack down to one entry (the cycle rule); clears `forward`. */
  rewindTo: (entryId: string) => void;
  /** Record the viewport a tab left behind; only the latest one is kept. */
  recordViewport: (panelId: string, viewport: GridViewport) => void;
  /** Take the viewport snapshot of a tab; taking it consumes it. */
  takeViewport: (panelId: string) => GridViewport | null;
  /** The entry id of a jump with this key, or `null`. */
  findEntryIdByNavKey: (navKey: string) => string | null;
  /** Drop every entry whose tab is gone. */
  prune: (existingPanelIds: ReadonlySet<string>) => void;
  /** Forget everything (used when the session ends). */
  reset: () => void;
}

let entryCounter = 0;

function nextEntryId(): string {
  entryCounter += 1;
  return `fk-nav-${entryCounter}`;
}

/** Reset the id counter. Test-only: ids must be predictable across runs. */
export function resetFkNavigationEntryIds(): void {
  entryCounter = 0;
}

export const useFkNavigationStore = create<FkNavigationState & FkNavigationActions>(
  (set, get) => ({
    back: [],
    forward: [],
    truncated: false,
    viewports: new Map(),

    push: (entry) => {
      const created: FkNavEntry = { ...entry, id: nextEntryId() };
      const back = [...get().back, created];
      // The depth limit drops the **oldest** entry; the breadcrumb says so
      // instead of pretending the history is complete.
      if (back.length > FK_NAV_MAX_DEPTH) {
        back.shift();
        set({ back, forward: [], truncated: true });
        return created;
      }
      set({ back, forward: [] });
      return created;
    },

    goBack: (existingPanelIds) => {
      const { back, forward } = get();
      // Skip (and drop) the entries whose tab the user closed meanwhile.
      let index = back.length - 1;
      while (index >= 0 && !existingPanelIds.has(back[index].panelId)) index -= 1;
      if (index < 0) {
        if (back.length > 0) set({ back: [], forward });
        return null;
      }
      const entry = back[index];
      // `slice(0, index)` drops the dead entries above the one we return to.
      set({
        back: back.slice(0, index),
        // The entry that was just left goes **on top** of the forward stack:
        // the top is the last element, so "forward" continues where "back"
        // stopped (the browser's own rule).
        forward: [...forward, entry],
      });
      return entry;
    },

    goForward: (existingPanelIds) => {
      const { back, forward } = get();
      let index = forward.length - 1;
      while (index >= 0 && !existingPanelIds.has(forward[index].panelId)) index -= 1;
      if (index < 0) {
        if (forward.length > 0) set({ back, forward: [] });
        return null;
      }
      const entry = forward[index];
      const nextBack = [...back, entry];
      set({
        back: nextBack.length > FK_NAV_MAX_DEPTH ? nextBack.slice(nextBack.length - FK_NAV_MAX_DEPTH) : nextBack,
        forward: forward.slice(0, index),
      });
      return entry;
    },

    rewindTo: (entryId) => {
      const { back } = get();
      const index = back.findIndex((entry) => entry.id === entryId);
      if (index < 0) return;
      // Everything after the entry is cut away: the cycle A → B → A keeps the
      // tab of A open but forgets the detour through B.
      set({ back: back.slice(0, index + 1), forward: [] });
    },

    recordViewport: (panelId, viewport) => {
      const viewports = new Map(get().viewports);
      // Only the latest snapshot of a tab is kept: two jumps out of the same
      // tab must not restore two different scroll positions.
      viewports.set(panelId, viewport);
      set({ viewports });
    },

    takeViewport: (panelId) => {
      const viewports = get().viewports;
      const viewport = viewports.get(panelId);
      if (!viewport) return null;
      // Taking the snapshot consumes it: a tab that is entered twice does not
      // restore the same scroll position twice.
      const next = new Map(viewports);
      next.delete(panelId);
      set({ viewports: next });
      return viewport;
    },

    findEntryIdByNavKey: (navKey) => {
      const { back } = get();
      for (let index = back.length - 1; index >= 0; index -= 1) {
        if (back[index].navKey === navKey) return back[index].id;
      }
      return null;
    },

    prune: (existingPanelIds) => {
      const { back, forward } = get();
      const keep = (entries: FkNavEntry[]): FkNavEntry[] =>
        entries.filter((entry) => existingPanelIds.has(entry.panelId));
      const viewports = new Map(get().viewports);
      for (const panelId of [...viewports.keys()]) {
        if (!existingPanelIds.has(panelId)) viewports.delete(panelId);
      }
      set({ back: keep(back), forward: keep(forward), viewports });
    },

    reset: () => set({ back: [], forward: [], truncated: false, viewports: new Map() }),
  }),
);
