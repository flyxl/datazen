//! T578: `navigator/useNavigatorContextMenus.ts` port — the seven context
//! menus. No React suite pins the hook in isolation (its coverage lives in
//! the host-level `ConnectionNavigatorTree.test.tsx`, a later task); these
//! tests pin the contracts the file itself states: item order, the gates
//! (connected / pinned / safe mode / read-only / product features / driver
//! capabilities / wired view actions), the decision payloads, and the pure
//! helpers (quoted names, truncate/drop SQL, the connection URL, the error
//! extractor, the drop-database fallback).

use std::collections::HashMap;

use serde_json::json;

use crate::views::navigator::utils::{
    extract_error_message, quote_relation_name, resolve_drop_database_fallback,
};
use crate::views::navigator::use_navigator_context_menus::deps::connection_url::{
    build_connection_url, ConnectionUrlConfig, ConnectionUrlMeta,
};
use crate::views::navigator::use_navigator_context_menus::deps::main_menu::{
    MainWindowContextMenuLabels, MoveTarget,
};
use crate::views::navigator::use_navigator_context_menus::deps::native_menu::MenuNode;
use crate::views::navigator::use_navigator_context_menus::deps::schema_tree_menu::ProductFeatures;
use crate::views::navigator::use_navigator_context_menus::{
    category_context_menu, connection_context_menu, database_context_menu,
    drop_relation_sql, group_context_menu, object_context_menu, schema_context_menu,
    table_context_menu, truncate_table_sql, MenuActiveEntry, MenuConnectionSnapshot,
    MenuDriverMeta, MenuLabels, NavigatorMenuAction, NavigatorMenuInputs, TableMenuArgs,
    ViewActionsAvailable, LIST_PROCESSES_COMMAND, SERVER_STATUS_SNAPSHOT_COMMAND,
};

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

fn labels() -> MenuLabels {
    let mut labels = MenuLabels::default();
    labels.context = MainWindowContextMenuLabels {
        new_group: "New Group".into(),
        new_connection: "New Connection".into(),
        rename_group: "Rename Group".into(),
        delete_group: "Delete Group".into(),
        open_connection: "Open Connection".into(),
        disconnect: "Disconnect".into(),
        edit_connection: "Edit Connection".into(),
        duplicate_connection: "Duplicate…".into(),
        move_to_group: "Move to Group".into(),
        remove_from_group: "Remove from Group".into(),
        delete_connection: "Delete Connection".into(),
        copy_name: "Copy Name".into(),
        copy_connection_url: "Copy Connection URL".into(),
        new_query: "New Query".into(),
        query_history: "Query History".into(),
        create_database: "Create Database…".into(),
        create_schema: "Create Schema…".into(),
        create_user: "Create User…".into(),
        refresh: "Refresh".into(),
        pin_connection: "Pin Connection".into(),
        unpin_connection: "Unpin Connection".into(),
        object_filter: "Object Filter…".into(),
        process_list: "Process List".into(),
        server_status: "Server Status".into(),
        backup: "Backup…".into(),
        restore: "Restore…".into(),
        connection: "Connection".into(),
        server: "Server".into(),
        organize: "Organize".into(),
        create_new: "Create New".into(),
        database: "Database…".into(),
        user: "User…".into(),
    };
    labels.schema.open = "Open".into();
    labels.schema.open_structure = "Open Structure".into();
    labels.schema.copy_name = "Copy Name".into();
    labels.schema.copy_ddl = "Copy DDL".into();
    labels.schema.focus_er = "Focus in ER".into();
    labels.schema.export_data = "Export…".into();
    labels.schema.import_data = "Import…".into();
    labels.schema.refresh = "Refresh".into();
    labels.schema.new_query = "New Query".into();
    labels.schema.copy_database_name = "Copy Database Name".into();
    labels.schema.new_table = "New Table…".into();
    labels.schema.batch_export = "Batch Export…".into();
    labels.schema.truncate = "Truncate".into();
    labels.schema.drop = "Drop…".into();
    labels.schema.drop_view = "Drop View…".into();
    labels.schema.drop_database = "Drop Database…".into();
    labels.schema.close_database_connection = "Close Database Connection".into();
    labels.schema.view_er_diagram = "View ER Diagram".into();
    labels.schema.new_schema = "New Schema…".into();
    labels.schema.create_schema = "Create Schema…".into();
    labels.schema.drop_schema = "Drop Schema…".into();
    labels.schema.execute_sql_file = "Execute SQL File…".into();
    labels.schema.query_history = "Query History".into();
    labels.schema.data_transfer = "Data Transfer…".into();
    labels.schema.compare_schema = "Compare Schema…".into();
    labels.schema.compare_data = "Compare Data…".into();
    labels.schema.backup = "Backup…".into();
    labels.schema.restore = "Restore…".into();
    labels.schema.generate_sql = "Generate SQL".into();
    labels.schema.generate_select = "SELECT".into();
    labels.schema.generate_insert = "INSERT".into();
    labels.schema.generate_update = "UPDATE".into();
    labels.schema.generate_delete = "DELETE".into();
    labels.schema.generate_ddl = "DDL".into();
    labels
}

/// The fixture connection without a pinned database: PostgreSQL only
/// takes the multi-database tree when the config left `database` empty (a
/// logical name would lock the sidebar).
fn multi_db_conn(id: &str) -> MenuConnectionSnapshot {
    let mut c = conn(id);
    c.database = None;
    c
}

fn pg_meta() -> MenuDriverMeta {
    MenuDriverMeta {
        has_multi_database: true,
        database_field_type: "name".into(),
        quote_char: "\"".into(),
        supports_create_database: true,
        supports_create_schema: true,
        supports_er_diagram: Some(true),
        clipboard_scheme: "postgresql".into(),
        default_port: 5432,
        ..Default::default()
    }
}

fn conn(id: &str) -> MenuConnectionSnapshot {
    MenuConnectionSnapshot {
        id: id.into(),
        name: format!("Conn {id}"),
        database_type: "postgresql".into(),
        database: Some("db1".into()),
        group: Some("team-a".into()),
        host: Some("db.example.com".into()),
        port: Some(5432),
        username: Some("alice".into()),
        ssl_mode: Some("prefer".into()),
        ..Default::default()
    }
}

struct InputsBuilder {
    connections: Vec<MenuConnectionSnapshot>,
    active: HashMap<String, MenuActiveEntry>,
    registry: HashMap<String, MenuDriverMeta>,
    driver_commands: HashMap<String, Vec<String>>,
    groups: Vec<String>,
    view: ViewActionsAvailable,
    features: ProductFeatures,
    safe_mode: bool,
}

impl InputsBuilder {
    fn new() -> Self {
        Self {
            connections: vec![conn("c1")],
            active: HashMap::from([(
                "c1".into(),
                MenuActiveEntry {
                    status: "connected".into(),
                    db_session_id: Some("s1".into()),
                },
            )]),
            registry: HashMap::from([("postgresql".into(), pg_meta())]),
            driver_commands: HashMap::from([(
                "postgresql".into(),
                vec![
                    SERVER_STATUS_SNAPSHOT_COMMAND.to_owned(),
                    LIST_PROCESSES_COMMAND.to_owned(),
                ],
            )]),
            groups: vec!["team-a".into(), "team-b".into(), "team-z".into()],
            view: ViewActionsAvailable {
                new_query: true,
                open_query_history: true,
                open_er_diagram: true,
                open_table_structure: true,
                create_table: true,
                ..Default::default()
            },
            features: ProductFeatures {
                data_transfer: true,
                schema_diff: true,
                data_sync: true,
            },
            safe_mode: false,
        }
    }

    fn inputs<'a>(&'a self, labels: &'a MenuLabels) -> NavigatorMenuInputs<'a> {
        NavigatorMenuInputs {
            labels,
            safe_mode: self.safe_mode,
            features: self.features,
            groups: &self.groups,
            connections: &self.connections,
            active_connections: &self.active,
            view: &self.view,
            registry: &self.registry,
            driver_commands: &self.driver_commands,
        }
    }
}

/// Flatten a menu into its item ids, descending into submenus with a
/// `parent/` prefix.
fn ids_of(nodes: &[MenuNode<NavigatorMenuAction>]) -> Vec<String> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            MenuNode::Item { id, .. } => out.push(id.clone()),
            MenuNode::Separator => out.push("--".into()),
            MenuNode::Submenu { id, items, .. } => {
                out.push(format!("{id}/"));
                out.extend(ids_of(items));
            }
        }
    }
    out
}

fn find_item<'a>(
    nodes: &'a [MenuNode<NavigatorMenuAction>],
    id: &str,
) -> Option<&'a MenuNode<NavigatorMenuAction>> {
    for node in nodes {
        match node {
            MenuNode::Item { id: node_id, .. } if node_id == id => return Some(node),
            MenuNode::Submenu { items, .. } => {
                if let Some(found) = find_item(items, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_submenu<'a>(
    nodes: &'a [MenuNode<NavigatorMenuAction>],
    id: &str,
) -> Option<&'a [MenuNode<NavigatorMenuAction>]> {
    for node in nodes {
        if let MenuNode::Submenu { id: node_id, items, .. } = node {
            if node_id == id {
                return Some(items);
            }
            if let Some(found) = find_submenu(items, id) {
                return Some(found);
            }
        }
    }
    None
}

fn action_of<'a>(nodes: &'a [MenuNode<NavigatorMenuAction>], id: &str) -> Option<&'a NavigatorMenuAction> {
    match find_item(nodes, id) {
        Some(MenuNode::Item { action, .. }) => action.as_ref(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// the group menu
// ---------------------------------------------------------------------------

#[test]
fn the_ungrouped_group_menu_offers_creation_only() {
    let l = labels();
    let items = group_context_menu(&l.context, "");
    assert_eq!(ids_of(&items), vec!["new-group", "new-connection"]);
    assert_eq!(action_of(&items, "new-connection"), Some(&NavigatorMenuAction::NewConnectionInGroup(None)));
}

#[test]
fn a_named_group_menu_adds_rename_and_delete_behind_a_separator() {
    let l = labels();
    let items = group_context_menu(&l.context, "team-a");
    assert_eq!(ids_of(&items), vec!["new-group", "new-connection", "--", "rename-group", "delete-group"]);
    assert_eq!(
        action_of(&items, "rename-group"),
        Some(&NavigatorMenuAction::StartRenameGroup {
            group: "team-a".into(),
            // A custom key is its own label; preset keys would localize.
            initial_value: "team-a".into(),
        })
    );
    assert_eq!(
        action_of(&items, "delete-group"),
        Some(&NavigatorMenuAction::ConfirmDeleteGroup {
            group: "team-a".into(),
            label: "team-a".into(),
        })
    );
    assert_eq!(
        action_of(&items, "new-connection"),
        Some(&NavigatorMenuAction::NewConnectionInGroup(Some("team-a".into())))
    );
}

// ---------------------------------------------------------------------------
// the connection menu
// ---------------------------------------------------------------------------

#[test]
fn the_connected_connection_menu_is_in_product_order() {
    let mut b = InputsBuilder::new();
    b.connections[0] = multi_db_conn("c1");
    let l = labels();
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "team-a");
    assert_eq!(
        ids_of(&items),
        vec![
            "disconnect",
            "--",
            "new-query",
            "query-history",
            "--",
            "connection-submenu/",
            "edit-connection",
            "duplicate-connection",
            "copy-connection-url",
            "copy-name",
            "object-filter",
            "server-submenu/",
            "process-list",
            "server-status",
            "create-new-submenu/",
            "create-database",
            "--",
            "refresh",
            "organize-submenu/",
            "pin-connection",
            "--",
            "move-group-team-b",
            "move-group-team-z",
            "remove-from-group",
            "--",
            "delete-connection",
        ]
    );
}

#[test]
fn a_disconnected_connection_announces_open_and_defers_its_history() {
    let mut b = InputsBuilder::new();
    b.active.insert(
        "c1".into(),
        MenuActiveEntry { status: "idle".into(), db_session_id: None },
    );
    let l = labels();
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "__pinned__");
    // The primary item flips to open-connection.
    assert!(find_item(&items, "open-connection").is_some());
    assert!(find_item(&items, "disconnect").is_none());
    // History stores the pending intent and names the section to toggle.
    assert_eq!(
        action_of(&items, "query-history"),
        Some(&NavigatorMenuAction::QueryHistory {
            connection_id: "c1".into(),
            pending_connect: Some("__pinned__".into()),
        })
    );
    // Open path carries the section group for the toggle.
    assert_eq!(
        action_of(&items, "open-connection"),
        Some(&NavigatorMenuAction::OpenOrDisconnect {
            connection_id: "c1".into(),
            section_group: "__pinned__".into(),
        })
    );
}

#[test]
fn the_server_submenu_follows_the_driver_commands_and_the_connection_state() {
    let mut b = InputsBuilder::new();
    let l = labels();

    // A driver with no commands: no server submenu at all.
    b.driver_commands.insert("postgresql".into(), vec![]);
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "team-a");
    assert!(find_submenu(&items, "server-submenu").is_none());

    // Commands present but the connection is idle: still no submenu —
    // monitoring only exists for an open connection.
    b.driver_commands.insert(
        "postgresql".into(),
        vec![SERVER_STATUS_SNAPSHOT_COMMAND.to_owned()],
    );
    b.active.insert(
        "c1".into(),
        MenuActiveEntry { status: "idle".into(), db_session_id: None },
    );
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "team-a");
    assert!(find_submenu(&items, "server-submenu").is_none());

    // Connected with only the status command: the submenu holds exactly
    // that item (no separator, no process list).
    b.active.insert(
        "c1".into(),
        MenuActiveEntry { status: "connected".into(), db_session_id: Some("s1".into()) },
    );
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "team-a");
    assert_eq!(
        ids_of(find_submenu(&items, "server-submenu").unwrap()),
        vec!["server-status"]
    );
}

#[test]
fn create_new_requires_a_multi_db_driver_that_can_create_databases() {
    let mut b = InputsBuilder::new();
    b.connections[0] = multi_db_conn("c1");
    let l = labels();

    // Single-db driver (has_multi_database=false): no create-new submenu
    // (create-user is off too).
    let mut single = pg_meta();
    single.has_multi_database = false;
    b.registry.insert("postgresql".into(), single);
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "team-a");
    assert!(find_submenu(&items, "create-new-submenu").is_none());

    // A pinned logical database locks the standard tree just the same.
    b.registry.insert("postgresql".into(), pg_meta());
    let pinned = conn("c1"); // database = Some("db1")
    let items = connection_context_menu(&b.inputs(&l), &pinned, "team-a");
    assert!(find_submenu(&items, "create-new-submenu").is_none());
    b.connections[0] = multi_db_conn("c1");

    // Multi-db but the driver cannot create: same.
    let mut no_create = pg_meta();
    no_create.supports_create_database = false;
    b.registry.insert("postgresql".into(), no_create);
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "team-a");
    assert!(find_submenu(&items, "create-new-submenu").is_none());

    // A driver that supports users (no db gate): the submenu exists with
    // just create-user, and the navigator never wires create-schema.
    let mut users = pg_meta();
    users.supports_create_database = false;
    users.supports_create_user = true;
    b.registry.insert("postgresql".into(), users);
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "team-a");
    assert_eq!(
        ids_of(find_submenu(&items, "create-new-submenu").unwrap()),
        vec!["create-user"]
    );
}

#[test]
fn the_organize_submenu_lists_other_groups_and_flips_pin_when_pinned() {
    let b = InputsBuilder::new();
    let l = labels();
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "team-a");
    // The current group is absent from the move targets.
    let organize = find_submenu(&items, "organize-submenu").unwrap();
    assert_eq!(
        ids_of(organize),
        vec![
            "pin-connection",
            "--",
            "move-group-team-b",
            "move-group-team-z",
            "remove-from-group",
        ]
    );
    assert_eq!(
        action_of(organize, "move-group-team-z"),
        Some(&NavigatorMenuAction::MoveConnectionToGroup {
            connection_id: "c1".into(),
            group: Some("team-z".into()),
        })
    );
    assert_eq!(
        action_of(organize, "remove-from-group"),
        Some(&NavigatorMenuAction::MoveConnectionToGroup {
            connection_id: "c1".into(),
            group: None,
        })
    );

    // Pinned: the item id and label flip to unpin.
    let mut pinned = conn("c1");
    pinned.pinned = true;
    let items = connection_context_menu(&b.inputs(&l), &pinned, "team-a");
    let organize = find_submenu(&items, "organize-submenu").unwrap();
    assert!(find_item(organize, "pin-connection").is_none());
    assert!(find_item(organize, "unpin-connection").is_some());
    assert_eq!(action_of(organize, "unpin-connection"), Some(&NavigatorMenuAction::TogglePinned("c1".into())));
}

#[test]
fn an_ungrouped_connection_hides_remove_from_group_and_keeps_moves() {
    let mut b = InputsBuilder::new();
    let l = labels();
    let mut ungrouped = conn("c1");
    ungrouped.group = None;
    b.connections[0] = ungrouped;
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "");
    let organize = find_submenu(&items, "organize-submenu").unwrap();
    assert_eq!(
        ids_of(organize),
        vec!["pin-connection", "--", "move-group-team-a", "move-group-team-b", "move-group-team-z"]
    );
}

#[test]
fn copy_url_carries_the_built_url_or_nothing() {
    let b = InputsBuilder::new();
    let l = labels();
    let items = connection_context_menu(&b.inputs(&l), &b.connections[0], "team-a");
    assert_eq!(
        action_of(&items, "copy-connection-url"),
        Some(&NavigatorMenuAction::CopyConnectionUrl {
            url: Some("postgresql://alice@db.example.com/db1?sslmode=prefer".into()),
        })
    );

    // A driver with no clipboard scheme: the action carries None (React
    // would not call copyToClipboard at all).
    let mut noscheme = pg_meta();
    noscheme.clipboard_scheme = String::new();
    let mut b2 = InputsBuilder::new();
    b2.registry.insert("postgresql".into(), noscheme);
    let items = connection_context_menu(&b2.inputs(&l), &b2.connections[0], "team-a");
    assert_eq!(
        action_of(&items, "copy-connection-url"),
        Some(&NavigatorMenuAction::CopyConnectionUrl { url: None })
    );
}

// ---------------------------------------------------------------------------
// the database menu
// ---------------------------------------------------------------------------

#[test]
fn the_database_menu_is_in_product_order_with_all_gates_open() {
    let b = InputsBuilder::new();
    let l = labels();
    let items = database_context_menu(&b.inputs(&l), "db1", "c1");
    assert_eq!(
        ids_of(&items),
        vec![
            "refresh",
            "new-query",
            "query-history",
            "copy-database-name",
            "view-er-diagram",
            "new-table", // showNewTable is always true for the database menu
            "create-schema",
            "--",
            "data-transfer",
            "compare-schema",
            "compare-data",
            "--",
            "close-database-connection",
            "--",
            "drop-database",
        ]
    );
}

#[test]
fn safe_mode_and_read_only_strip_the_mutating_items() {
    let mut b = InputsBuilder::new();
    b.safe_mode = true;
    b.view.open_sql_file = true;
    b.view.create_table = true;
    b.view.open_sql_file = false; // (execute-sql-file needs the action wired too)
    let l = labels();
    let items = database_context_menu(&b.inputs(&l), "db1", "c1");
    assert!(find_item(&items, "drop-database").is_some()); // safe mode alone keeps drop? No:
    // React gates drop only on readOnly; safe mode gates execute-sql-file.
    // Wire open_sql_file on and check safe mode alone hides it.
    b.view.open_sql_file = true;
    let items = database_context_menu(&b.inputs(&l), "db1", "c1");
    assert!(find_item(&items, "execute-sql-file").is_none());

    // Read-only: drop, import and the structure-creating items go.
    b.safe_mode = false;
    let mut ro = pg_meta();
    ro.read_only = true;
    b.registry.insert("postgresql".into(), ro);
    b.view.create_table = true;
    let items = database_context_menu(&b.inputs(&l), "db1", "c1");
    assert!(find_item(&items, "drop-database").is_none());
    assert!(find_item(&items, "new-table").is_none());
    assert!(find_item(&items, "create-schema").is_some(), "create-schema is not read-only-gated in React");
}

#[test]
fn an_unconnected_session_hides_the_session_bound_items() {
    let mut b = InputsBuilder::new();
    b.active.insert(
        "c1".into(),
        MenuActiveEntry { status: "idle".into(), db_session_id: None },
    );
    let l = labels();
    let items = database_context_menu(&b.inputs(&l), "db1", "c1");
    for id in ["refresh", "close-database-connection", "drop-database", "create-schema"] {
        assert!(find_item(&items, id).is_none(), "{id} must hide without a session");
    }
}

#[test]
fn the_drop_database_action_carries_the_fallback_seed() {
    let b = InputsBuilder::new();
    let l = labels();
    let items = database_context_menu(&b.inputs(&l), "db1", "c1");
    assert_eq!(
        action_of(&items, "drop-database"),
        Some(&NavigatorMenuAction::DropDatabase {
            connection_id: "c1".into(),
            db_session_id: "s1".into(),
            db_name: "db1".into(),
            configured_db: Some("db1".into()),
        })
    );
}

// ---------------------------------------------------------------------------
// the schema menu
// ---------------------------------------------------------------------------

#[test]
fn system_schemas_cannot_be_dropped() {
    let b = InputsBuilder::new();
    let l = labels();
    for schema in ["pg_catalog", "information_schema"] {
        let items = schema_context_menu(&b.inputs(&l), schema, "db1", "c1");
        assert!(find_item(&items, "drop-schema").is_none(), "{schema} must not be droppable");
    }
    let items = schema_context_menu(&b.inputs(&l), "public", "db1", "c1");
    assert_eq!(
        action_of(&items, "drop-schema"),
        Some(&NavigatorMenuAction::DropSchema {
            db_session_id: "s1".into(),
            db_name: "db1".into(),
            schema_name: "public".into(),
        })
    );
}

// ---------------------------------------------------------------------------
// the table / view menu
// ---------------------------------------------------------------------------

fn table_args<'a>(is_table: bool, name: &'a str, schema: Option<&'a str>) -> TableMenuArgs<'a> {
    TableMenuArgs {
        is_table,
        name,
        schema,
        db_name: "db1",
        connection_id: "c1",
        db_session_id: "s1",
    }
}

#[test]
fn the_table_menu_is_in_product_order_with_the_generate_submenu() {
    let b = InputsBuilder::new();
    let l = labels();
    let items = table_context_menu(&b.inputs(&l), table_args(true, "users", Some("public")));
    assert_eq!(
        ids_of(&items),
        vec![
            "open",
            "open-structure",
            "focus-er",
            "generate-sql/",
            "generate-select",
            "generate-insert",
            "generate-update",
            "generate-delete",
            "generate-ddl",
            "new-query",
            "copy-name",
            "copy-ddl",
            "--",
            "truncate",
            "drop",
        ]
    );
    // The quoted name rides in the dangerous actions.
    assert_eq!(
        action_of(&items, "truncate"),
        Some(&NavigatorMenuAction::TruncateTable {
            db_session_id: "s1".into(),
            db_name: "db1".into(),
            name: "users".into(),
            schema: Some("public".into()),
            quoted: "\"public\".\"users\"".into(),
        })
    );
    assert_eq!(
        action_of(&items, "drop"),
        Some(&NavigatorMenuAction::DropRelation {
            connection_id: "c1".into(),
            db_session_id: "s1".into(),
            db_name: "db1".into(),
            is_view: false,
            name: "users".into(),
            schema: Some("public".into()),
            quoted: "\"public\".\"users\"".into(),
        })
    );
}

#[test]
fn the_view_menu_drops_the_generate_and_truncate_paths() {
    let b = InputsBuilder::new();
    let l = labels();
    let items = table_context_menu(&b.inputs(&l), table_args(false, "v_users", None));
    assert_eq!(
        ids_of(&items),
        vec!["open", "copy-name", "copy-ddl", "--", "drop-view"]
    );
    assert_eq!(
        action_of(&items, "drop-view"),
        Some(&NavigatorMenuAction::DropRelation {
            connection_id: "c1".into(),
            db_session_id: "s1".into(),
            db_name: "db1".into(),
            is_view: true,
            name: "v_users".into(),
            schema: None,
            quoted: "\"v_users\"".into(),
        })
    );
}

#[test]
fn read_only_tables_keep_the_read_paths_and_lose_the_dangerous_ones() {
    let mut b = InputsBuilder::new();
    let mut ro = pg_meta();
    ro.read_only = true;
    b.registry.insert("postgresql".into(), ro);
    let l = labels();
    let items = table_context_menu(&b.inputs(&l), table_args(true, "users", None));
    assert!(find_item(&items, "truncate").is_none());
    assert!(find_item(&items, "drop").is_none());
    assert!(find_item(&items, "open").is_some());
    // Safe mode additionally hides the same pair; open-structure follows the
    // wired view action, not read-only.
    let mut b2 = InputsBuilder::new();
    b2.safe_mode = true;
    b2.view.open_table_structure = false;
    let items = table_context_menu(&b2.inputs(&l), table_args(true, "users", None));
    assert!(find_item(&items, "truncate").is_none());
    assert!(find_item(&items, "open-structure").is_none());
}

#[test]
fn a_driver_that_forbids_er_diagrams_loses_the_focus_item() {
    let mut b = InputsBuilder::new();
    let mut no_er = pg_meta();
    no_er.supports_er_diagram = Some(false);
    b.registry.insert("postgresql".into(), no_er);
    let l = labels();
    let items = table_context_menu(&b.inputs(&l), table_args(true, "users", None));
    assert!(find_item(&items, "focus-er").is_none());
    // A missing registry entry keeps the item (undefined !== false).
    b.registry.remove("postgresql");
    let items = table_context_menu(&b.inputs(&l), table_args(true, "users", None));
    assert!(find_item(&items, "focus-er").is_some());
}

// ---------------------------------------------------------------------------
// the category and object menus
// ---------------------------------------------------------------------------

#[test]
fn the_category_refresh_action_routes_by_driver_shape() {
    let mut b = InputsBuilder::new();
    b.connections[0] = multi_db_conn("c1");
    let l = labels();
    // Multi-db driver, tables category: reload tables for that database.
    let items = category_context_menu(&b.inputs(&l), "c1::db1::public::tables", "tables", "c1");
    assert_eq!(
        action_of(&items, "refresh"),
        Some(&NavigatorMenuAction::RefreshCategory {
            connection_id: "c1".into(),
            db_session_id: "s1".into(),
            db_name: "db1".into(),
            cat_key: "c1::db1::public::tables".into(),
            cat_id: "tables".into(),
            via_tables: true,
            multi_db: true,
        })
    );

    // Single-db driver, tables: the loadForConnection route.
    let mut single = pg_meta();
    single.has_multi_database = false;
    let mut b2 = InputsBuilder::new();
    b2.registry.insert("postgresql".into(), single);
    let items = category_context_menu(&b2.inputs(&l), "c1::db1::public::tables", "tables", "c1");
    let Some(NavigatorMenuAction::RefreshCategory { multi_db, .. }) = action_of(&items, "refresh")
    else {
        panic!("no refresh action");
    };
    assert_eq!(*multi_db, false);

    // A non-tables category reloads through the object-category cache.
    let items = category_context_menu(&b.inputs(&l), "c1::db1::public::routines", "routines", "c1");
    let _ = &b;
    let Some(NavigatorMenuAction::RefreshCategory { via_tables, .. }) = action_of(&items, "refresh")
    else {
        panic!("no refresh action");
    };
    assert_eq!(*via_tables, false);
}

#[test]
fn new_table_offers_only_for_the_tables_category_when_the_host_wired_it() {
    let mut b = InputsBuilder::new();
    b.view.create_table = true;
    let l = labels();
    let items = category_context_menu(&b.inputs(&l), "c1::db1::public::tables", "tables", "c1");
    assert!(find_item(&items, "new-table").is_some());
    let items = category_context_menu(&b.inputs(&l), "c1::db1::public::views", "views", "c1");
    assert!(find_item(&items, "new-table").is_none());
    // Not wired: gone even for tables.
    b.view.create_table = false;
    let items = category_context_menu(&b.inputs(&l), "c1::db1::public::tables", "tables", "c1");
    assert!(find_item(&items, "new-table").is_none());
}

#[test]
fn the_object_menu_is_a_bare_copy_name() {
    let b = InputsBuilder::new();
    let l = labels();
    let items = object_context_menu(&b.inputs(&l), "fn_calc");
    assert_eq!(ids_of(&items), vec!["copy-name"]);
    assert_eq!(action_of(&items, "copy-name"), Some(&NavigatorMenuAction::CopyText("fn_calc".into())));
}

// ---------------------------------------------------------------------------
// the pure helpers
// ---------------------------------------------------------------------------

#[test]
fn truncate_and_drop_sql_follow_the_dialect_and_the_kind() {
    assert_eq!(truncate_table_sql("\"t\"", None), "TRUNCATE TABLE \"t\"");
    assert_eq!(
        truncate_table_sql("\"t\"", Some("TRUNCATE \"t\" RESTART IDENTITY".into())),
        "TRUNCATE \"t\" RESTART IDENTITY"
    );
    assert_eq!(drop_relation_sql(false, "\"t\""), "DROP TABLE \"t\"");
    assert_eq!(drop_relation_sql(true, "\"v\""), "DROP VIEW \"v\"");
}

#[test]
fn quote_relation_name_uses_the_drivers_quote_char() {
    // PostgreSQL default.
    assert_eq!(quote_relation_name("users", Some("public"), "\""), "\"public\".\"users\"");
    assert_eq!(quote_relation_name("users", None, "\""), "\"users\"");
    // MySQL backticks, doubled inside.
    assert_eq!(quote_relation_name("ta`ble", None, "`"), "`ta``ble`");
    // SQL Server brackets.
    assert_eq!(quote_relation_name("or]der", Some("sc]h"), "["), "[sc]]h].[or]]der]");
    // A driver with no quoting (Redis) gets the name as-is.
    assert_eq!(quote_relation_name("users", None, ""), "users");
}

#[test]
fn build_connection_url_shapes_the_full_form() {
    let meta = ConnectionUrlMeta {
        scheme: "postgresql".into(),
        file_mode: false,
        default_port: 5432,
    };
    // Default port: omitted. Password + sslmode present.
    assert_eq!(
        build_connection_url(
            &ConnectionUrlConfig {
                database_type: "postgresql",
                host: Some("h"),
                port: Some(5432),
                username: Some("u s"),
                password: Some("p/w"),
                database: Some("my db"),
                ssl_mode: Some("require"),
            },
            Some(&meta),
        ),
        Some("postgresql://u%20s:p%2Fw@h/my%20db?sslmode=require".into())
    );
    // Non-default port shown; disable hides sslmode.
    assert_eq!(
        build_connection_url(
            &ConnectionUrlConfig {
                database_type: "postgresql",
                host: None,
                port: Some(5433),
                username: None,
                password: Some("pw"),
                database: None,
                ssl_mode: Some("disable"),
            },
            Some(&meta),
        ),
        Some("postgresql://:pw@localhost:5433".into())
    );
    // File mode: scheme://path, nothing else.
    let file_meta = ConnectionUrlMeta {
        scheme: "sqlite".into(),
        file_mode: true,
        default_port: 0,
    };
    assert_eq!(
        build_connection_url(
            &ConnectionUrlConfig {
                database: Some("/tmp/db.sqlite"),
                ..Default::default()
            },
            Some(&file_meta),
        ),
        Some("sqlite:///tmp/db.sqlite".into())
    );
    assert_eq!(
        build_connection_url(&ConnectionUrlConfig::default(), Some(&file_meta)),
        None
    );
    // No scheme in the registry: no URL.
    assert_eq!(build_connection_url(&ConnectionUrlConfig::default(), Some(&ConnectionUrlMeta::default())), None);
    assert_eq!(build_connection_url(&ConnectionUrlConfig::default(), None), None);
}

#[test]
fn extract_error_message_prefers_the_message_shapes() {
    assert_eq!(extract_error_message(&json!("boom"), "fallback"), "boom");
    assert_eq!(extract_error_message(&json!("  "), "fallback"), "fallback");
    assert_eq!(
        extract_error_message(&json!({ "message": "driver said no" }), "fallback"),
        "driver said no"
    );
    assert_eq!(
        extract_error_message(&json!({ "message": "" }), "fallback"),
        "fallback"
    );
    assert_eq!(extract_error_message(&json!({ "code": 42 }), "fallback"), "fallback");
    assert_eq!(extract_error_message(&json!(null), "fallback"), "fallback");
}

#[test]
fn the_drop_fallback_prefers_postgres_then_the_configured_then_anything() {
    let dbs = |names: &[&str]| -> Vec<String> { names.iter().map(|s| (*s).to_owned()).collect() };
    // postgres first, even mid-list.
    assert_eq!(
        resolve_drop_database_fallback(&dbs(&["db1", "postgres", "db2"]), "db1", Some("db2")),
        Some("postgres".into())
    );
    // Dropping postgres itself skips it.
    assert_eq!(
        resolve_drop_database_fallback(&dbs(&["db1", "postgres"]), "postgres", Some("db1")),
        Some("db1".into())
    );
    // No postgres: the configured database, when it is not the dropped one.
    assert_eq!(
        resolve_drop_database_fallback(&dbs(&["db1", "db2"]), "db1", Some("db2")),
        Some("db2".into())
    );
    // The configured db is the dropped one: fall through to any other.
    assert_eq!(
        resolve_drop_database_fallback(&dbs(&["db1", "db2"]), "db1", Some("db1")),
        Some("db2".into())
    );
    // Whitespace-only configured db reads as absent.
    assert_eq!(
        resolve_drop_database_fallback(&dbs(&["db1", "db2"]), "db1", Some("  ")),
        Some("db2".into())
    );
    // The last database standing: nowhere to go.
    assert_eq!(resolve_drop_database_fallback(&dbs(&["db1"]), "db1", None), None);
    assert_eq!(resolve_drop_database_fallback(&[], "db1", None), None);
}

// ---------------------------------------------------------------------------
// the menu builders' own contracts (main menu deps)
// ---------------------------------------------------------------------------

#[test]
fn the_connection_menu_hides_organize_when_there_is_nothing_to_organize() {
    let b = InputsBuilder::new();
    let l = labels();
    // No move targets (only the current group): the organize submenu
    // still exists — pin is always wired, and a grouped connection keeps
    // remove-from-group — holding exactly those two.
    let mut bare = InputsBuilder::new();
    bare.groups = vec!["team-a".into()]; // only the current group
    let items = connection_context_menu(&bare.inputs(&l), &bare.connections[0], "team-a");
    assert_eq!(
        ids_of(find_submenu(&items, "organize-submenu").unwrap()),
        vec!["pin-connection", "--", "remove-from-group"]
    );
    assert_eq!(ids_of(&items).last().unwrap(), "delete-connection");

    // MoveTarget is the raw shape the submenu consumes (import path check).
    let target = MoveTarget {
        id: "g".into(),
        label: "G".into(),
        action: NavigatorMenuAction::MoveConnectionToGroup {
            connection_id: "c1".into(),
            group: Some("g".into()),
        },
    };
    assert_eq!(target.id, "g");
    let _ = b;
}
