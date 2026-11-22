//! The two smallest navigator menus: a category node and a generic object
//! (the "column" case of the family list — React's `handleObjectContextMenu`).
//!
//! The category builder's `import` item is absent because the hook passes no
//! `onImport`; only `refresh` and a writable-tables `new-table` remain. The
//! object menu is the single `copy-name` entry the generic node gets.

use super::{
    act, at, item, MenuEntry, NavActionKind, NavTarget, ViewActionCaps,
};

/// React `buildSchemaTreeContextMenuItems({ kind: 'category' })` plus the
/// hook's `onRefresh` / `onNewTable` availability.
#[allow(dead_code)] // VM mirror; supplied by the navigator host.
#[derive(Clone, Debug, Default)]
pub(crate) struct CategoryMenuInput {
    pub cat_id: String,
    pub read_only: bool,
    pub view: ViewActionCaps,
}

#[allow(dead_code)] // menu builder; remove when the navigator host wires it.
pub(crate) fn build_category_menu(input: &CategoryMenuInput) -> Vec<MenuEntry> {
    let mut items = vec![item(
        "refresh",
        "connWin.refresh",
        act(NavActionKind::RefreshCategory),
    )];
    if !input.read_only && input.cat_id == "tables" && input.view.create_table {
        items.push(item(
            "new-table",
            "common.newTable",
            at(
                NavActionKind::CreateTable,
                NavTarget {
                    cat_id: input.cat_id.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    }
    items
}

/// React `handleObjectContextMenu` — the generic object/column node's single
/// `copy-name` entry.
#[allow(dead_code)] // menu builder; remove when the navigator host wires it.
pub(crate) fn build_object_menu(name: &str) -> Vec<MenuEntry> {
    vec![item(
        "copy-name",
        "common.copyName",
        at(
            NavActionKind::CopyName,
            NavTarget {
                name: name.to_owned(),
                ..NavTarget::default()
            },
        ),
    )]
}
