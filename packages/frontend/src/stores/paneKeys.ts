/**
 * Pane identity for the query-execution state map.
 *
 * `queryExec` started life as a 1:1 `Map<panelId, QueryExecState>`, which is
 * what blocks Split Pane: a panel can only ever own one editor. Panes therefore
 * get their own key space. Two properties keep this wave behavior-neutral:
 *
 * 1. A panel's **default pane is keyed by the bare panel id**. Every caller
 *    that still passes a panel id — and every existing fixture that seeds
 *    `queryExec` by panel id — therefore resolves to exactly the same entry it
 *    did before, so single-pane behavior is bit-for-bit unchanged.
 * 2. Secondary panes use a `panelId::paneId` composite, so a pane key can
 *    always be decomposed back into the panel that owns it. That is what
 *    `cancelAndCleanupExec` uses to drop *every* pane of a closed tab.
 */

/** Id of the pane a query panel owns before any split happens. */
export const DEFAULT_PANE_ID = 'main';

/**
 * Separator between a panel id and a secondary pane id. Panel ids are built by
 * `nextPanelId` as `panel-<prefix>-<n>`, so they can never contain it.
 */
const PANE_SEPARATOR = '::';

/** `queryExec` key of `paneId` inside `panelId`; the default pane keeps the bare panel id. */
export function paneKey(panelId: string, paneId: string = DEFAULT_PANE_ID): string {
  return paneId === DEFAULT_PANE_ID ? panelId : `${panelId}${PANE_SEPARATOR}${paneId}`;
}

/** Panel that owns `key`. */
export function panelIdOfPaneKey(key: string): string {
  const at = key.indexOf(PANE_SEPARATOR);
  return at < 0 ? key : key.slice(0, at);
}

/** Pane `key` names inside its panel; a bare panel id is the default pane. */
export function paneIdOfPaneKey(key: string): string {
  const at = key.indexOf(PANE_SEPARATOR);
  return at < 0 ? DEFAULT_PANE_ID : key.slice(at + PANE_SEPARATOR.length);
}

/** Whether `key` is an exec-map entry owned by `panelId` — any of its panes. */
export function isPaneKeyOfPanel(key: string, panelId: string): boolean {
  return panelIdOfPaneKey(key) === panelId;
}

/** Every `queryExec` key belonging to `panelId`, in map order. */
export function paneKeysOfPanel(current: Map<string, unknown>, panelId: string): string[] {
  const keys: string[] = [];
  for (const key of current.keys()) {
    if (isPaneKeyOfPanel(key, panelId)) keys.push(key);
  }
  return keys;
}

/**
 * Trailing `paneId` argument list for a store action.
 *
 * A split tab appends the pane id; a tab that has not been split appends
 * *nothing at all*, so a single-pane call site keeps the exact argument list it
 * had before panes existed. That matters beyond tidiness: passing an explicit
 * `undefined` would still widen the call to `f(a, b, undefined)`, and the
 * pre-split call contract is what callers and their tests pin.
 */
export function paneArgs(paneId: string | undefined): [paneId?: string] {
  return paneId == null || paneId === DEFAULT_PANE_ID ? [] : [paneId];
}

/**
 * Pane that editor actions route to.
 *
 * `null` means "no split has happened", i.e. the panel's default pane — that
 * single rule is what makes the single-pane path indistinguishable from the
 * pre-pane behavior. Focus lives in the store, not in component state, because
 * `ContentView` unmounts every inactive tab's panel and component state would
 * not survive a tab switch.
 */
export function resolveFocusedPaneId(focusedPaneId: string | null | undefined): string {
  return focusedPaneId ?? DEFAULT_PANE_ID;
}
