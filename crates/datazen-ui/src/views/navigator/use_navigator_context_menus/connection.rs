//! The connection-row context menu — React
//! `buildMainConnectionContextMenuItems` plus the capability derivation the
//! hook does just before it (`handleConnectionContextMenu`).
//!
//! Item order and submenu grouping mirror the builder exactly:
//! `open/disconnect → query → connection → server → create → refresh →
//! organize → delete`. The connection submenu's create-new half never carries
//! a create-schema entry: the hook passes `onCreateDatabase` and `onCreateUser`
//! only, so that item is deliberately absent here too.

use super::super::build_flat_rows::deps::utils::should_use_multi_database_tree;
use super::{
    act, at, dynamic_item, format_group_label, item, separator, submenu, DriverCaps, MenuEntry,
    NavActionKind, NavTarget, ProductFeatures, ViewActionCaps,
};

/// The hook's input for one right-clicked connection.
#[allow(dead_code)] // VM mirror; supplied by the navigator host.
#[derive(Clone, Debug, Default)]
pub(crate) struct ConnectionMenuInput {
    pub id: String,
    pub section_group: String,
    /// `conn.group` — `None` means ungrouped.
    pub group: Option<String>,
    pub pinned: bool,
    pub is_connected: bool,
    pub db_session_id: Option<String>,
    /// Other group names, already ordered by the host (React `groups.filter`).
    pub move_targets: Vec<String>,
    pub driver: DriverCaps,
    pub initial_database: Option<String>,
    /// `hasCommand(commands, SERVER_STATUS_SNAPSHOT_COMMAND)`.
    pub has_server_status_command: bool,
    /// `hasCommand(commands, LIST_PROCESSES_COMMAND)`.
    pub has_process_list_command: bool,
    pub view: ViewActionCaps,
    pub features: ProductFeatures,
}

/// React `shouldUseMultiDatabaseTree(dbMeta, conn.database)`.
#[allow(dead_code)] // deps mirror; remove when the libs are ported.
pub(crate) fn is_multi_database(driver: &DriverCaps, initial_database: Option<&str>) -> bool {
    should_use_multi_database_tree(
        driver.has_multi_database,
        &driver.database_field_type,
        initial_database,
    )
}

/// The capability flags the hook computes before calling the builder.
#[allow(dead_code)] // VM mirror; supplied by the navigator host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConnectionCaps {
    pub supports_server_status: bool,
    pub supports_process_list: bool,
    pub supports_backup: bool,
    pub supports_create_database: bool,
    pub supports_create_user: bool,
}

#[allow(dead_code)] // pure projection; consumed by the builder and tests.
pub(crate) fn derive_caps(input: &ConnectionMenuInput) -> ConnectionCaps {
    ConnectionCaps {
        supports_server_status: input.is_connected && input.has_server_status_command,
        supports_process_list: input.is_connected && input.has_process_list_command,
        supports_backup: input.driver.supports_backup,
        supports_create_database: input.driver.supports_create_database
            && is_multi_database(&input.driver, input.initial_database.as_deref()),
        supports_create_user: input.driver.supports_create_user,
    }
}

/// The favorite (pin/unpin) entry — the one spelling of the connection menu's
/// pin action. React calls it `pin-connection` / `unpin-connection`.
#[allow(dead_code)] // menu builder; remove when the navigator host wires it.
pub(crate) fn favorite_entry(pinned: bool) -> MenuEntry {
    if pinned {
        item(
            "unpin-connection",
            "main.ctx.unpinConnection",
            act(NavActionKind::ToggleFavorite),
        )
    } else {
        item(
            "pin-connection",
            "main.ctx.pinConnection",
            act(NavActionKind::ToggleFavorite),
        )
    }
}

fn connection_submenu(input: &ConnectionMenuInput) -> MenuEntry {
    submenu(
        "connection-submenu",
        "main.ctx.connection",
        vec![
            item(
                "edit-connection",
                "common.editConnection",
                at(
                    NavActionKind::EditConnection,
                    NavTarget {
                        id: input.id.clone(),
                        ..NavTarget::default()
                    },
                ),
            ),
            item(
                "duplicate-connection",
                "main.ctx.duplicateConnection",
                at(
                    NavActionKind::DuplicateConnection,
                    NavTarget {
                        id: input.id.clone(),
                        ..NavTarget::default()
                    },
                ),
            ),
            item(
                "copy-connection-url",
                "main.ctx.copyConnectionUrl",
                at(
                    NavActionKind::CopyConnectionUrl,
                    NavTarget {
                        id: input.id.clone(),
                        ..NavTarget::default()
                    },
                ),
            ),
            item(
                "copy-name",
                "common.copyName",
                at(
                    NavActionKind::CopyName,
                    NavTarget {
                        name: String::new(), // host fills conn.name at fire time
                        ..NavTarget::default()
                    },
                ),
            ),
            item(
                "object-filter",
                "common.objectFilter",
                at(
                    NavActionKind::ObjectFilter,
                    NavTarget {
                        id: input.id.clone(),
                        ..NavTarget::default()
                    },
                ),
            ),
        ],
    )
}

fn server_submenu(caps: ConnectionCaps) -> Option<MenuEntry> {
    let mut monitor: Vec<MenuEntry> = Vec::new();
    if caps.supports_process_list {
        monitor.push(item(
            "process-list",
            "common.processList",
            act(NavActionKind::OpenProcessList),
        ));
    }
    if caps.supports_server_status {
        monitor.push(item(
            "server-status",
            "common.serverStatus",
            act(NavActionKind::OpenServerStatus),
        ));
    }
    let mut ops: Vec<MenuEntry> = Vec::new();
    if caps.supports_backup {
        ops.push(item("backup", "common.backupDatabase", act(NavActionKind::Backup)));
        ops.push(item(
            "restore",
            "common.restoreDatabase",
            act(NavActionKind::Restore),
        ));
    }
    if monitor.is_empty() && ops.is_empty() {
        return None;
    }
    let mut children = monitor;
    if !children.is_empty() && !ops.is_empty() {
        children.push(separator());
    }
    children.extend(ops);
    Some(submenu("server-submenu", "main.ctx.server", children))
}

fn create_submenu(caps: ConnectionCaps) -> Option<MenuEntry> {
    if !caps.supports_create_database && !caps.supports_create_user {
        return None;
    }
    let mut children = Vec::new();
    if caps.supports_create_database {
        children.push(item(
            "create-database",
            "common.database",
            act(NavActionKind::CreateDatabase),
        ));
    }
    if caps.supports_create_user {
        children.push(item(
            "create-user",
            "common.user",
            act(NavActionKind::CreateUser),
        ));
    }
    Some(submenu("create-new-submenu", "main.ctx.createNew", children))
}

fn organize_submenu(input: &ConnectionMenuInput) -> Option<MenuEntry> {
    let mut children = vec![favorite_entry(input.pinned)];
    let mut moves: Vec<MenuEntry> = input
        .move_targets
        .iter()
        .filter(|group| group.as_str() != input.group.as_deref().unwrap_or(""))
        .map(|group| {
            dynamic_item(
                format!("move-group-{group}"),
                format_group_label(group),
                at(
                    NavActionKind::MoveToGroup,
                    NavTarget {
                        id: input.id.clone(),
                        group: group.clone(),
                        ..NavTarget::default()
                    },
                ),
            )
        })
        .collect();
    if input.group.as_deref().is_some_and(|g| !g.is_empty()) {
        moves.push(item(
            "remove-from-group",
            "main.ctx.removeFromGroup",
            at(
                NavActionKind::RemoveFromGroup,
                NavTarget {
                    id: input.id.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    }
    if !moves.is_empty() {
        children.push(separator());
        children.extend(moves);
    }
    Some(submenu("organize-submenu", "main.ctx.organize", children))
}

/// React `buildMainConnectionContextMenuItems`, with the hook's handler
/// availability folded in.
#[allow(dead_code)] // menu builder; remove when the navigator host wires it.
pub(crate) fn build_connection_menu(input: &ConnectionMenuInput) -> Vec<MenuEntry> {
    let caps = derive_caps(input);
    let mut items: Vec<MenuEntry> = Vec::new();

    if input.is_connected {
        items.push(item(
            "disconnect",
            "main.ctx.disconnect",
            at(
                NavActionKind::OpenOrDisconnect,
                NavTarget {
                    id: input.id.clone(),
                    section_group: input.section_group.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    } else {
        items.push(item(
            "open-connection",
            "main.ctx.openConnection",
            at(
                NavActionKind::OpenOrDisconnect,
                NavTarget {
                    id: input.id.clone(),
                    section_group: input.section_group.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    }
    items.push(separator());

    items.push(item(
        "new-query",
        "common.newQuery",
        at(
            NavActionKind::NewQuery,
            NavTarget {
                id: input.id.clone(),
                ..NavTarget::default()
            },
        ),
    ));
    // The hook always supplies onQueryHistory: connected views open history,
    // disconnected ones stage a pending intent and connect first.
    items.push(item(
        "query-history",
        "main.ctx.queryHistory",
        at(
            NavActionKind::OpenQueryHistory,
            NavTarget {
                id: input.id.clone(),
                ..NavTarget::default()
            },
        ),
    ));
    items.push(separator());

    items.push(connection_submenu(input));
    if let Some(server) = server_submenu(caps) {
        items.push(server);
    }
    if let Some(create) = create_submenu(caps) {
        items.push(create);
    }

    items.push(separator());
    items.push(item(
        "refresh",
        "main.ctx.refresh",
        at(
            NavActionKind::RefreshConnection,
            NavTarget {
                id: input.id.clone(),
                ..NavTarget::default()
            },
        ),
    ));
    if let Some(organize) = organize_submenu(input) {
        items.push(organize);
    }
    items.push(separator());
    items.push(item(
        "delete-connection",
        "common.deleteConnection",
        at(
            NavActionKind::DeleteConnection,
            NavTarget {
                id: input.id.clone(),
                ..NavTarget::default()
            },
        ),
    ));
    items
}
