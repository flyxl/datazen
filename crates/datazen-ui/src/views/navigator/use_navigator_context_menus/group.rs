//! The two main-window menus: a group header and the blank canvas.
//!
//! React `buildMainGroupContextMenuItems` / `buildMainBlankContextMenuItems`.
//! The ungrouped bucket (`groupName === ''`) gets only the two create items —
//! rename/delete a group that does not exist would be meaningless.

use super::{act, item, separator, MenuEntry, NavActionKind};

/// React `buildMainGroupContextMenuItems({ isUngrouped })`.
#[allow(dead_code)] // menu builder; remove when the navigator host wires it.
pub(crate) fn build_group_menu(is_ungrouped: bool) -> Vec<MenuEntry> {
    let mut items = vec![
        item("new-group", "common.newGroup", act(NavActionKind::NewGroup)),
        item(
            "new-connection",
            "common.newConnection",
            act(NavActionKind::NewConnection),
        ),
    ];
    if !is_ungrouped {
        items.push(separator());
        items.push(item(
            "rename-group",
            "main.ctx.renameGroup",
            act(NavActionKind::RenameGroup),
        ));
        items.push(item(
            "delete-group",
            "main.ctx.deleteGroup",
            act(NavActionKind::DeleteGroup),
        ));
    }
    items
}

/// React `buildMainBlankContextMenuItems` — the navigator canvas' own menu.
#[allow(dead_code)] // menu builder; remove when the navigator host wires it.
pub(crate) fn build_blank_menu() -> Vec<MenuEntry> {
    vec![
        item("new-group", "common.newGroup", act(NavActionKind::NewGroup)),
        item(
            "new-connection",
            "common.newConnection",
            act(NavActionKind::NewConnection),
        ),
    ]
}
