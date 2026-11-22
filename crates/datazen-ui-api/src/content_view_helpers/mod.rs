mod context;
mod icons;
mod labels;
mod types;

#[cfg(test)]
mod tests;

pub use context::{resolve_connection_context, resolve_connection_context_by_connection};
pub use icons::get_panel_icon;
pub use labels::{
    get_panel_label, get_panel_short_label, get_panel_tab_label, get_sub_tabs, get_view_sub_tabs,
    resolve_panel_tab_database,
};
pub use types::{
    ActiveConnectionEntry, ContentViewConnectionContext, ContentViewPanel, PanelIcon, PanelKind,
    SubTab, SubTabId, CONTENT_VIEW_HELPERS_GAP, CONTENT_VIEW_HELPERS_SURFACE, ICON_CLASS,
};
