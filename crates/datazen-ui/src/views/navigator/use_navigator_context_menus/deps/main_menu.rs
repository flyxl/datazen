//! React `lib/mainWindowContextMenu.ts` — the main window's group and
//! connection menu builders, ported verbatim for the navigator (T578).
//! Removal stage: shard 2's T555 ports the file formally (its blank-area
//! builder included); this moves beside it, behaviour unchanged.
//!
//! React callbacks become pre-built actions of the caller's type `A`: the
//! builders only ever *place* them, never call them.

use super::native_menu::MenuNode;

/// React `MainWindowContextMenuLabels` — every label the two builders read,
/// supplied by the caller (the navigator builds it from `t()`).
#[derive(Clone, Debug, Default)]
pub(crate) struct MainWindowContextMenuLabels {
    pub new_group: String,
    pub new_connection: String,
    pub rename_group: String,
    pub delete_group: String,
    pub open_connection: String,
    pub disconnect: String,
    pub edit_connection: String,
    pub duplicate_connection: String,
    pub move_to_group: String,
    pub remove_from_group: String,
    pub delete_connection: String,
    pub copy_name: String,
    pub copy_connection_url: String,
    pub new_query: String,
    pub query_history: String,
    pub create_database: String,
    pub create_schema: String,
    pub create_user: String,
    pub refresh: String,
    pub pin_connection: String,
    pub unpin_connection: String,
    pub object_filter: String,
    pub process_list: String,
    pub server_status: String,
    pub backup: String,
    pub restore: String,
    /// Submenu header labels.
    pub connection: String,
    pub server: String,
    pub organize: String,
    pub create_new: String,
    /// Short label for create-database inside the create-new submenu.
    pub database: String,
    /// Short label for create-user inside the create-new submenu.
    pub user: String,
}

/// React `buildMainGroupContextMenuItems`. The rename/delete pair is gated
/// on `is_ungrouped` inside the builder, exactly as in React.
pub(crate) fn build_main_group_context_menu_items<A: Clone>(
    labels: &MainWindowContextMenuLabels,
    is_ungrouped: bool,
    on_new_group: A,
    on_new_connection: A,
    on_rename_group: A,
    on_delete_group: A,
) -> Vec<MenuNode<A>> {
    let mut items = vec![
        MenuNode::item("new-group", &labels.new_group, on_new_group),
        MenuNode::item("new-connection", &labels.new_connection, on_new_connection),
    ];
    if !is_ungrouped {
        items.push(MenuNode::separator());
        items.push(MenuNode::item("rename-group", &labels.rename_group, on_rename_group));
        items.push(MenuNode::item("delete-group", &labels.delete_group, on_delete_group));
    }
    items
}

/// One move target of the connection menu's organize submenu: the group's
/// storage key, its display label, and the action moving there.
#[derive(Clone, Debug)]
pub(crate) struct MoveTarget<A> {
    pub id: String,
    pub label: String,
    pub action: A,
}

/// React `buildMainConnectionContextMenuItems`' callback set, as slots. The
/// `Option` slots are React's optional handlers: a submenu entry appears
/// only when its handler is wired.
pub(crate) struct ConnectionMenuSlots<A> {
    pub on_open_or_disconnect: A,
    pub on_copy_name: A,
    pub on_copy_url: A,
    pub on_new_query: A,
    pub on_query_history: Option<A>,
    pub on_create_database: Option<A>,
    pub on_create_schema: Option<A>,
    pub on_create_user: Option<A>,
    pub on_pin: Option<A>,
    pub on_object_filter: Option<A>,
    pub on_process_list: Option<A>,
    pub on_server_status: Option<A>,
    pub on_backup: Option<A>,
    pub on_restore: Option<A>,
    pub on_refresh: A,
    pub on_edit: A,
    pub on_duplicate: A,
    pub move_targets: Vec<MoveTarget<A>>,
    pub on_remove_from_group: A,
    pub on_delete: A,
}

/// React `buildMainConnectionContextMenuItems`: primary action, query
/// actions, the connection submenu (edit/duplicate/copy/filter), the server
/// submenu (monitoring + backup/restore), the create submenu
/// (database/schema/user), then refresh / organize / delete.
pub(crate) fn build_main_connection_context_menu_items<A: Clone>(
    labels: &MainWindowContextMenuLabels,
    is_connected: bool,
    grouped: bool,
    pinned: bool,
    slots: ConnectionMenuSlots<A>,
) -> Vec<MenuNode<A>> {
    let mut items: Vec<MenuNode<A>> = Vec::new();

    // ── Primary actions ────────────────────────────────────────
    items.push(MenuNode::item(
        if is_connected { "disconnect" } else { "open-connection" },
        if is_connected { &labels.disconnect } else { &labels.open_connection },
        slots.on_open_or_disconnect,
    ));

    items.push(MenuNode::separator());

    // ── Query actions ──────────────────────────────────────────
    items.push(MenuNode::item("new-query", &labels.new_query, slots.on_new_query));

    if let Some(action) = slots.on_query_history {
        items.push(MenuNode::item("query-history", &labels.query_history, action));
    }

    items.push(MenuNode::separator());

    // ── Connection submenu (edit / duplicate / copy / filter) ──
    let mut connection_items = vec![
        MenuNode::item("edit-connection", &labels.edit_connection, slots.on_edit),
        MenuNode::item(
            "duplicate-connection",
            &labels.duplicate_connection,
            slots.on_duplicate,
        ),
        MenuNode::item("copy-connection-url", &labels.copy_connection_url, slots.on_copy_url),
        MenuNode::item("copy-name", &labels.copy_name, slots.on_copy_name),
    ];
    if let Some(action) = slots.on_object_filter {
        connection_items.push(MenuNode::item("object-filter", &labels.object_filter, action));
    }
    items.push(MenuNode::Submenu {
        id: "connection-submenu".to_owned(),
        label: labels.connection.clone(),
        items: connection_items,
    });

    // ── Server submenu (monitoring + backup/restore) ───────────
    let mut monitor_items = Vec::new();
    let mut ops_items = Vec::new();
    if let Some(action) = slots.on_process_list {
        monitor_items.push(MenuNode::item("process-list", &labels.process_list, action));
    }
    if let Some(action) = slots.on_server_status {
        monitor_items.push(MenuNode::item("server-status", &labels.server_status, action));
    }
    if let Some(action) = slots.on_backup {
        ops_items.push(MenuNode::item("backup", &labels.backup, action));
    }
    if let Some(action) = slots.on_restore {
        ops_items.push(MenuNode::item("restore", &labels.restore, action));
    }
    let mut server_items = monitor_items.clone();
    if !monitor_items.is_empty() && !ops_items.is_empty() {
        server_items.push(MenuNode::separator());
    }
    server_items.extend(ops_items);
    if !server_items.is_empty() {
        items.push(MenuNode::Submenu {
            id: "server-submenu".to_owned(),
            label: labels.server.clone(),
            items: server_items,
        });
    }

    // ── Create submenu (database / schema / user) ──────────────
    if slots.on_create_database.is_some()
        || slots.on_create_schema.is_some()
        || slots.on_create_user.is_some()
    {
        let mut create_items = Vec::new();
        if let Some(action) = slots.on_create_database.clone() {
            create_items.push(MenuNode::item("create-database", &labels.database, action));
        }
        if let Some(action) = slots.on_create_schema.clone() {
            create_items.push(MenuNode::item("create-schema", &labels.create_schema, action));
        }
        if let Some(action) = slots.on_create_user.clone() {
            create_items.push(MenuNode::item("create-user", &labels.user, action));
        }
        items.push(MenuNode::Submenu {
            id: "create-new-submenu".to_owned(),
            label: labels.create_new.clone(),
            items: create_items,
        });
    }

    // ── Bottom section: refresh / organize / delete ────────────
    items.push(MenuNode::separator());
    items.push(MenuNode::item("refresh", &labels.refresh, slots.on_refresh));

    let mut organize_items = Vec::new();
    if let Some(action) = slots.on_pin {
        organize_items.push(MenuNode::item(
            if pinned { "unpin-connection" } else { "pin-connection" },
            if pinned { &labels.unpin_connection } else { &labels.pin_connection },
            action,
        ));
    }
    let mut move_items: Vec<MenuNode<A>> = slots
        .move_targets
        .iter()
        .map(|t| {
            MenuNode::item(&format!("move-group-{}", t.id), &t.label, t.action.clone())
        })
        .collect();
    if grouped {
        move_items.push(MenuNode::item(
            "remove-from-group",
            &labels.remove_from_group,
            slots.on_remove_from_group,
        ));
    }
    if !move_items.is_empty() {
        if !organize_items.is_empty() {
            organize_items.push(MenuNode::separator());
        }
        organize_items.extend(move_items);
    }
    if !organize_items.is_empty() {
        items.push(MenuNode::Submenu {
            id: "organize-submenu".to_owned(),
            label: labels.organize.clone(),
            items: organize_items,
        });
    }

    items.push(MenuNode::separator());
    items.push(MenuNode::item(
        "delete-connection",
        &labels.delete_connection,
        slots.on_delete,
    ));
    items
}
