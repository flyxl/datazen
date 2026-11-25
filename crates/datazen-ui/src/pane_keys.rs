//! Pane identity for the query-execution state map, ported from React
//! `packages/frontend/src/stores/paneKeys.ts` (T274).
//!
//! The React doc comment, verbatim in intent: `queryExec` started life
//! as a 1:1 `Map<panelId, QueryExecState>`, which is what blocks Split
//! Pane — a panel can only ever own one editor. Panes therefore get
//! their own key space. Two properties keep this wave
//! behavior-neutral:
//!
//! 1. A panel's **default pane is keyed by the bare panel id**. Every
//!    caller that still passes a panel id — and every existing fixture
//!    that seeds `queryExec` by panel id — therefore resolves to
//!    exactly the same entry it did before, so single-pane behavior
//!    is bit-for-bit unchanged.
//! 2. Secondary panes use a `panelId::paneId` composite, so a pane key
//!    can always be decomposed back into the panel that owns it. That
//!    is what `cancelAndCleanupExec` uses to drop *every* pane of a
//!    closed tab.
//!
//! ## Rust shape
//!
//! The exec map itself is not modelled here (its owners —
//! `queryExecActions` (T375), `panelExecCleanup` (T312) — hold their
//! own mirrors); this module only reads keys, so
//! [`pane_keys_of_panel`] takes the key list in the map's order (a JS
//! `Map` iterates insertion-first; the Rust caller's own map decides
//! the order it hands over). The optional-with-default parameters
//! (`paneId: string = DEFAULT_PANE_ID`, `paneId: string | undefined`)
//! mirror as `Option<&str>`, `None` being the default pane.

/// Id of the pane a query panel owns before any split happens.
pub const DEFAULT_PANE_ID: &str = "main";

/// Separator between a panel id and a secondary pane id. Panel ids
/// are built by `nextPanelId` as `panel-<prefix>-<n>`, so they can
/// never contain it.
const PANE_SEPARATOR: &str = "::";

/// `paneKey(panelId, paneId = DEFAULT_PANE_ID)`: the `queryExec` key
/// of `paneId` inside `panelId`; the default pane (`None` or
/// `"main"`) keeps the bare panel id.
pub fn pane_key(panel_id: &str, pane_id: Option<&str>) -> String {
    match pane_id {
        None | Some(DEFAULT_PANE_ID) => panel_id.to_owned(),
        Some(pane_id) => format!("{panel_id}{PANE_SEPARATOR}{pane_id}"),
    }
}

/// `panelIdOfPaneKey(key)`: the panel that owns `key` — a bare panel
/// id names itself.
pub fn panel_id_of_pane_key(key: &str) -> &str {
    match key.find(PANE_SEPARATOR) {
        None => key,
        Some(at) => &key[..at],
    }
}

/// `paneIdOfPaneKey(key)`: the pane `key` names inside its panel; a
/// bare panel id is the default pane.
pub fn pane_id_of_pane_key(key: &str) -> &str {
    match key.find(PANE_SEPARATOR) {
        None => DEFAULT_PANE_ID,
        Some(at) => &key[at + PANE_SEPARATOR.len()..],
    }
}

/// `isPaneKeyOfPanel(key, panelId)`: whether `key` is an exec-map
/// entry owned by `panelId` — any of its panes.
pub fn is_pane_key_of_panel(key: &str, panel_id: &str) -> bool {
    panel_id_of_pane_key(key) == panel_id
}

/// `paneKeysOfPanel(current, panelId)`: every `queryExec` key
/// belonging to `panelId`, in the order the caller's map iterates
/// (a JS `Map`'s insertion order — the Rust mirror's owner decides,
/// and hands the keys over in it).
pub fn pane_keys_of_panel<'a>(
    keys: impl IntoIterator<Item = &'a str>,
    panel_id: &str,
) -> Vec<String> {
    keys.into_iter()
        .filter(|key| is_pane_key_of_panel(key, panel_id))
        .map(|key| key.to_owned())
        .collect()
}

/// `paneArgs(paneId)`: the trailing `paneId` argument list for a
/// store action. A split tab appends the pane id; a tab that has not
/// been split appends *nothing at all*, so a single-pane call site
/// keeps the exact argument list it had before panes existed (the
/// React doc's own point: passing an explicit `undefined` would still
/// widen the call, and the pre-split contract is what callers and
/// their tests pin). `None`, `undefined`, and the default pane all
/// append nothing.
pub fn pane_args<'a>(pane_id: Option<&'a str>) -> Vec<&'a str> {
    match pane_id {
        Some(pane_id) if pane_id != DEFAULT_PANE_ID => vec![pane_id],
        _ => Vec::new(),
    }
}

/// `resolveFocusedPaneId(focusedPaneId)`: the pane that editor actions
/// route to. `null`/`undefined` means "no split has happened", i.e.
/// the panel's default pane — that single rule is what makes the
/// single-pane path indistinguishable from the pre-pane behavior.
/// (Nullish, not truthy: an empty string stays an empty string.)
pub fn resolve_focused_pane_id(focused_pane_id: Option<&str>) -> &str {
    focused_pane_id.unwrap_or(DEFAULT_PANE_ID)
}
