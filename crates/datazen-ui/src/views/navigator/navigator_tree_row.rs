//! Port of `packages/frontend/src/windows/connection/navigator/NavigatorTreeRow.tsx`.
//!
//! React switches over 13 `UnifiedRow` variants; this view does the same over
//! [`RowKind`]. Every click, context request and drag affordance is a
//! [`RowAction`] the host executes — no IPC, store or drag simulation. The ARIA
//! contract (role, level, disclosure) lives in [`aria`] so indent and announced
//! level can legitimately differ under a search, exactly as in React.
//!
//! `treeStatusDot.tsx` is ported separately (T475); the three-state pip needed
//! here is inlined in [`status_dot`] with that removal stage noted. Icons the
//! registry lacks (Eye/Braces/Database/AlertCircle) use the closest registered
//! glyph rather than inventing one.
//!
//! Leftover L1 (removal stage): mount this from the navigator tree host, feed
//! rows from its store slice, reuse T475's status dot, and drop the allows.

use rune::{icon, row, text, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::{
    ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT, ICON_FOLDER, ICON_FOLDER_OPEN, ICON_PLUS, ICON_TABLE, ICON_X,
};
use crate::ui::kit::kit;

mod actions;
mod aria;

pub(crate) use actions::{RowAction, TreeRowHostPort};
pub(crate) use aria::{
    aria_expanded, aria_level_of, indent_of, is_decoration, is_navigator_branch, searched,
    section_group_key, ContextMenuTarget, RowConnectionStatus, RowKind, RowVm, TREE_TOP_LEVEL,
    PINNED_GROUP_KEY, RECENT_GROUP_KEY,
};

fn marker(key: &str) -> El {
    row().key(key).a11y_hidden()
}

fn context<S: 'static>(el: El, host: &TreeRowHostPort<S>, target: ContextMenuTarget) -> El {
    if let Some(on_action) = host.on_action.clone() {
        el.on_secondary_press(move |s, _press, _tx| {
            on_action(s, RowAction::ContextMenu(target.clone()))
        })
    } else {
        el
    }
}

fn chevron(expanded: bool, k: &Kit) -> El {
    icon(
        if expanded { &ICON_CHEVRON_DOWN } else { &ICON_CHEVRON_RIGHT },
        12.0,
    )
    .color(k.c.text_muted)
}

fn tree_item(
    key: &str,
    row_vm: &RowVm,
    label: &str,
    fill: rune::Color,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut el = row()
        .key(key)
        .w_full()
        .h(24.0)
        .pl(indent_of(row_vm))
        .pr(8.0)
        .gap(6.0)
        .center_y()
        .fill(fill);
    if !is_decoration(row_vm) {
        el = el
            .role("treeitem")
            .label(label)
            .level(aria_level_of(row_vm) as u32)
            .focusable(true);
        if let Some(expanded) = aria_expanded(row_vm) {
            el = el.expanded(expanded);
        }
    }
    el
}

/// Inlined three-state connection pip (T475 will port `renderStatusDot` fully).
#[allow(dead_code)] // view helper; removed when T475 lands.
fn status_dot<S: 'static>(connection_id: &str, status: RowConnectionStatus, dark: bool) -> El {
    let k = kit(dark);
    let (ink, title) = match status {
        RowConnectionStatus::Connecting => (k.c.warning.fg, t("conn.connecting")),
        RowConnectionStatus::Connected => (k.c.success.fg, t("conn.connected")),
        RowConnectionStatus::Error => (k.c.danger.fg, t("conn.failed")),
        RowConnectionStatus::Idle => return row().key(format!("conn-status-{connection_id}")).a11y_hidden(),
    };
    row()
        .key(format!("conn-status-{connection_id}"))
        .role("img")
        .label(title.to_owned())
        .child(row().key(format!("conn-status-dot-{connection_id}")).w(8.0).h(8.0).radius(4.0).fill(ink))
}

#[allow(dead_code)] // view entry point; consumer: the navigator tree host (not wired).
pub(crate) fn navigator_tree_row<S: 'static>(
    vm: &RowVm,
    dark: bool,
    host: &TreeRowHostPort<S>,
) -> El {
    let k = kit(dark);
    let normal = k.c.surface_base;
    let hover_fill = k.c.surface_base;
    match &vm.kind {
        RowKind::Section { section, display_name, count, expanded } => {
            let key = format!("nav-tree-section-{section}");
            let mut el = tree_item(&key, vm, display_name, hover_fill, dark)
                .child(marker("data-section-header"))
                .child(chevron(*expanded, k))
                .child(icon(&ICON_FOLDER_OPEN, 14.0).color(k.c.warning.fg))
                .child(text(display_name.clone()).font(k.ty.text_xsmall).color(k.c.text_muted))
                .child(text(format!("({count})")).font(k.ty.text_xsmall).color(k.c.text_muted));
            if let Some(on_action) = host.on_action.clone() {
                let group = section_group_key(section).to_owned();
                el = el.on_press(move |s: &mut S| on_action(s, RowAction::ToggleGroup(group.clone())));
            }
            el = el.on_key({
                let group = section_group_key(section).to_owned();
                let on_action = host.on_action.clone();
                move |s: &mut S, ev: &rune::KeyEvent, _tx: &mut rune::Tx| {
                    if ev.key == rune::Key::Enter {
                        if let Some(on_action) = &on_action {
                            on_action(s, RowAction::ToggleGroup(group.clone()));
                        }
                        return true;
                    }
                    false
                }
            });
            el
        }
        RowKind::Group { group_name, display_name, count, expanded, drop_target } => {
            let key = format!("nav-tree-group-{group_name}");
            let fill = if *drop_target { k.alpha(k.c.text_accent, 0.2) } else { hover_fill };
            let mut el = tree_item(&key, vm, display_name, fill, dark)
                .child(marker("data-group-header"))
                .child(chevron(*expanded, k))
                .child(icon(&ICON_FOLDER_OPEN, 14.0).color(k.c.warning.fg))
                .child(text(display_name.clone()).font(k.ty.text_small).color(k.c.text_base))
                .child(text(format!("({count})")).font(k.ty.text_xsmall).color(k.c.text_muted));
            if let Some(on_action) = host.on_action.clone() {
                let group = group_name.clone();
                el = el.on_press(move |s: &mut S| on_action(s, RowAction::ToggleGroup(group.clone())));
            }
            context(el, host, ContextMenuTarget::Group(group_name.clone()))
        }
        RowKind::Connection {
            id, name, section_group, is_selected, status, expanded, match_reason, match_context,
            drop_before, drop_after, engine, ..
        } => {
            let key = format!("nav-conn-{id}");
            let fill = if *is_selected { k.alpha(k.c.text_accent, 0.1) } else { hover_fill };
            let mut el = tree_item(&key, vm, name, fill, dark)
                .child(marker("data-conn-item"))
                .child(if *drop_before { row().key("nav-conn-drop-before").w_full().h(2.0).fill(k.c.text_accent) } else { row().key("nav-conn-drop-none-before").a11y_hidden() });
            let mut toggle = row()
                .key(format!("nav-conn-toggle-{id}"))
                .w(16.0)
                .h(16.0)
                .center()
                .label(t(if *expanded { "erDiagram.collapse" } else { "erDiagram.expand" }))
                .focusable(true)
                .child(chevron(*expanded, k));
            if let Some(on_action) = host.on_action.clone() {
                let connection_id = id.clone();
                let section = section_group.clone().unwrap_or_default();
                let connected = *status == RowConnectionStatus::Connected;
                let (action_id, action_section, connected_flag) =
                    (connection_id.clone(), section.clone(), connected);
                toggle = toggle.on_press(move |s: &mut S| {
                    if connected_flag {
                        on_action(s, RowAction::ToggleConnection {
                            connection_id: action_id.clone(),
                            section_group: action_section.clone(),
                        });
                    } else {
                        on_action(s, RowAction::OpenConnection(action_id.clone()));
                    }
                });
            }
            el = el.child(toggle)
                .child(
                    row()
                        .key(format!("conn-engine-{id}"))
                        .w(18.0)
                        .h(18.0)
                        .radius(4.0)
                        .fill(k.c.surface_base)
                        .border(1.0, k.c.border_base)
                        .center()
                        .child(text(engine.clone()).font(k.ty.text_xsmall).color(k.c.text_base)),
                )
                .child(text(name.clone()).font(k.ty.text_small).color(k.c.text_base))
                .child(status_dot::<S>(id, *status, dark));
            if let Some(reason) = match_reason {
                el = el.child(row().key("nav-conn-match").label(reason.clone()).a11y_hidden());
                if let Some(context) = match_context {
                    el = el.child(row().key("nav-conn-match-context").label(context.clone()).a11y_hidden());
                }
            }
            el = el.child(if *drop_after {
                row().key("nav-conn-drop-after").w_full().h(2.0).fill(k.c.text_accent)
            } else {
                row().key("nav-conn-drop-none-after").a11y_hidden()
            });
            if let Some(on_action) = host.on_action.clone() {
                let id = id.clone();
                el = el.on_press(move |s: &mut S| on_action(s, RowAction::SelectConnection(id.clone())));
            }
            context(el, host, ContextMenuTarget::Connection(id.clone()))
        }
        RowKind::Db { connection_id, db_session_id, db_name, expanded, loading, is_open } => {
            let key = format!("nav-tree-db-{db_name}");
            let mut el = tree_item(&key, vm, db_name, hover_fill, dark)
                .child(marker("schema-tree-node"))
                .child(chevron(*expanded, k))
                .child(icon(&ICON_FOLDER, 14.0).color(k.c.success.fg))
                .child(text(db_name.clone()).font(k.ty.text_small).color(k.c.text_muted));
            if *loading {
                el = el.child(row().key("nav-tree-loading-dot").w(8.0).h(8.0).radius(4.0).fill(k.c.text_muted));
            } else if *is_open {
                el = el.child(
                    row()
                        .key("data-db-open")
                        .w(6.0)
                        .h(6.0)
                        .radius(3.0)
                        .fill(k.c.success.fg)
                        .label(t("schemaTree.databaseOpenAria").to_owned())
                        .role("img"),
                );
            }
            if let Some(on_action) = host.on_action.clone() {
                let (c, s, d) = (connection_id.clone(), db_session_id.clone(), db_name.clone());
                el = el.on_press(move |st: &mut S| on_action(st, RowAction::ToggleDatabase {
                    connection_id: c.clone(), db_session_id: s.clone(), db_name: d.clone(),
                }));
            }
            context(el, host, ContextMenuTarget::Database { db_name: db_name.clone(), connection_id: connection_id.clone() })
        }
        RowKind::Schema { schema_name, db_name, connection_id, .. } => {
            let key = format!("nav-tree-schema-{schema_name}");
            let label = if schema_name.is_empty() { t("common.default") } else { schema_name };
            let mut el = tree_item(&key, vm, label, hover_fill, dark)
                .child(marker("schema-tree-node"))
                .child(chevron(aria_expanded(vm).unwrap_or(false), k))
                .child(icon(&ICON_FOLDER, 14.0).color(k.c.success.fg))
                .child(text(label.to_owned()).font(k.ty.text_small).color(k.c.text_muted));
            if let Some(on_action) = host.on_action.clone() {
                let key = format!("nav-tree-schema-{schema_name}");
                el = el.on_press(move |s: &mut S| on_action(s, RowAction::ToggleSchema(key.clone())));
            }
            context(el, host, ContextMenuTarget::Schema { schema_name: schema_name.clone(), db_name: db_name.clone(), connection_id: connection_id.clone() })
        }
        RowKind::Category { key, cat_id, label_key, .. } => {
            let row_key = format!("nav-tree-category-{cat_id}");
            let mut el = tree_item(&row_key, vm, label_key, hover_fill, dark)
                .child(marker("schema-tree-node"))
                .child(chevron(aria_expanded(vm).unwrap_or(false), k))
                .child(icon(&ICON_FOLDER, 14.0).color(k.c.warning.fg))
                .child(text(t(label_key).to_owned()).font(k.ty.text_small).color(k.c.text_muted));
            if let Some(on_action) = host.on_action.clone() {
                let (key, cat) = (key.clone(), cat_id.clone());
                el = el.on_press(move |s: &mut S| on_action(s, RowAction::ToggleCategory {
                    key: key.clone(), cat_id: cat.clone(), db_session_id: String::new(), db_name: String::new(),
                }));
            }
            context(el, host, ContextMenuTarget::Category { key: key.clone(), cat_id: cat_id.clone() })
        }
        RowKind::Table { name, cat_id, is_selected, db_name, connection_id, db_session_id, .. } => {
            let key = format!("nav-tree-table-{name}");
            let fill = if *is_selected { k.c.surface_raised } else { hover_fill };
            let mut el = tree_item(&key, vm, name, fill, dark)
                .child(marker("schema-tree-node"))
                .child(icon(&ICON_TABLE, 14.0).color(if cat_id == "views" { k.c.warning.fg } else { k.c.text_accent }))
                .child(text(name.clone()).font(k.ty.text_small).color(k.c.text_muted));
            if let Some(on_action) = host.on_action.clone() {
                let (n, s, d) = (name.clone(), None, db_name.clone());
                el = el.on_press(move |st: &mut S| on_action(st, RowAction::SelectTable {
                    name: n.clone(), schema: s.clone(), database: d.clone(),
                }));
            }
            context(el, host, ContextMenuTarget::Table { name: name.clone(), schema: None })
        }
        RowKind::Object { name, .. } => {
            let key = format!("nav-tree-object-{name}");
            let mut el = tree_item(&key, vm, name, hover_fill, dark)
                .child(marker("schema-tree-node"))
                .child(icon(&ICON_TABLE, 14.0).color(k.c.warning.fg))
                .child(text(name.clone()).font(k.ty.text_small).color(k.c.text_muted));
            if let Some(on_action) = host.on_action.clone() {
                let name = name.clone();
                el = el.on_press(move |s: &mut S| on_action(s, RowAction::ContextMenu(ContextMenuTarget::Object(name.clone()))));
            }
            context(el, host, ContextMenuTarget::Object(name.clone()))
        }
        RowKind::KvDb { connection_id, db_name, count, .. } => {
            let key = format!("nav-tree-kv-{db_name}");
            let mut el = tree_item(&key, vm, db_name, hover_fill, dark)
                .child(marker("schema-tree-node"))
                .child(icon(&ICON_FOLDER, 14.0).color(k.c.success.fg))
                .child(text(db_name.clone()).font(k.ty.text_small).color(k.c.text_muted));
            if let Some(count) = count {
                el = el.child(
                    text(format!("({count})"))
                        .key(format!("kv-db-count-{db_name}"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                );
            }
            if let Some(on_action) = host.on_action.clone() {
                let (id, db) = (connection_id.clone(), db_name.clone());
                el = el.on_press(move |s: &mut S| on_action(s, RowAction::SelectKvDb {
                    connection_id: id.clone(), db_name: db.clone(),
                }));
            }
            el
        }
        RowKind::DbLoading { owner_key } => {
            tree_item(&format!("nav-tree-loading-{owner_key}"), vm, owner_key, hover_fill, dark)
                .child(icon(&ICON_FOLDER, 12.0).color(k.c.text_muted))
                .child(text(t("common.loading")).font(k.ty.text_small).color(k.c.text_muted))
        }
        RowKind::DbError { message } => {
            tree_item("db-load-error", vm, message, hover_fill, dark)
                .child(icon(&ICON_X, 14.0).color(k.c.danger.fg))
                .child(
                    text(t("schema.loadTablesFailed"))
                        .font(k.ty.text_small)
                        .color(k.c.danger.fg)
                        .wrap(),
                )
                .child(text(message.clone()).font(k.ty.text_xsmall).color(k.c.text_muted))
        }
        RowKind::NamespaceNode { name, expanded, is_leaf, key, connection_id, db_session_id } => {
            if *is_leaf {
                let leaf_key = format!("nav-tree-namespace-leaf-{name}");
                let mut el = tree_item(&leaf_key, vm, name, hover_fill, dark)
                    .child(marker("schema-tree-node"));
                if let Some(on_action) = host.on_action.clone() {
                    let (n, d) = (name.clone(), db_name_from_segments(key));
                    el = el.on_press(move |s: &mut S| on_action(s, RowAction::SelectTable {
                        name: n.clone(), schema: None, database: d.clone(),
                    }));
                }
                context(
                    el.child(icon(&ICON_TABLE, 14.0).color(k.c.text_accent))
                        .child(text(name.clone()).font(k.ty.text_small).color(k.c.text_muted)),
                    host,
                    ContextMenuTarget::Table { name: name.clone(), schema: None },
                )
            } else {
                let row_key = format!("nav-tree-namespace-{key}");
                let mut el = tree_item(&row_key, vm, name, hover_fill, dark)
                    .child(marker("schema-tree-node"))
                    .child(chevron(*expanded, k))
                    .child(icon(&ICON_FOLDER, 14.0).color(k.c.warning.fg))
                    .child(text(name.clone()).font(k.ty.text_small).color(k.c.text_muted));
                if let Some(on_action) = host.on_action.clone() {
                    let (key, session) = (key.clone(), db_session_id.clone());
                    el = el.on_press(move |s: &mut S| on_action(s, RowAction::ToggleNamespace {
                        key: key.clone(), db_session_id: session.clone(), segments: Vec::new(),
                    }));
                }
                el
            }
        }
        RowKind::EmptyGroup { group_name, drop_target } => {
            let key = format!("nav-tree-empty-group-{}", group_name.as_deref().unwrap_or("none"));
            let fill = if *drop_target { k.alpha(k.c.text_accent, 0.2) } else { normal };
            tree_item(&key, vm, "", fill, dark)
                .child(text(t("main.noConnections")).font(k.ty.text_xsmall).color(k.c.text_muted))
        }
        RowKind::NoConnections => {
            tree_item("nav-tree-no-connections", vm, "", hover_fill, dark)
                .child(text(t("main.noConnections")).font(k.ty.text_small).color(k.c.text_muted))
                .child({
                    let mut button = row()
                        .key("new-connection-button")
                        .gap(6.0)
                        .label(t("main.createFirst"))
                        .focusable(true)
                        .child(icon(&ICON_PLUS, 14.0).color(k.c.text_accent))
                        .child(text(t("main.createFirst")).font(k.ty.text_small).color(k.c.text_accent));
                    if let Some(on_action) = host.on_action.clone() {
                        button = button.on_press(move |s: &mut S| on_action(s, RowAction::NewConnection));
                    }
                    button
                })
        }
    }
}

fn db_name_from_segments(key: &str) -> String {
    key.split("::").next().unwrap_or("").to_owned()
}
