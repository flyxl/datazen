//! T578 audit for the pure navigator context-menu decision module.
//!
//! The assertions pin the item sets, their order, the separators, the exact
//! i18n keys, the enablement-by-omission rule and the emitted action for every
//! menu family. They are the Rust twin of the React
//! `mainWindowContextMenu.test.ts` / `schemaTreeContextMenu.test.ts` cases.

use crate::i18n::{has, t};
use crate::views::navigator::use_navigator_context_menus as menus;
use crate::views::navigator::use_navigator_context_menus::{
    category, connection, database, group, schema, table, EntryKind, MenuEntry, NavActionKind,
    ProductFeatures, SqlKind, ViewActionCaps,
};

fn all_view_actions() -> ViewActionCaps {
    ViewActionCaps {
        new_query: true,
        query_history: true,
        open_sql_file: true,
        create_table: true,
        open_server_status: true,
        open_process_list: true,
        open_er_diagram: true,
        open_table_structure: true,
    }
}

fn all_features() -> ProductFeatures {
    ProductFeatures {
        data_transfer: true,
        schema_diff: true,
        data_sync: true,
    }
}

fn ids(entries: &[MenuEntry]) -> Vec<String> {
    menus::entry_ids(entries)
}

fn find<'a>(entries: &'a [MenuEntry], id: &str) -> &'a MenuEntry {
    menus::find_entry(entries, id).unwrap_or_else(|| panic!("menu id {id} not found"))
}

fn all_keys(entries: &[MenuEntry], into: &mut Vec<&'static str>) {
    for entry in entries {
        if let Some(key) = entry.label_key {
            into.push(key);
        }
        all_keys(&entry.children, into);
    }
}

// ── group / blank ──────────────────────────────────────────────────────────

#[test]
fn group_menu_omits_rename_delete_for_ungrouped() {
    assert_eq!(
        ids(&group::build_group_menu(false)),
        [
            "new-group",
            "new-connection",
            "separator",
            "rename-group",
            "delete-group"
        ]
    );
    assert_eq!(
        ids(&group::build_group_menu(true)),
        ["new-group", "new-connection"]
    );
    assert_eq!(ids(&group::build_blank_menu()), ["new-group", "new-connection"]);
}

#[test]
fn group_items_carry_exact_keys_and_actions() {
    let grouped = group::build_group_menu(false);
    let new_group = find(&grouped, "new-group");
    assert_eq!(new_group.label_key, Some("common.newGroup"));
    assert_eq!(new_group.label, t("common.newGroup"));
    assert_eq!(new_group.action.as_ref().map(|a| a.kind), Some(NavActionKind::NewGroup));
    let rename = find(&grouped, "rename-group");
    assert_eq!(rename.label_key, Some("main.ctx.renameGroup"));
    assert_eq!(rename.action.as_ref().map(|a| a.kind), Some(NavActionKind::RenameGroup));
    assert_eq!(
        find(&grouped, "delete-group").action.as_ref().map(|a| a.kind),
        Some(NavActionKind::DeleteGroup)
    );
}

// ── connection ─────────────────────────────────────────────────────────────

fn connection_input(connected: bool) -> connection::ConnectionMenuInput {
    connection::ConnectionMenuInput {
        id: "conn-1".to_owned(),
        section_group: "Prod".to_owned(),
        group: Some("Prod".to_owned()),
        pinned: false,
        is_connected: connected,
        db_session_id: Some("session-1".to_owned()),
        move_targets: vec!["Stage".to_owned(), "Prod".to_owned()],
        driver: menus::DriverCaps {
            supports_backup: true,
            supports_create_database: true,
            supports_create_user: true,
            has_multi_database: true,
            database_field_type: "domain".to_owned(),
            ..menus::DriverCaps::default()
        },
        initial_database: None,
        has_server_status_command: true,
        has_process_list_command: true,
        view: all_view_actions(),
        features: all_features(),
    }
}

#[test]
fn connection_menu_order_and_submenus_match_react() {
    let items = connection::build_connection_menu(&connection_input(true));
    assert_eq!(
        ids(&items),
        [
            "disconnect",
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
    assert_eq!(
        ids(&find(&items, "connection-submenu").children),
        [
            "edit-connection",
            "duplicate-connection",
            "copy-connection-url",
            "copy-name",
            "object-filter"
        ]
    );
    assert_eq!(
        ids(&find(&items, "server-submenu").children),
        ["process-list", "server-status", "separator", "backup", "restore"]
    );
    // The hook never passes onCreateSchema, so the create submenu has no schema.
    assert_eq!(
        ids(&find(&items, "create-new-submenu").children),
        ["create-database", "create-user"]
    );
}

#[test]
fn connection_menu_toggles_open_disconnect_and_favorite() {
    let connected = connection::build_connection_menu(&connection_input(true));
    assert_eq!(connected[0].id, "disconnect");
    assert_eq!(connected[0].label_key, Some("main.ctx.disconnect"));
    assert_eq!(
        connected[0].action.as_ref().map(|a| a.kind),
        Some(NavActionKind::OpenOrDisconnect)
    );
    assert_eq!(
        connection::build_connection_menu(&connection_input(false))[0].id,
        "open-connection"
    );

    let mut pinned = connection_input(true);
    pinned.pinned = true;
    let items = connection::build_connection_menu(&pinned);
    assert_eq!(find(&items, "unpin-connection").label_key, Some("main.ctx.unpinConnection"));
    assert_eq!(
        find(&items, "unpin-connection").action.as_ref().map(|a| a.kind),
        Some(NavActionKind::ToggleFavorite)
    );
    assert!(menus::find_entry(&items, "pin-connection").is_none());
}

#[test]
fn connection_organize_holds_move_targets_and_remove_from_group() {
    let items = connection::build_connection_menu(&connection_input(false));
    let organize = find(&items, "organize-submenu");
    assert_eq!(organize.label_key, Some("main.ctx.organize"));
    assert_eq!(
        ids(&organize.children),
        ["pin-connection", "separator", "move-group-Stage", "remove-from-group"]
    );
    let move_group = find(&organize.children, "move-group-Stage");
    assert_eq!(move_group.label_key, None); // caller-formatted group label
    assert_eq!(
        move_group.action.as_ref().map(|a| a.kind),
        Some(NavActionKind::MoveToGroup)
    );
    assert_eq!(
        move_group.action.as_ref().map(|a| a.target.group.as_str()),
        Some("Stage")
    );
    assert!(menus::find_entry(&organize.children, "move-group-Prod").is_none());
}

#[test]
fn connection_caps_fold_connected_commands_and_multi_db() {
    let mut input = connection_input(false);
    let caps = connection::derive_caps(&input);
    assert!(!caps.supports_server_status);
    assert!(!caps.supports_process_list);
    assert!(caps.supports_backup);
    assert!(caps.supports_create_database); // domain driver is always multi-db
    assert!(caps.supports_create_user);

    input.is_connected = true;
    let caps = connection::derive_caps(&input);
    assert!(caps.supports_server_status && caps.supports_process_list);

    input.driver.has_multi_database = false;
    input.initial_database = Some("postgres".to_owned());
    assert!(!connection::derive_caps(&input).supports_create_database);
    input.driver.has_multi_database = true;
    input.driver.database_field_type = String::new();
    assert!(connection::is_multi_database(&input.driver, Some("")));
    assert!(!connection::is_multi_database(&input.driver, Some("postgres")));
}

#[test]
fn connection_menu_hides_unsupported_submenus() {
    let mut input = connection_input(true);
    input.has_server_status_command = false;
    input.has_process_list_command = false;
    input.driver.supports_backup = false;
    input.driver.supports_create_database = false;
    input.driver.supports_create_user = false;
    input.driver.has_multi_database = false;
    input.group = None;
    input.move_targets.clear();
    let items = connection::build_connection_menu(&input);
    assert!(menus::find_entry(&items, "server-submenu").is_none());
    assert!(menus::find_entry(&items, "create-new-submenu").is_none());
    // Pin is always supplied, so organize stays even with no move targets.
    assert_eq!(
        ids(&find(&items, "organize-submenu").children),
        ["pin-connection"]
    );
}

// ── database ───────────────────────────────────────────────────────────────

fn database_input() -> database::DatabaseMenuInput {
    database::DatabaseMenuInput {
        connection_id: "conn-1".to_owned(),
        database: "demo".to_owned(),
        db_session_id: Some("session-1".to_owned()),
        read_only: false,
        safe_mode: false,
        driver: menus::DriverCaps {
            supports_backup: true,
            supports_create_schema: true,
            ..menus::DriverCaps::default()
        },
        view: all_view_actions(),
        features: all_features(),
    }
}

#[test]
fn database_menu_full_order_and_separators() {
    let items = database::build_database_menu(&database_input());
    assert_eq!(
        ids(&items),
        [
            "refresh",
            "new-query",
            "query-history",
            "execute-sql-file",
            "copy-database-name",
            "view-er-diagram",
            "new-table",
            "create-schema",
            "separator",
            "data-transfer",
            "compare-schema",
            "compare-data",
            "separator",
            "backup",
            "restore",
            "separator",
            "close-database-connection",
            "separator",
            "drop-database"
        ]
    );
    assert_eq!(find(&items, "refresh").label_key, Some("connWin.refresh"));
    assert_eq!(find(&items, "create-schema").label_key, Some("common.createSchema"));
    assert_eq!(
        find(&items, "drop-database").action.as_ref().map(|a| a.kind),
        Some(NavActionKind::DropDatabase)
    );
    assert!(find(&items, "drop-database").danger);
}

#[test]
fn database_menu_gates_session_readonly_and_safe_mode() {
    let mut input = database_input();
    input.db_session_id = None;
    let items = database::build_database_menu(&input);
    assert!(menus::find_entry(&items, "refresh").is_none());
    assert!(menus::find_entry(&items, "close-database-connection").is_none());
    assert!(menus::find_entry(&items, "drop-database").is_none());

    let mut input = database_input();
    input.read_only = true;
    input.safe_mode = true;
    let items = database::build_database_menu(&input);
    assert!(menus::find_entry(&items, "execute-sql-file").is_none());
    assert!(menus::find_entry(&items, "new-table").is_none());
    assert!(menus::find_entry(&items, "drop-database").is_none());
}

#[test]
fn database_menu_feature_flags_drive_migration_tools() {
    let mut input = database_input();
    input.features = ProductFeatures::default();
    let items = database::build_database_menu(&input);
    assert!(menus::find_entry(&items, "data-transfer").is_none());
    assert!(menus::find_entry(&items, "compare-schema").is_none());
    assert!(menus::find_entry(&items, "compare-data").is_none());
    assert_eq!(database::migration_tool_items(all_features()).len(), 3);
}

// ── schema ─────────────────────────────────────────────────────────────────

fn schema_input(schema: &str) -> schema::SchemaMenuInput {
    schema::SchemaMenuInput {
        connection_id: "conn-1".to_owned(),
        database: "demo".to_owned(),
        schema: schema.to_owned(),
        db_session_id: Some("session-1".to_owned()),
        read_only: false,
        safe_mode: false,
        view: all_view_actions(),
        features: all_features(),
    }
}

#[test]
fn schema_menu_full_order() {
    let items = schema::build_schema_menu(&schema_input("public"));
    assert_eq!(
        ids(&items),
        [
            "refresh",
            "new-query",
            "query-history",
            "execute-sql-file",
            "copy-schema-name",
            "view-er-diagram",
            "new-table",
            "separator",
            "data-transfer",
            "compare-schema",
            "compare-data",
            "separator",
            "drop-schema"
        ]
    );
    assert_eq!(find(&items, "copy-schema-name").label_key, Some("common.copyName"));
    assert_eq!(find(&items, "copy-schema-name").label, t("common.copyName"));
}

#[test]
fn schema_menu_never_drops_system_schemas() {
    assert!(schema::can_drop_schema("public", true, false));
    assert!(!schema::can_drop_schema("pg_catalog", true, false));
    assert!(!schema::can_drop_schema("information_schema", true, false));
    assert!(!schema::can_drop_schema("public", false, false));
    assert!(!schema::can_drop_schema("public", true, true));

    let items = schema::build_schema_menu(&schema_input("pg_catalog"));
    assert!(menus::find_entry(&items, "drop-schema").is_none());
    let items = schema::build_schema_menu(&schema_input("public"));
    assert!(menus::find_entry(&items, "drop-schema").is_some());
}

// ── table / view ───────────────────────────────────────────────────────────

fn table_input(kind: table::TableKind) -> table::TableMenuInput {
    table::TableMenuInput {
        kind,
        name: "users".to_owned(),
        schema: Some("public".to_owned()),
        database: "demo".to_owned(),
        connection_id: "conn-1".to_owned(),
        db_session_id: "session-1".to_owned(),
        database_type: "postgresql".to_owned(),
        read_only: false,
        safe_mode: false,
        supports_er_diagram: true,
        view: all_view_actions(),
    }
}

#[test]
fn table_menu_full_order_and_generate_submenu() {
    let items = table::build_table_menu(&table_input(table::TableKind::Table));
    assert_eq!(
        ids(&items),
        [
            "open",
            "open-structure",
            "focus-er",
            "generate-sql",
            "new-query",
            "copy-name",
            "copy-ddl",
            "separator",
            "truncate",
            "drop"
        ]
    );
    let generate = find(&items, "generate-sql");
    assert_eq!(generate.label_key, Some("schemaTree.generateSql"));
    assert_eq!(
        ids(&generate.children),
        [
            "generate-select",
            "generate-insert",
            "generate-update",
            "generate-delete",
            "generate-ddl"
        ]
    );
    let kinds: Vec<SqlKind> = generate
        .children
        .iter()
        .map(|entry| entry.action.as_ref().map(|a| a.target.sql_kind).unwrap_or_default())
        .collect();
    assert_eq!(
        kinds,
        [SqlKind::Select, SqlKind::Insert, SqlKind::Update, SqlKind::Delete, SqlKind::Ddl]
    );
    assert_eq!(find(&generate.children, "generate-ddl").label_key, Some("schemaTree.generateDdl"));
}

#[test]
fn table_menu_read_only_drops_mutating_items() {
    let mut input = table_input(table::TableKind::Table);
    input.read_only = true;
    input.safe_mode = true;
    let items = table::build_table_menu(&input);
    assert!(menus::find_entry(&items, "truncate").is_none());
    assert!(menus::find_entry(&items, "drop").is_none());
    assert_eq!(ids(&items), ["open", "open-structure", "focus-er", "generate-sql", "new-query", "copy-name", "copy-ddl"]);
}

#[test]
fn view_menu_is_short_and_has_drop_view() {
    let mut input = table_input(table::TableKind::View);
    input.view.open_table_structure = false;
    let items = table::build_table_menu(&input);
    assert_eq!(ids(&items), ["open", "copy-name", "copy-ddl", "separator", "drop-view"]);
    assert_eq!(find(&items, "drop-view").label_key, Some("schemaTree.dropView"));
    assert!(find(&items, "drop-view").action.as_ref().map(|a| a.target.is_view) == Some(true));

    input.read_only = true;
    let items = table::build_table_menu(&input);
    assert_eq!(ids(&items), ["open", "copy-name", "copy-ddl"]);
}

#[test]
fn table_open_and_copy_actions_carry_their_target() {
    let items = table::build_table_menu(&table_input(table::TableKind::Table));
    let open = find(&items, "open");
    assert_eq!(open.label_key, Some("schemaTree.openTable"));
    assert_eq!(open.action.as_ref().map(|a| a.kind), Some(NavActionKind::OpenTable));
    assert_eq!(open.action.as_ref().map(|a| a.target.name.as_str()), Some("users"));
    assert_eq!(open.action.as_ref().map(|a| a.target.schema.as_str()), Some("public"));
    let copy = find(&items, "copy-ddl");
    assert_eq!(copy.label_key, Some("common.copyDdl"));
    assert_eq!(copy.action.as_ref().map(|a| a.kind), Some(NavActionKind::CopyDdl));
}

// ── category / object ──────────────────────────────────────────────────────

#[test]
fn category_menu_new_table_only_for_writable_tables() {
    let input = category::CategoryMenuInput {
        cat_id: "tables".to_owned(),
        read_only: false,
        view: all_view_actions(),
    };
    assert_eq!(ids(&category::build_category_menu(&input)), ["refresh", "new-table"]);

    let input = category::CategoryMenuInput {
        cat_id: "views".to_owned(),
        read_only: false,
        view: all_view_actions(),
    };
    assert_eq!(ids(&category::build_category_menu(&input)), ["refresh"]);

    let input = category::CategoryMenuInput {
        cat_id: "tables".to_owned(),
        read_only: true,
        view: all_view_actions(),
    };
    assert_eq!(ids(&category::build_category_menu(&input)), ["refresh"]);
}

#[test]
fn object_menu_is_a_single_copy_name_carrying_the_name() {
    let items = category::build_object_menu("created_at");
    assert_eq!(ids(&items), ["copy-name"]);
    assert_eq!(items[0].label_key, Some("common.copyName"));
    assert_eq!(items[0].action.as_ref().map(|a| a.kind), Some(NavActionKind::CopyName));
    assert_eq!(items[0].action.as_ref().map(|a| a.target.name.as_str()), Some("created_at"));
}

#[test]
fn server_submenu_separates_only_when_both_groups_present() {
    let mut input = connection_input(true);
    input.driver.supports_backup = false; // monitoring only
    let items = connection::build_connection_menu(&input);
    assert_eq!(
        ids(&find(&items, "server-submenu").children),
        ["process-list", "server-status"]
    );

    let mut input = connection_input(true);
    input.has_server_status_command = false;
    input.has_process_list_command = false;
    input.driver.supports_backup = true; // backup/restore only
    let items = connection::build_connection_menu(&input);
    assert_eq!(ids(&find(&items, "server-submenu").children), ["backup", "restore"]);
}

#[test]
fn category_new_table_needs_create_table_action() {
    let mut view = all_view_actions();
    view.create_table = false;
    let input = category::CategoryMenuInput {
        cat_id: "tables".to_owned(),
        read_only: false,
        view,
    };
    assert_eq!(ids(&category::build_category_menu(&input)), ["refresh"]);
}

#[test]
fn database_create_schema_needs_driver_support() {
    let mut input = database_input();
    input.driver.supports_create_schema = false;
    let items = database::build_database_menu(&input);
    assert!(menus::find_entry(&items, "create-schema").is_none());
}

#[test]
fn view_menu_never_offers_er_or_generate_sql() {
    let mut input = table_input(table::TableKind::View);
    input.supports_er_diagram = true;
    input.view.open_table_structure = true;
    let items = table::build_table_menu(&input);
    assert!(menus::find_entry(&items, "focus-er").is_none());
    assert!(menus::find_entry(&items, "generate-sql").is_none());
    assert!(menus::find_entry(&items, "new-query").is_none());
}

// ── cross-cutting invariants ───────────────────────────────────────────────

#[test]
fn every_emitted_item_is_enabled_and_keyed_or_dynamic() {
    let mut all: Vec<MenuEntry> = Vec::new();
    all.extend(group::build_group_menu(false));
    all.extend(connection::build_connection_menu(&connection_input(true)));
    all.extend(database::build_database_menu(&database_input()));
    all.extend(schema::build_schema_menu(&schema_input("public")));
    all.extend(table::build_table_menu(&table_input(table::TableKind::Table)));
    all.extend(category::build_category_menu(&category::CategoryMenuInput {
        cat_id: "tables".to_owned(),
        read_only: false,
        view: all_view_actions(),
    }));
    all.extend(category::build_object_menu("id"));

    fn check(entries: &[MenuEntry]) {
        for entry in entries {
            assert!(entry.enabled, "{} is emitted disabled", entry.id);
            if entry.kind == EntryKind::Item {
                assert!(
                    entry.action.is_some(),
                    "item {} has no emitted action",
                    entry.id
                );
            }
            if entry.kind == EntryKind::Separator {
                assert!(entry.action.is_none());
            }
            check(&entry.children);
        }
    }
    check(&all);
}

#[test]
fn every_i18n_key_used_by_the_menus_exists() {
    let mut keys: Vec<&'static str> = Vec::new();
    all_keys(&group::build_group_menu(false), &mut keys);
    all_keys(&group::build_blank_menu(), &mut keys);
    all_keys(&connection::build_connection_menu(&connection_input(true)), &mut keys);
    all_keys(&database::build_database_menu(&database_input()), &mut keys);
    all_keys(&schema::build_schema_menu(&schema_input("public")), &mut keys);
    all_keys(&table::build_table_menu(&table_input(table::TableKind::Table)), &mut keys);
    all_keys(&table::build_table_menu(&table_input(table::TableKind::View)), &mut keys);
    all_keys(
        &category::build_category_menu(&category::CategoryMenuInput {
            cat_id: "tables".to_owned(),
            read_only: false,
            view: all_view_actions(),
        }),
        &mut keys,
    );
    all_keys(&category::build_object_menu("x"), &mut keys);

    assert!(keys.len() > 30, "too few keys audited: {}", keys.len());
    for key in keys {
        assert!(has(key), "menu asks for missing i18n key {key}");
        assert!(!t(key).is_empty(), "menu key {key} resolves to empty text");
    }
    // The empty-label focus-er entry is dynamic, never a locale key.
    assert_eq!(find(&table::build_table_menu(&table_input(table::TableKind::Table)), "focus-er").label_key, None);
}
