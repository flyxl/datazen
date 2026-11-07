/**
 * Active right-panel sub-tab, keyed by the host-issued `panelId`.
 *
 * ## Why this is not `useState`
 *
 * The host renders only the active panel and remounts the incoming one with
 * `key={activePanel.id}` (`PanelContentRenderer`). Everything below the top-level
 * tab bar - `RedisConnectionView -> RedisWorkbench -> RedisRightPanel` - is
 * therefore destroyed whenever the user switches tabs, taking component state
 * with it. A `useState<RightTab>('detail')` reset the user back to the item
 * sub-tab every time they came back, which read as "my console vanished".
 *
 * Same trade-off as `consoleTranscript.ts`, and for the same reason: module
 * scope outlives the unmount.
 *
 * ## Why keyed by `panelId` and not `dbSessionId`
 *
 * `dbSessionId` is shared by every db tab of one connection, so keying by it
 * leaked one tab's sub-tab into a sibling: db0 on the console also opened a
 * fresh db1 tab on the console. `panelId` is unique per tab, so state survives
 * a remount of *the same* tab and never crosses to another one. Cleared by
 * `panelLifecycle` when the host closes the tab.
 */
import { useSyncExternalStore } from 'react';

export type RightTab = 'detail' | 'console' | 'pubsub' | 'slowlog';

const DEFAULT_TAB: RightTab = 'detail';

const tabs = new Map<string, RightTab>();
const listeners = new Map<string, Set<() => void>>();

/**
 * Bound subscribe, cached per panel.
 *
 * `useSyncExternalStore` compares `subscribe` by identity and re-subscribes
 * whenever it changes. A freshly built closure on every render would therefore
 * unsubscribe/resubscribe on each commit - churn during render, and lost
 * notifications if a write lands in the gap. One stable function is kept for as
 * long as the panel exists.
 */
const subscribes = new Map<string, (listener: () => void) => () => void>();

function stableSubscribe(panelId: string): (listener: () => void) => () => void {
  let sub = subscribes.get(panelId);
  if (!sub) {
    sub = (listener) => {
      let set = listeners.get(panelId);
      if (!set) {
        set = new Set();
        listeners.set(panelId, set);
      }
      set.add(listener);
      return () => {
        set.delete(listener);
        if (set.size === 0) listeners.delete(panelId);
      };
    };
    subscribes.set(panelId, sub);
  }
  return sub;
}

/** Read a panel's tab without subscribing. */
export function readRightTab(panelId: string): RightTab {
  return tabs.get(panelId) ?? DEFAULT_TAB;
}

/** Persist a tab choice. A no-op write is skipped so `useSyncExternalStore` stays quiet. */
export function writeRightTab(panelId: string, tab: RightTab): void {
  if (readRightTab(panelId) === tab) return;
  tabs.set(panelId, tab);
  for (const listener of listeners.get(panelId) ?? []) listener();
}

export function useRightTab(panelId: string): RightTab {
  return useSyncExternalStore(
    stableSubscribe(panelId),
    () => readRightTab(panelId),
    () => DEFAULT_TAB,
  );
}

/** Forget a panel so its tab state is released (also used on tab close). */
export function resetRightTab(panelId: string): void {
  tabs.delete(panelId);
  subscribes.delete(panelId);
  listeners.delete(panelId);
}
