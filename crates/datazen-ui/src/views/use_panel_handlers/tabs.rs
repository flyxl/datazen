//! Tab-level decisions for `usePanelHandlers`: close operations, the tab
//! context menu, `setSubTab` and the refresh routing.

use super::model::{
    ConnectionContextVm, PanelEffect, PanelKind, PanelPatch, PanelVm, SubTab, TabMenuItem,
};

/// React `handleClosePanel`: remove one id.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn close_panel(panel_id: &str) -> PanelEffect {
    PanelEffect::Close { panel_ids: vec![panel_id.to_owned()] }
}

/// React `closeOtherPanels`: keep exactly `keep_id`.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn close_other_panels(panels: &[PanelVm], keep_id: &str) -> PanelEffect {
    PanelEffect::Close {
        panel_ids: panels.iter().filter(|panel| panel.id != keep_id).map(|panel| panel.id.clone()).collect(),
    }
}

/// React `closeAllPanels`: remove every id.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn close_all_panels(panels: &[PanelVm]) -> PanelEffect {
    PanelEffect::Close { panel_ids: panels.iter().map(|panel| panel.id.clone()).collect() }
}

/// React `closePanelsToTheRight` / `closePanelsToTheLeft`: strip by position in
/// the store's panel order. A missing target id is a no-op.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn close_panels_side(panels: &[PanelVm], panel_id: &str, to_right: bool) -> PanelEffect {
    let Some(index) = panels.iter().position(|panel| panel.id == panel_id) else {
        return PanelEffect::None;
    };
    let ids: Vec<String> = if to_right {
        panels[index + 1..].iter()
    } else {
        panels[..index].iter()
    }
    .map(|panel| panel.id.clone())
    .collect();
    PanelEffect::Close { panel_ids: ids }
}

/// React `handlePanelTabContextMenu` flags: `onlyOneTab`, `hasTabsToRight`,
/// `hasTabsToLeft`. The menu shows every entry but disables/hides those that do
/// not apply; here we return only the applicable items in React's order.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn tab_context_menu_items(panels: &[PanelVm], panel_id: &str) -> Vec<TabMenuItem> {
    let idx = panels.iter().position(|panel| panel.id == panel_id);
    let mut items = vec![TabMenuItem::Close, TabMenuItem::CloseOthers, TabMenuItem::CloseAll];
    if idx.is_some_and(|index| index < panels.len().saturating_sub(1)) {
        items.push(TabMenuItem::CloseToRight);
    }
    if idx.is_some_and(|index| index > 0) {
        items.push(TabMenuItem::CloseToLeft);
    }
    items
}

/// React `handleSetSubTab`: only table/view panels change. A table's
/// `structureEditing` is kept when moving to structure and cleared otherwise;
/// a view only changes `subTab`.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn set_sub_tab(panel: Option<&PanelVm>, sub_tab: SubTab) -> PanelEffect {
    let Some(panel) = panel else {
        return PanelEffect::None;
    };
    match panel.kind {
        Some(PanelKind::Table) => PanelEffect::Patch {
            panel_id: panel.id.clone(),
            patch: PanelPatch {
                sub_tab: Some(sub_tab),
                structure_editing: Some(sub_tab == SubTab::Structure && panel.structure_editing),
                ..PanelPatch::default()
            },
        },
        Some(PanelKind::View) => PanelEffect::Patch {
            panel_id: panel.id.clone(),
            patch: PanelPatch { sub_tab: Some(sub_tab), ..PanelPatch::default() },
        },
        _ => PanelEffect::None,
    }
}

/// React `handleRefresh`: without a session nothing happens; with a current
/// database it reloads just that database's tables, otherwise the whole
/// connection.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn refresh(
    ctx: Option<&ConnectionContextVm>,
    current_database: Option<&str>,
) -> PanelEffect {
    let Some(ctx) = ctx else {
        return PanelEffect::None;
    };
    if ctx.db_session_id.is_empty() {
        return PanelEffect::None;
    }
    PanelEffect::Refresh {
        db_session_id: ctx.db_session_id.clone(),
        database: current_database.filter(|db| !db.is_empty()).map(str::to_owned),
        database_type: ctx.database_type.clone(),
    }
}
