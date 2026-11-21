#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainWindowAction {
    NewGroup,
    NewConnection,
    RenameGroup,
    DeleteGroup,
    OpenConnection,
    Disconnect,
    NewQuery,
    QueryHistory,
    EditConnection,
    DuplicateConnection,
    CopyUrl,
    CopyName,
    ObjectFilter,
    ProcessList,
    ServerStatus,
    Backup,
    Restore,
    CreateDatabase,
    CreateSchema,
    CreateUser,
    Refresh,
    PinConnection,
    UnpinConnection,
    MoveToGroup(String),
    RemoveFromGroup,
    DeleteConnection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainWindowMenuItem {
    Item {
        id: String,
        label: String,
        action: MainWindowAction,
    },
    Separator,
    Submenu {
        id: String,
        label: String,
        items: Vec<MainWindowMenuItem>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MainWindowContextMenuLabels {
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
    pub connection: String,
    pub server: String,
    pub organize: String,
    pub create_new: String,
    pub database: String,
    pub user: String,
}

fn item(id: &str, label: String, action: MainWindowAction) -> MainWindowMenuItem {
    MainWindowMenuItem::Item {
        id: id.to_owned(),
        label,
        action,
    }
}

pub fn build_main_blank_context_menu_items(
    labels: &MainWindowContextMenuLabels,
    on_new_group: bool,
    on_new_connection: bool,
) -> Vec<MainWindowMenuItem> {
    let mut items = Vec::new();
    if on_new_group {
        items.push(item(
            "new-group",
            labels.new_group.clone(),
            MainWindowAction::NewGroup,
        ));
    }
    if on_new_connection {
        items.push(item(
            "new-connection",
            labels.new_connection.clone(),
            MainWindowAction::NewConnection,
        ));
    }
    items
}

pub fn build_main_group_context_menu_items(
    labels: &MainWindowContextMenuLabels,
    is_ungrouped: bool,
) -> Vec<MainWindowMenuItem> {
    let mut items = vec![
        item(
            "new-group",
            labels.new_group.clone(),
            MainWindowAction::NewGroup,
        ),
        item(
            "new-connection",
            labels.new_connection.clone(),
            MainWindowAction::NewConnection,
        ),
    ];
    if !is_ungrouped {
        items.push(MainWindowMenuItem::Separator);
        items.push(item(
            "rename-group",
            labels.rename_group.clone(),
            MainWindowAction::RenameGroup,
        ));
        items.push(item(
            "delete-group",
            labels.delete_group.clone(),
            MainWindowAction::DeleteGroup,
        ));
    }
    items
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveTarget {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MainWindowConnectionHandlers {
    pub on_open_or_disconnect: bool,
    pub on_copy_name: bool,
    pub on_copy_url: bool,
    pub on_new_query: bool,
    pub on_query_history: bool,
    pub on_create_database: bool,
    pub on_create_schema: bool,
    pub on_create_user: bool,
    pub on_pin: bool,
    pub on_object_filter: bool,
    pub on_process_list: bool,
    pub on_server_status: bool,
    pub on_backup: bool,
    pub on_restore: bool,
    pub on_refresh: bool,
    pub on_edit: bool,
    pub on_duplicate: bool,
    pub on_move_to_group: bool,
    pub on_remove_from_group: bool,
    pub on_delete: bool,
}

pub fn build_main_connection_context_menu_items(
    labels: &MainWindowContextMenuLabels,
    is_connected: bool,
    grouped: bool,
    pinned: bool,
    move_targets: &[MoveTarget],
    handlers: &MainWindowConnectionHandlers,
) -> Vec<MainWindowMenuItem> {
    let mut items = Vec::new();
    items.push(if is_connected {
        item(
            "disconnect",
            labels.disconnect.clone(),
            MainWindowAction::Disconnect,
        )
    } else {
        item(
            "open-connection",
            labels.open_connection.clone(),
            MainWindowAction::OpenConnection,
        )
    });
    items.push(MainWindowMenuItem::Separator);
    items.push(item(
        "new-query",
        labels.new_query.clone(),
        MainWindowAction::NewQuery,
    ));
    if handlers.on_query_history {
        items.push(item(
            "query-history",
            labels.query_history.clone(),
            MainWindowAction::QueryHistory,
        ));
    }
    items.push(MainWindowMenuItem::Separator);

    let mut connection_items = vec![
        item(
            "edit-connection",
            labels.edit_connection.clone(),
            MainWindowAction::EditConnection,
        ),
        item(
            "duplicate-connection",
            labels.duplicate_connection.clone(),
            MainWindowAction::DuplicateConnection,
        ),
        item(
            "copy-connection-url",
            labels.copy_connection_url.clone(),
            MainWindowAction::CopyUrl,
        ),
        item(
            "copy-name",
            labels.copy_name.clone(),
            MainWindowAction::CopyName,
        ),
    ];
    if handlers.on_object_filter {
        connection_items.push(item(
            "object-filter",
            labels.object_filter.clone(),
            MainWindowAction::ObjectFilter,
        ));
    }
    items.push(MainWindowMenuItem::Submenu {
        id: "connection-submenu".to_owned(),
        label: labels.connection.clone(),
        items: connection_items,
    });

    let mut monitor_items = Vec::new();
    if handlers.on_process_list {
        monitor_items.push(item(
            "process-list",
            labels.process_list.clone(),
            MainWindowAction::ProcessList,
        ));
    }
    if handlers.on_server_status {
        monitor_items.push(item(
            "server-status",
            labels.server_status.clone(),
            MainWindowAction::ServerStatus,
        ));
    }
    let mut ops_items = Vec::new();
    if handlers.on_backup {
        ops_items.push(item(
            "backup",
            labels.backup.clone(),
            MainWindowAction::Backup,
        ));
    }
    if handlers.on_restore {
        ops_items.push(item(
            "restore",
            labels.restore.clone(),
            MainWindowAction::Restore,
        ));
    }
    let mut server_items = monitor_items.clone();
    if !monitor_items.is_empty() && !ops_items.is_empty() {
        server_items.push(MainWindowMenuItem::Separator);
    }
    server_items.extend(ops_items);
    if !server_items.is_empty() {
        items.push(MainWindowMenuItem::Submenu {
            id: "server-submenu".to_owned(),
            label: labels.server.clone(),
            items: server_items,
        });
    }

    if handlers.on_create_database || handlers.on_create_schema || handlers.on_create_user {
        let mut create_items = Vec::new();
        if handlers.on_create_database {
            create_items.push(item(
                "create-database",
                labels.database.clone(),
                MainWindowAction::CreateDatabase,
            ));
        }
        if handlers.on_create_schema {
            create_items.push(item(
                "create-schema",
                labels.create_schema.clone(),
                MainWindowAction::CreateSchema,
            ));
        }
        if handlers.on_create_user {
            create_items.push(item(
                "create-user",
                labels.user.clone(),
                MainWindowAction::CreateUser,
            ));
        }
        items.push(MainWindowMenuItem::Submenu {
            id: "create-new-submenu".to_owned(),
            label: labels.create_new.clone(),
            items: create_items,
        });
    }

    items.push(MainWindowMenuItem::Separator);
    items.push(item(
        "refresh",
        labels.refresh.clone(),
        MainWindowAction::Refresh,
    ));

    let mut organize_items = Vec::new();
    if handlers.on_pin {
        organize_items.push(if pinned {
            item(
                "unpin-connection",
                labels.unpin_connection.clone(),
                MainWindowAction::UnpinConnection,
            )
        } else {
            item(
                "pin-connection",
                labels.pin_connection.clone(),
                MainWindowAction::PinConnection,
            )
        });
    }
    let mut move_items: Vec<MainWindowMenuItem> = move_targets
        .iter()
        .map(|target| {
            item(
                &format!("move-group-{}", target.id),
                target.label.clone(),
                MainWindowAction::MoveToGroup(target.id.clone()),
            )
        })
        .collect();
    if grouped {
        move_items.push(item(
            "remove-from-group",
            labels.remove_from_group.clone(),
            MainWindowAction::RemoveFromGroup,
        ));
    }
    if !move_items.is_empty() {
        if !organize_items.is_empty() {
            organize_items.push(MainWindowMenuItem::Separator);
        }
        organize_items.extend(move_items);
    }
    if !organize_items.is_empty() {
        items.push(MainWindowMenuItem::Submenu {
            id: "organize-submenu".to_owned(),
            label: labels.organize.clone(),
            items: organize_items,
        });
    }

    items.push(MainWindowMenuItem::Separator);
    items.push(item(
        "delete-connection",
        labels.delete_connection.clone(),
        MainWindowAction::DeleteConnection,
    ));
    items
}

pub fn invoke(
    menu_item: &MainWindowMenuItem,
    handlers: &MainWindowConnectionHandlers,
) -> Option<MainWindowAction> {
    let MainWindowMenuItem::Item { action, .. } = menu_item else {
        return None;
    };
    let present = match action {
        MainWindowAction::NewGroup
        | MainWindowAction::NewConnection
        | MainWindowAction::RenameGroup
        | MainWindowAction::DeleteGroup => false,
        MainWindowAction::OpenConnection | MainWindowAction::Disconnect => {
            handlers.on_open_or_disconnect
        }
        MainWindowAction::NewQuery => handlers.on_new_query,
        MainWindowAction::QueryHistory => handlers.on_query_history,
        MainWindowAction::EditConnection => handlers.on_edit,
        MainWindowAction::DuplicateConnection => handlers.on_duplicate,
        MainWindowAction::CopyUrl => handlers.on_copy_url,
        MainWindowAction::CopyName => handlers.on_copy_name,
        MainWindowAction::ObjectFilter => handlers.on_object_filter,
        MainWindowAction::ProcessList => handlers.on_process_list,
        MainWindowAction::ServerStatus => handlers.on_server_status,
        MainWindowAction::Backup => handlers.on_backup,
        MainWindowAction::Restore => handlers.on_restore,
        MainWindowAction::CreateDatabase => handlers.on_create_database,
        MainWindowAction::CreateSchema => handlers.on_create_schema,
        MainWindowAction::CreateUser => handlers.on_create_user,
        MainWindowAction::Refresh => handlers.on_refresh,
        MainWindowAction::PinConnection | MainWindowAction::UnpinConnection => handlers.on_pin,
        MainWindowAction::MoveToGroup(_) => handlers.on_move_to_group,
        MainWindowAction::RemoveFromGroup => handlers.on_remove_from_group,
        MainWindowAction::DeleteConnection => handlers.on_delete,
    };
    if present {
        Some(action.clone())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> MainWindowContextMenuLabels {
        MainWindowContextMenuLabels {
            new_group: "New Group".to_owned(),
            new_connection: "New Connection".to_owned(),
            rename_group: "Rename".to_owned(),
            delete_group: "Delete Group".to_owned(),
            open_connection: "Open".to_owned(),
            disconnect: "Disconnect".to_owned(),
            edit_connection: "Edit".to_owned(),
            duplicate_connection: "Duplicate".to_owned(),
            move_to_group: "Move".to_owned(),
            remove_from_group: "Remove".to_owned(),
            delete_connection: "Delete".to_owned(),
            copy_name: "Copy Name".to_owned(),
            copy_connection_url: "Copy URL".to_owned(),
            new_query: "New Query".to_owned(),
            query_history: "History".to_owned(),
            create_database: "Create DB".to_owned(),
            create_schema: "Create Schema".to_owned(),
            create_user: "Create User".to_owned(),
            refresh: "Refresh".to_owned(),
            pin_connection: "Pin".to_owned(),
            unpin_connection: "Unpin".to_owned(),
            object_filter: "Filter".to_owned(),
            process_list: "Processes".to_owned(),
            server_status: "Status".to_owned(),
            backup: "Backup".to_owned(),
            restore: "Restore".to_owned(),
            connection: "Connection".to_owned(),
            server: "Server".to_owned(),
            organize: "Organize".to_owned(),
            create_new: "Create New".to_owned(),
            database: "DB".to_owned(),
            user: "User".to_owned(),
        }
    }

    fn ids(items: &[MainWindowMenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|entry| match entry {
                MainWindowMenuItem::Item { id, .. } | MainWindowMenuItem::Submenu { id, .. } => {
                    id.clone()
                }
                MainWindowMenuItem::Separator => "separator".to_owned(),
            })
            .collect()
    }

    fn handlers() -> MainWindowConnectionHandlers {
        MainWindowConnectionHandlers {
            on_open_or_disconnect: true,
            on_copy_name: true,
            on_copy_url: true,
            on_new_query: true,
            on_query_history: true,
            on_create_database: true,
            on_create_schema: true,
            on_create_user: true,
            on_pin: true,
            on_object_filter: true,
            on_process_list: true,
            on_server_status: true,
            on_backup: true,
            on_restore: true,
            on_refresh: true,
            on_edit: true,
            on_duplicate: true,
            on_move_to_group: true,
            on_remove_from_group: true,
            on_delete: true,
        }
    }

    #[test]
    fn blank_menu_has_only_the_two_creation_entries() {
        let items = build_main_blank_context_menu_items(&labels(), true, true);
        assert_eq!(ids(&items), ["new-group", "new-connection"]);
        assert!(build_main_blank_context_menu_items(&labels(), false, false).is_empty());
    }

    #[test]
    fn group_menu_hides_management_for_ungrouped() {
        let grouped = build_main_group_context_menu_items(&labels(), false);
        assert_eq!(
            ids(&grouped),
            [
                "new-group",
                "new-connection",
                "separator",
                "rename-group",
                "delete-group"
            ]
        );
        let ungrouped = build_main_group_context_menu_items(&labels(), true);
        assert_eq!(ids(&ungrouped), ["new-group", "new-connection"]);
    }

    #[test]
    fn connection_menu_builds_the_full_layout_in_order() {
        let targets = vec![MoveTarget {
            id: "g1".to_owned(),
            label: "Group 1".to_owned(),
        }];
        let items = build_main_connection_context_menu_items(
            &labels(),
            false,
            true,
            false,
            &targets,
            &handlers(),
        );
        assert_eq!(
            ids(&items),
            [
                "open-connection",
                "separator",
                "new-query",
                "query-history",
                "separator",
                "connection-submenu",
                "server-submenu",
                "create-new-submenu",
                "separator",
                "refresh",
                "organize-submenu",
                "separator",
                "delete-connection"
            ]
        );
        let sub = |id: &str| {
            items
                .iter()
                .find_map(|entry| match entry {
                    MainWindowMenuItem::Submenu {
                        id: actual, items, ..
                    } if actual == id => Some(items),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(
            ids(sub("connection-submenu")),
            [
                "edit-connection",
                "duplicate-connection",
                "copy-connection-url",
                "copy-name",
                "object-filter"
            ]
        );
        assert_eq!(
            ids(sub("server-submenu")),
            [
                "process-list",
                "server-status",
                "separator",
                "backup",
                "restore"
            ]
        );
        assert_eq!(
            ids(sub("create-new-submenu")),
            ["create-database", "create-schema", "create-user"]
        );
        assert_eq!(
            ids(sub("organize-submenu")),
            [
                "pin-connection",
                "separator",
                "move-group-g1",
                "remove-from-group"
            ]
        );
    }

    #[test]
    fn connected_pinned_and_optional_handlers_change_labels_and_items() {
        let mut h = handlers();
        h.on_query_history = false;
        h.on_backup = false;
        h.on_restore = false;
        h.on_create_database = false;
        h.on_create_schema = false;
        h.on_create_user = false;
        h.on_process_list = false;
        h.on_server_status = false;
        h.on_object_filter = false;
        h.on_pin = false;
        let items = build_main_connection_context_menu_items(&labels(), true, false, true, &[], &h);
        assert_eq!(
            ids(&items),
            [
                "disconnect",
                "separator",
                "new-query",
                "separator",
                "connection-submenu",
                "separator",
                "refresh",
                "separator",
                "delete-connection"
            ]
        );
        assert_eq!(
            items[0],
            item(
                "disconnect",
                "Disconnect".to_owned(),
                MainWindowAction::Disconnect
            )
        );
        let sub = |id: &str| {
            items
                .iter()
                .find_map(|entry| match entry {
                    MainWindowMenuItem::Submenu {
                        id: actual, items, ..
                    } if actual == id => Some(items),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(
            ids(sub("connection-submenu")),
            [
                "edit-connection",
                "duplicate-connection",
                "copy-connection-url",
                "copy-name"
            ]
        );

        let mut pin_only = handlers();
        pin_only.on_move_to_group = false;
        pin_only.on_remove_from_group = false;
        let items =
            build_main_connection_context_menu_items(&labels(), false, false, true, &[], &pin_only);
        let organize = items
            .iter()
            .find_map(|entry| match entry {
                MainWindowMenuItem::Submenu { id, items, .. } if id == "organize-submenu" => {
                    Some(items)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(ids(organize), ["unpin-connection"]);
        assert_eq!(
            organize[0],
            item(
                "unpin-connection",
                "Unpin".to_owned(),
                MainWindowAction::UnpinConnection
            )
        );
    }

    #[test]
    fn server_submenu_separates_monitors_from_ops() {
        let mut monitors_only = handlers();
        monitors_only.on_backup = false;
        monitors_only.on_restore = false;
        let items = build_main_connection_context_menu_items(
            &labels(),
            false,
            false,
            false,
            &[],
            &monitors_only,
        );
        let server = items
            .iter()
            .find_map(|entry| match entry {
                MainWindowMenuItem::Submenu { id, items, .. } if id == "server-submenu" => {
                    Some(items)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(ids(server), ["process-list", "server-status"]);

        let mut ops_only = handlers();
        ops_only.on_process_list = false;
        ops_only.on_server_status = false;
        let items = build_main_connection_context_menu_items(
            &labels(),
            false,
            false,
            false,
            &[],
            &ops_only,
        );
        let server = items
            .iter()
            .find_map(|entry| match entry {
                MainWindowMenuItem::Submenu { id, items, .. } if id == "server-submenu" => {
                    Some(items)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(ids(server), ["backup", "restore"]);
    }

    #[test]
    fn invoke_maps_actions_and_skips_non_items() {
        let targets = vec![MoveTarget {
            id: "g1".to_owned(),
            label: "G1".to_owned(),
        }];
        let items = build_main_connection_context_menu_items(
            &labels(),
            false,
            true,
            false,
            &targets,
            &handlers(),
        );
        assert_eq!(
            invoke(&items[0], &handlers()),
            Some(MainWindowAction::OpenConnection)
        );
        assert_eq!(invoke(&items[1], &handlers()), None);
        let move_item = MainWindowMenuItem::Item {
            id: "move-group-g1".to_owned(),
            label: "G1".to_owned(),
            action: MainWindowAction::MoveToGroup("g1".to_owned()),
        };
        assert_eq!(
            invoke(&move_item, &handlers()),
            Some(MainWindowAction::MoveToGroup("g1".to_owned()))
        );
        let mut off = handlers();
        off.on_open_or_disconnect = false;
        assert_eq!(invoke(&items[0], &off), None);
    }
}
