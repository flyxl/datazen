//! The one writer of pane focus, ported from React
//! `packages/frontend/src/stores/paneFocus.ts` (T274).
//!
//! The React doc comment, verbatim in intent: a tab may be split into
//! several panes, and each pane has its own editor. Which pane has the
//! focus is therefore **two** facts — the map from panel to focused
//! pane, and a mirror of the focused pane of the tab that is on
//! screen. They must never disagree, so they are written together, by
//! one rule, in one place; the rule has nothing to do with the rest
//! of the panel store and therefore lives in its own module.
//!
//! ## Rust shape
//!
//! * `Record<string, string>` mirrors as `HashMap` (the focus map's
//!   order is never observable — the patch reads and writes by key).
//! * The `Panel[]` argument collapses to the ids the function itself
//!   reads (`panels.map((p) => p.id)`): the `Panel` type mirror is
//!   T364's (`stores/panelTypes.ts`, pending), and this module needs
//!   nothing else of it, so it takes the id slice.
//! * The `focus` object mirrors as a `(panel_id, pane_id)` pair;
//!   `paneId: string | null` is `Option<&str>` with `null` and the
//!   empty string both meaning "this tab's default pane" (the entry
//!   is removed rather than stored, so an unsplit tab is simply
//!   absent from the map — the React doc's own rule).

use std::collections::{HashMap, HashSet};

/// `PaneFocusPatch`: the two focus fields an action has to write,
/// plus the new activation. The shape is the one the panel store's
/// state has, spelled out here so this module does not have to know
/// the store's private state type.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaneFocusPatch {
    pub focused_pane_id_by_panel: HashMap<String, String>,
    pub focused_pane_id: Option<String>,
    pub active_panel_id: Option<String>,
}

/// `syncPaneFocus`: the one writer of pane focus.
///
/// It drops entries whose tab is gone (a pane cannot outlive its
/// panel), applies `paneId` to `panelId` when given, and re-derives
/// the `focusedPaneId` mirror from whichever tab ends up active.
/// Routing the mirror through the map — rather than letting each
/// action assign it — is what keeps a tab from inheriting another
/// tab's pane id, and what keeps closing a pane in one tab from
/// stealing the focus of another.
///
/// `pane_id` of `None` (or the empty string, JS's null/undefined/null
/// default-pane readings) means "this tab's default pane": the entry
/// is removed rather than stored.
pub fn sync_pane_focus(
    prev: &HashMap<String, String>,
    panel_ids: &[&str],
    active_panel_id: Option<&str>,
    focus: Option<(&str, Option<&str>)>,
) -> PaneFocusPatch {
    let live: HashSet<&str> = panel_ids.iter().copied().collect();
    let focus_panel = focus.map(|(panel_id, _)| panel_id);
    let mut next: HashMap<String, String> = HashMap::new();
    for (panel_id, pane_id) in prev {
        // React: `if (live.has(panelId) && panelId !== focus?.panelId)`.
        // The focus panel's previous entry drops even when the new
        // pane is null — the removal below is that same write.
        if live.contains(panel_id.as_str()) && Some(panel_id.as_str()) != focus_panel {
            next.insert(panel_id.clone(), pane_id.clone());
        }
    }
    // React: `if (focus?.paneId) next[focus.panelId] = focus.paneId;`
    // — truthiness, so only a non-null, non-empty pane id stores.
    if let Some((panel_id, pane_id)) = focus {
        if let Some(pane_id) = pane_id.filter(|pane| !pane.is_empty()) {
            next.insert(panel_id.to_owned(), pane_id.to_owned());
        }
    }
    // React: `activePanelId ? (next[activePanelId] ?? null) : null` —
    // truthiness on the active id (an empty string is falsy); the
    // map lookup keeps whatever it holds (`??` only falls back on a
    // missing key, never on an empty value).
    let active = active_panel_id.filter(|id| !id.is_empty());
    let focused_pane_id = match active {
        Some(active) => next.get(active).map(|pane| pane.to_owned()),
        None => None,
    };
    PaneFocusPatch {
        focused_pane_id_by_panel: next,
        focused_pane_id,
        active_panel_id: active.map(|id| id.to_owned()),
    }
}
