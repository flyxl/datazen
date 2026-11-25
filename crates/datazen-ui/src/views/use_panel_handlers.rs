//! Port of `windows/connection/usePanelHandlers.ts` — the panel handler hook —
//! as pure decision logic (T622).
//!
//! React's hook is the single place a connection view turns a user intent into
//! a panel-store transition: opening/reusing tabs, routing table SQL actions,
//! consuming pending intents, closing tabs and refreshing data. This port keeps
//! those *decisions* as pure functions over host-owned panel snapshots: each
//! handler answers with a [`PanelEffect`] the host applies. No store, IPC,
//! promise or timer lives here.
//!
//! The React tests pin the tricky rules, all kept here: query tabs bind their
//! database at creation (including the whole path hierarchy), ER diagrams keep
//! their own database but follow the active panel's schema, `DBR-03` keys table
//! tabs by schema too, DB-object panels use the full identity tuple, server
//! status/processes are unique per connection, and `openData` routes back to
//! the table selector.
//!
//! ## Consumer / removal stage
//!
//! No production host wires these decisions yet; the connection-view host will
//! feed the panels/context and apply the effects. Every reusable item carries a
//! per-item `#[allow(dead_code)]` with this stage.

mod model;
mod open;
mod tabs;

pub use model::{
    ConnectionContextVm, OpenTargetVm, PanelEffect, PanelKind, PanelPatch, PanelVm, SubTab,
    TabMenuItem,
};
pub use open::{
    edit_table_structure, new_query, open_by_kind, open_connection_panel, open_db_object,
    open_er_diagram, open_structure, pending_matches, resolve_open_target, select_table,
    table_action_route, NamespaceEnsure, PathPin, TableActionRoute,
};
pub use tabs::{
    close_all_panels, close_other_panels, close_panel, close_panels_side, refresh, set_sub_tab,
    tab_context_menu_items,
};
