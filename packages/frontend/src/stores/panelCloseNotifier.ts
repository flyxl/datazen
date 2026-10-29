/**
 * Tab-closed notification, keyed by a panel's id.
 *
 * ## Why this exists
 *
 * A tab's state inside a driver has to outlive the *view* (the host renders
 * only the active tab, so switching tabs unmounts it) but must not outlive the
 * *tab*. Only the host can tell those apart, so it has to say so — and the only
 * reliable signal is the tab disappearing, not the view unmounting.
 *
 * ## Why the store is passed in
 *
 * A tab can disappear through nine different actions (`removePanel`,
 * `removeAllForConnection`, `removePanelsForRelation`, `removePanelsForDatabase`,
 * `closeOtherPanels`, `closeAllPanels`, `closePanelsToTheRight`,
 * `closePanelsToTheLeft`, ...). Firing from each would be a ninth place to
 * remember, and the first action added later would silently leak. Every one of
 * them still goes through a single `set(...)` on `usePanelStore`, so one
 * subscriber that diffs the id set catches all of them — including paths that do
 * not exist yet.
 *
 * ## Why ids are a safe signal
 *
 * `nextPanelId` is a monotonic counter and the store is memory-only (no
 * persist), so an id is never reused within a session. "Present before, absent
 * after" therefore means exactly one thing: that tab was closed.
 *
 * The store is injected rather than imported so this module stays independent
 * of the store module (no cycle) and so a test can drive it with a fake.
 */
type PanelLike = { id: string };
type PanelStateLike = { panels: PanelLike[] };
type PanelStoreLike = {
  /** Zustand semantics: the listener receives both the next and the previous state. */
  subscribe: (listener: (state: PanelStateLike, prev: PanelStateLike) => void) => () => void;
};

type PanelCloseHandler = () => void;

export interface PanelCloseNotifier {
  /**
   * Register a close handler for a tab; returns an unsubscribe.
   *
   * Registering again for the same panel is allowed and keeps both handlers.
   * Registering for an already-closed panel is allowed too: the handler is
   * simply never fired, which is the right outcome for a view with nothing
   * left to clean up.
   */
  onPanelClosed: (panelId: string, handler: PanelCloseHandler) => () => void;
}

export function createPanelCloseNotifier(store: PanelStoreLike): PanelCloseNotifier {
  const handlers = new Map<string, Set<PanelCloseHandler>>();

  function onPanelClosed(panelId: string, handler: PanelCloseHandler): () => void {
    let set = handlers.get(panelId);
    if (!set) {
      set = new Set();
      handlers.set(panelId, set);
    }
    set.add(handler);
    return () => {
      set.delete(handler);
      if (set.size === 0) handlers.delete(panelId);
    };
  }

  function fire(panelIds: readonly string[]): void {
    for (const panelId of panelIds) {
      const set = handlers.get(panelId);
      if (!set) continue;
      // Drop the entry before running: a handler that throws must not leave a
      // registration that fires again later.
      handlers.delete(panelId);
      for (const handler of [...set]) {
        try {
          handler();
        } catch (err) {
          // One driver's cleanup failing must never break closing a tab.
          console.error('[panelCloseNotifier] handler threw', err);
        }
      }
    }
  }

  store.subscribe((state, prev) => {
    if (state.panels === prev.panels) return;
    const after = new Set<string>(state.panels.map((p) => p.id));
    const closed: string[] = [];
    for (const panel of prev.panels) {
      if (!after.has(panel.id)) closed.push(panel.id);
    }
    if (closed.length > 0) fire(closed);
  });

  return { onPanelClosed };
}
