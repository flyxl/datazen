//! The one writer of pane focus.
//!
//! A tab may be split into several panes, and each pane has its own editor.
//! Which pane has the focus is therefore **two** facts: the map from panel to
//! focused pane, and a mirror of the focused pane of the tab that is on screen.
//! They must never disagree, so they are written together, by one rule, in one
//! place — a rule that has nothing to do with the rest of the panel store and
//! therefore lives in its own module.

import type { Panel } from './panelTypes';

/**
 * The two focus fields an action has to write, plus the new activation.
 *
 * The shape is the one the panel store's state has; it is spelled out here so
 * that this module does not have to know the store's private state type.
 */
export type PaneFocusPatch = {
  focusedPaneIdByPanel: Record<string, string>;
  focusedPaneId: string | null;
  activePanelId: string | null;
};

/**
 * The one writer of pane focus.
 *
 * It drops entries whose tab is gone (a pane cannot outlive its panel), applies
 * `paneId` to `panelId` when given, and re-derives the `focusedPaneId` mirror
 * from whichever tab ends up active. Routing the mirror through the map — rather
 * than letting each action assign it — is what keeps a tab from inheriting
 * another tab's pane id, and what keeps closing a pane in one tab from stealing
 * the focus of another.
 *
 * `paneId` of `null` / `undefined` means "this tab's default pane": the entry is
 * removed rather than stored, so an unsplit tab is simply absent from the map.
 */
export function syncPaneFocus(
  prev: { focusedPaneIdByPanel: Record<string, string> },
  panels: Panel[],
  activePanelId: string | null,
  focus?: { panelId: string; paneId: string | null },
): PaneFocusPatch {
  const live = new Set(panels.map((p) => p.id));
  const next: Record<string, string> = {};
  for (const [panelId, paneId] of Object.entries(prev.focusedPaneIdByPanel)) {
    if (live.has(panelId) && panelId !== focus?.panelId) next[panelId] = paneId;
  }
  if (focus?.paneId) next[focus.panelId] = focus.paneId;
  return {
    focusedPaneIdByPanel: next,
    focusedPaneId: activePanelId ? (next[activePanelId] ?? null) : null,
    activePanelId,
  };
}
