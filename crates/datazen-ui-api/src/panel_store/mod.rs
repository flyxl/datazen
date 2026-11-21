mod actions;
mod focus;
mod host;
mod machine;
mod result_edit;
mod types;

#[cfg(test)]
mod tests;

pub use focus::{
    is_pane_key_of_panel, matches_database, matches_relation, pane_args, pane_id_of_pane_key,
    pane_key, pane_keys_of_panel, panel_id_of_pane_key, resolve_focused_pane_id,
    resolve_next_active, sync_pane_focus, FocusPatch, DEFAULT_PANE_ID, PANE_SEPARATOR,
};
pub use host::{CommitCellRequest, PanelStoreHost, RunQueryRequest};
pub use machine::PanelStore;
pub use result_edit::{validate_batch, BatchTarget};
pub use types::{
    BindParams, ExecTable, PanelExecState, PanelPatch, PanelState, PendingHistoryQuery,
    ResultColumn, ResultEdit, ResultView, StorePanel, StorePanelType,
};

pub const PANEL_STORE_SURFACE: [&str; 10] = [
    "addPanel",
    "removePanel",
    "setFocusedPane",
    "openPane",
    "closePane",
    "executeQuery",
    "cancelQuery",
    "updateResultCells",
    "loadHistory",
    "PANEL_STORE_GAP",
];

pub const PANEL_STORE_GAP: [&str; 8] = [
    "zustand create/set/get and async Promise actions",
    "queryCommands getQueryHistoryPage/getFavoriteQueries/refreshFavorites/getFavoritesRoot/addFavoriteQuery/deleteFavoriteQuery",
    "useBackendClient().commitQueryResultCell and toTableValue",
    "getQueryPanelSessionRegistry peek/remove and tableSessionKey",
    "createRunningCloseGuard and createPanelCloseNotifier event wiring",
    "runBoundQuery / runStreamingQuery stream callbacks",
    "the t() locale strings and the module-level resultWriteInflight set",
    "React component consumption of focusedPaneId and the pane focus mirror",
];
