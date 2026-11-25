//! React `windows/connection/navigator/useNavigatorContextMenus.ts` — the
//! navigator's seven context menus (T578).
//!
//! Every handler is a pure function from a snapshot (the deps struct React
//! receives, plus the label tables and the driver registry read set) to the
//! menu it shows: items in product order, each gated exactly where React
//! gates it (connected, pinned, safe mode, read-only, product features,
//! driver capabilities, which view actions the host wired). Selecting an
//! item dispatches a [`NavigatorMenuAction`] — one enum for the whole file,
//! like T470's `RowAction` — whose payload carries every decision *this
//! file* makes (the quoted relation name, the refresh route, the move
//! target, the URL to copy). The async bodies behind the actions (confirm
//! dialogs, driver IPC, DDL fallback chains, tab cleanup) are the host's to
//! run; each variant's doc spells the sequence React spells, and the pure
//! pieces are helpers here the host reuses.
//!
//! Split per the shard's split mark: `deps/` holds verbatim private ports of
//! the menu builders this file consumes (shard 2's T555/T599/T567/T340, plus
//! the utils additions in `build_flat_rows::deps`, removal stage T582).

pub mod deps;

use std::collections::HashMap;

use deps::connection_url::{build_connection_url, ConnectionUrlConfig, ConnectionUrlMeta};
use deps::main_menu::{
    build_main_connection_context_menu_items, build_main_group_context_menu_items,
    ConnectionMenuSlots, MainWindowContextMenuLabels, MoveTarget,
};
use deps::native_menu::MenuNode;
use deps::schema_tree_menu::{
    build_schema_tree_context_menu_items, BuildSchemaTreeMenuArgs, ProductFeatures,
    SchemaTreeContextMenuLabels, SchemaTreeMenuSlots, SchemaTreeNodeKind,
};

use crate::views::navigator::build_flat_rows::deps::groups::format_group_label;
use crate::views::navigator::utils::{
    quote_relation_name, should_use_multi_database_tree,
};

/// React `SERVER_STATUS_SNAPSHOT_COMMAND` / `LIST_PROCESSES_COMMAND`
/// (`lib/driverCommandIds.ts`).
pub const SERVER_STATUS_SNAPSHOT_COMMAND: &str = "server_status_snapshot";
pub const LIST_PROCESSES_COMMAND: &str = "list_processes";

// ---------------------------------------------------------------------------
// The snapshot the handlers read (React `NavigatorContextMenuDeps`, as data)
// ---------------------------------------------------------------------------

/// The read set the menus have off a connection config.
#[derive(Clone, Debug, Default)]
pub struct MenuConnectionSnapshot {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub database: Option<String>,
    pub group: Option<String>,
    pub pinned: bool,
    pub read_only: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub ssl_mode: Option<String>,
}

/// React `ConnectionEntry`, read set.
#[derive(Clone, Debug, Default)]
pub struct MenuActiveEntry {
    pub status: String,
    pub db_session_id: Option<String>,
}

/// The driver meta read set the menus have (off `DB_REGISTRY`).
#[derive(Clone, Debug, Default)]
pub struct MenuDriverMeta {
    pub has_multi_database: bool,
    /// `'name' | 'path' | 'index' | 'domain'`.
    pub database_field_type: String,
    /// `escapeIdent`'s quote char; empty reads as the `"` default.
    pub quote_char: String,
    pub supports_create_database: bool,
    pub supports_create_user: bool,
    pub supports_create_schema: bool,
    pub supports_backup: bool,
    /// `supportsErDiagram !== false`: `None` (or `Some(true)`) keeps the ER
    /// focus item, `Some(false)` hides it.
    pub supports_er_diagram: Option<bool>,
    pub read_only: bool,
    /// `clipboardSchemes?.[0]`; empty means the Copy URL item hides.
    pub clipboard_scheme: String,
    pub connection_mode_file: bool,
    pub default_port: u16,
}

impl MenuDriverMeta {
    fn url_meta(&self) -> ConnectionUrlMeta {
        ConnectionUrlMeta {
            scheme: self.clipboard_scheme.clone(),
            file_mode: self.connection_mode_file,
            default_port: self.default_port,
        }
    }

}

/// Which `viewActions` the host wired: a menu item exists only when its
/// action does, exactly as React's optional handlers read.
#[derive(Clone, Debug, Default)]
pub struct ViewActionsAvailable {
    pub new_query: bool,
    pub open_sql_file: bool,
    pub create_table: bool,
    pub open_create_database: bool,
    pub open_create_schema: bool,
    pub open_create_user: bool,
    pub open_er_diagram: bool,
    pub open_table_structure: bool,
    pub open_query_history: bool,
    pub open_server_status: bool,
    pub open_process_list: bool,
}

/// The label tables (React's two `useMemo`s over `t()`); the host builds
/// them from its i18n, tests build them literally.
#[derive(Clone, Debug, Default)]
pub struct MenuLabels {
    pub context: MainWindowContextMenuLabels,
    pub schema: SchemaTreeContextMenuLabels,
}

/// Everything the seven handlers read, in one snapshot.
pub struct NavigatorMenuInputs<'a> {
    pub labels: &'a MenuLabels,
    pub safe_mode: bool,
    pub features: ProductFeatures,
    pub groups: &'a [String],
    pub connections: &'a [MenuConnectionSnapshot],
    pub active_connections: &'a HashMap<String, MenuActiveEntry>,
    pub view: &'a ViewActionsAvailable,
    /// Caller-supplied registry (`DB_REGISTRY`), the builder's contract.
    pub registry: &'a HashMap<String, MenuDriverMeta>,
    /// Per-database-type driver command ids (`getDriverCommands`), for the
    /// `hasCommand` gates.
    pub driver_commands: &'a HashMap<String, Vec<String>>,
}

impl NavigatorMenuInputs<'_> {
    fn conn(&self, id: &str) -> Option<&MenuConnectionSnapshot> {
        self.connections.iter().find(|c| c.id == id)
    }

    fn meta(&self, database_type: &str) -> MenuDriverMeta {
        self.registry.get(database_type).cloned().unwrap_or_default()
    }

    fn entry(&self, id: &str) -> MenuActiveEntry {
        self.active_connections.get(id).cloned().unwrap_or_default()
    }

    fn is_connected(&self, id: &str) -> bool {
        self.entry(id).status == "connected"
    }

    /// React `hasCommand(commands, id)` over the injected command list.
    fn has_command(&self, database_type: &str, command_id: &str) -> bool {
        self.driver_commands
            .get(database_type)
            .is_some_and(|defs| defs.iter().any(|id| id == command_id))
    }

    /// The read-only verdict the db/schema/table menus share: the connection
    /// flag or the driver's.
    fn read_only(&self, conn: Option<&MenuConnectionSnapshot>, meta: &MenuDriverMeta) -> bool {
        conn.is_some_and(|c| c.read_only) || meta.read_only
    }
}

// ---------------------------------------------------------------------------
// The actions
// ---------------------------------------------------------------------------

/// Every intent a navigator menu item can carry, one enum for the whole
/// file. The host executes each; the doc on each variant is the React body
/// it must reproduce.
#[derive(Clone, Debug, PartialEq)]
pub enum NavigatorMenuAction {
    // ── Group menu ──────────────────────────────────────────────
    /// Reset the new-group dialog and open it.
    NewGroupDialog,
    /// `onNewConnection(group || undefined)`.
    NewConnectionInGroup(Option<String>),
    /// Enter rename mode with the group's display label as the seed.
    StartRenameGroup { group: String, initial_value: String },
    /// Confirm (`kind: warning`, message `main.confirmDeleteGroup` with the
    /// display label), then `deleteGroup(group)` when accepted.
    ConfirmDeleteGroup { group: String, label: String },

    // ── Connection menu ─────────────────────────────────────────
    /// Connected → `onDisconnect(id)`; else select, toggle, `connect(conn)`.
    OpenOrDisconnect { connection_id: String, section_group: String },
    CopyText(String),
    /// `buildConnectionUrl(conn)` then copy; the URL is resolved at menu
    /// build time from the same snapshot, so `None` means "no URL" and the
    /// host copies nothing (React's `if (url)`).
    CopyConnectionUrl { url: Option<String> },
    NewQuery {
        connection_id: String,
        sql: Option<String>,
        database: Option<String>,
        schema: Option<String>,
    },
    /// Query history. `pending_connect` is `Some(section_group)` when the
    /// connection is not open yet: store the pending intent
    /// (`setPendingQueryHistory`), toggle and connect — the query panel
    /// consumes it once the connection context mounts. `None` opens
    /// history directly.
    QueryHistory { connection_id: String, pending_connect: Option<String> },
    OpenCreateDatabase { connection_id: String },
    OpenCreateSchema {
        connection_id: String,
        db_name: String,
        db_session_id: String,
    },
    OpenCreateUser { connection_id: String },
    /// Open server status, resolving the connection's open target.
    OpenServerStatus { connection_id: String },
    OpenProcessList { connection_id: String },
    TogglePinned(String),
    OpenObjectFilter(String),
    /// Open the backup window in backup or restore mode.
    OpenBackupWindow { restore: bool, connection_id: String, database: Option<String> },
    /// `refreshConnection(id)` — the item shows unconditionally; the host
    /// no-ops when the entry has no session (React checks at select time).
    RefreshConnection(String),
    EditConnection(String),
    DuplicateConnection(String),
    MoveConnectionToGroup { connection_id: String, group: Option<String> },
    DeleteConnection(String),

    // ── Database menu ───────────────────────────────────────────
    /// `closeDatabase(session, db)` → close the database's panels →
    /// `clearDbLocalCache`; a failure shows the extracted error
    /// (`schemaTree.closeDatabaseConnectionFailed`).
    CloseDatabase { connection_id: String, db_session_id: String, db_name: String },
    /// Multi-db → `reloadDbTables`; single-db → `loadForConnection` with
    /// `activate: false` (must not steal the active session) + refresh open
    /// databases. Then reload the database's expanded object categories.
    RefreshDatabase { connection_id: String, db_name: String },
    /// Pin the clicked database (`setCurrentDatabase`) — a contextmenu fires
    /// without a preceding click, so the pin is part of the action.
    SetCurrentDatabase { db_name: String, db_session_id: String },
    CopyDatabaseName(String),
    /// Pin the database, then open/focus the ER diagram on it.
    ViewErDiagram { connection_id: String, db_name: String, focus_table: Option<String> },
    ExecuteSqlFile { connection_id: String },
    NewTable { connection_id: String },
    /// Confirm → `drop_database` command (the atomic step; only its failure
    /// is a "drop failed") → close the database's panels → repoint the
    /// session to the fallback (`resolveDropDatabaseFallback` on the live
    /// schema state: cached tables via `setLoadedTables`, else
    /// `setCurrentDatabase`) → `clearDbLocalCache` →
    /// `loadForConnection(skipLoadTables)`. Refresh errors after a
    /// successful drop are swallowed.
    DropDatabase {
        connection_id: String,
        db_session_id: String,
        db_name: String,
        configured_db: Option<String>,
    },
    /// The three window-opening items, gated by product features.
    OpenDataTransferWindow,
    OpenSchemaDiffWindow,
    OpenDataSyncWindow,

    // ── Schema menu ─────────────────────────────────────────────
    /// `refreshSchema` (tables for the database, then the schema's expanded
    /// object categories).
    RefreshSchema { connection_id: String, db_name: String, schema_name: String },
    /// Confirm → `drop_schema` cascade → `refreshDatabase`. Emitted only
    /// for non-system schemas (`pg_*`, `information_schema`).
    DropSchema {
        db_session_id: String,
        db_name: String,
        schema_name: String,
    },

    // ── Table / view menu ───────────────────────────────────────
    /// Select, `activateDatabase`, then `onSelectTable`.
    OpenTable {
        connection_id: String,
        db_session_id: String,
        db_name: String,
        name: String,
        schema: Option<String>,
    },
    /// `fetchTableSchemaForSqlGeneration` → `generateTableSqlWithFallbacks`
    /// → `newQuery(sql, {database, schema})` (shard 2's T631 libs).
    GenerateTableSql {
        kind: &'static str,
        connection_id: String,
        db_session_id: String,
        db_name: String,
        name: String,
        schema: Option<String>,
        database_type: String,
    },
    /// `getObjectDdl`, falling back to the dialect's table-DDL query →
    /// `newQuery(ddl || "/* No DDL found … */", …)`.
    GenerateDdl {
        connection_id: String,
        db_session_id: String,
        db_name: String,
        name: String,
        schema: Option<String>,
        database_type: String,
    },
    CopyDdl {
        db_session_id: String,
        db_name: String,
        name: String,
        schema: Option<String>,
        database_type: String,
        is_view: bool,
    },
    FocusEr { connection_id: String, name: String, db_name: String },
    /// `viewActions.openTableStructure(name)` — the structure editor.
    OpenTableStructure { name: String },
    /// Tables: open a new query seeded with the table's select template
    /// (`buildQueryOpenContext`, shard 2's T631). Views: a plain new query
    /// for the database.
    NewQueryForTable {
        is_table: bool,
        connection_id: String,
        db_session_id: String,
        db_name: String,
        name: String,
        schema: Option<String>,
        database_type: String,
    },
    /// Confirm → the dialect's truncate SQL, or `TRUNCATE TABLE <quoted>` →
    /// `executeQuery`; failures show the extracted error.
    TruncateTable {
        db_session_id: String,
        db_name: String,
        name: String,
        schema: Option<String>,
        quoted: String,
    },
    /// Confirm → `DROP TABLE/VIEW <quoted>` → close the relation's panels →
    /// invalidate the schema cache, drop the local row, `reloadDbTables`.
    DropRelation {
        connection_id: String,
        db_session_id: String,
        db_name: String,
        is_view: bool,
        name: String,
        schema: Option<String>,
        quoted: String,
    },

    // ── Category menu ───────────────────────────────────────────
    /// The refresh route the handler decided: tables/views on a multi-db
    /// driver reload through `reloadDbTables`; on a single-db driver through
    /// `loadForConnection(preferredDatabase)`; other categories through
    /// `reloadDbObjectCategory`.
    RefreshCategory {
        connection_id: String,
        db_session_id: String,
        db_name: String,
        cat_key: String,
        cat_id: String,
        via_tables: bool,
        multi_db: bool,
    },
    NewTableForCategory { connection_id: String },
}

// ---------------------------------------------------------------------------
// The pure helpers this file owns
// ---------------------------------------------------------------------------

/// React `handleTableContextMenu`'s truncate SQL: the dialect's
/// `getTruncateTableSql(quoted)` when it has one, else the plain form.
pub fn truncate_table_sql(quoted: &str, dialect_sql: Option<String>) -> String {
    dialect_sql.unwrap_or_else(|| format!("TRUNCATE TABLE {quoted}"))
}

/// React's drop SQL: `DROP VIEW` for views, `DROP TABLE` otherwise.
pub fn drop_relation_sql(is_view: bool, quoted: &str) -> String {
    if is_view {
        format!("DROP VIEW {quoted}")
    } else {
        format!("DROP TABLE {quoted}")
    }
}

/// The default quote char when the registry has no entry for a driver
/// (React `escapeIdent`'s default).
fn quote_char_of(meta: &MenuDriverMeta) -> &str {
    if meta.quote_char.is_empty() {
        "\""
    } else {
        &meta.quote_char
    }
}

// ---------------------------------------------------------------------------
// The seven handlers
// ---------------------------------------------------------------------------

/// React `handleGroupContextMenu`.
pub fn group_context_menu(
    labels: &MainWindowContextMenuLabels,
    group_name: &str,
) -> Vec<MenuNode<NavigatorMenuAction>> {
    let is_ungrouped = group_name.is_empty();
    let mut items = vec![
        MenuNode::item("new-group", &labels.new_group, NavigatorMenuAction::NewGroupDialog),
        MenuNode::item(
            "new-connection",
            &labels.new_connection,
            NavigatorMenuAction::NewConnectionInGroup(if is_ungrouped {
                None
            } else {
                Some(group_name.to_owned())
            }),
        ),
    ];
    if !is_ungrouped {
        items.push(MenuNode::separator());
        items.push(MenuNode::item(
            "rename-group",
            &labels.rename_group,
            NavigatorMenuAction::StartRenameGroup {
                group: group_name.to_owned(),
                initial_value: format_group_label(group_name),
            },
        ));
        items.push(MenuNode::item(
            "delete-group",
            &labels.delete_group,
            NavigatorMenuAction::ConfirmDeleteGroup {
                group: group_name.to_owned(),
                label: format_group_label(group_name),
            },
        ));
    }
    items
}

/// React `handleConnectionContextMenu`.
pub fn connection_context_menu(
    inputs: &NavigatorMenuInputs<'_>,
    conn: &MenuConnectionSnapshot,
    section_group: &str,
) -> Vec<MenuNode<NavigatorMenuAction>> {
    let is_connected = inputs.is_connected(&conn.id);
    let db_meta = inputs.meta(&conn.database_type);
    let is_multi_db =
        should_use_multi_database_tree(
            db_meta.has_multi_database,
            &db_meta.database_field_type,
            conn.database.as_deref(),
        );
    let move_targets: Vec<MoveTarget<NavigatorMenuAction>> = inputs
        .groups
        .iter()
        .filter(|g| conn.group.as_deref() != Some(g.as_str()))
        .map(|g| MoveTarget {
            id: g.clone(),
            label: format_group_label(g),
            action: NavigatorMenuAction::MoveConnectionToGroup {
                connection_id: conn.id.clone(),
                group: Some(g.clone()),
            },
        })
        .collect();

    let supports_server_status =
        is_connected && inputs.has_command(&conn.database_type, SERVER_STATUS_SNAPSHOT_COMMAND);
    let supports_process_list =
        is_connected && inputs.has_command(&conn.database_type, LIST_PROCESSES_COMMAND);

    build_main_connection_context_menu_items(
        &inputs.labels.context,
        is_connected,
        conn.group.is_some(),
        conn.pinned,
        ConnectionMenuSlots {
            on_open_or_disconnect: NavigatorMenuAction::OpenOrDisconnect {
                connection_id: conn.id.clone(),
                section_group: section_group.to_owned(),
            },
            on_copy_name: NavigatorMenuAction::CopyText(conn.name.clone()),
            on_copy_url: NavigatorMenuAction::CopyConnectionUrl {
                url: build_connection_url(
                    &ConnectionUrlConfig {
                        database_type: &conn.database_type,
                        host: conn.host.as_deref(),
                        port: conn.port,
                        username: conn.username.as_deref(),
                        password: conn.password.as_deref(),
                        database: conn.database.as_deref(),
                        ssl_mode: conn.ssl_mode.as_deref(),
                    },
                    Some(&db_meta.url_meta()),
                ),
            },
            on_new_query: NavigatorMenuAction::NewQuery {
                connection_id: conn.id.clone(),
                sql: None,
                database: None,
                schema: None,
            },
            on_query_history: Some(NavigatorMenuAction::QueryHistory {
                connection_id: conn.id.clone(),
                pending_connect: if is_connected {
                    None
                } else {
                    Some(section_group.to_owned())
                },
            }),
            on_create_database: (db_meta.supports_create_database && is_multi_db).then(|| {
                NavigatorMenuAction::OpenCreateDatabase { connection_id: conn.id.clone() }
            }),
            // The navigator never wires create-schema on the connection
            // menu (React: no `onCreateSchema` passed).
            on_create_schema: None,
            on_create_user: db_meta.supports_create_user.then(|| {
                NavigatorMenuAction::OpenCreateUser { connection_id: conn.id.clone() }
            }),
            on_pin: Some(NavigatorMenuAction::TogglePinned(conn.id.clone())),
            on_object_filter: Some(NavigatorMenuAction::OpenObjectFilter(conn.id.clone())),
            on_process_list: supports_process_list.then(|| {
                NavigatorMenuAction::OpenProcessList { connection_id: conn.id.clone() }
            }),
            on_server_status: supports_server_status.then(|| {
                NavigatorMenuAction::OpenServerStatus { connection_id: conn.id.clone() }
            }),
            on_backup: db_meta.supports_backup.then(|| NavigatorMenuAction::OpenBackupWindow {
                restore: false,
                connection_id: conn.id.clone(),
                database: conn.database.clone(),
            }),
            on_restore: db_meta.supports_backup.then(|| NavigatorMenuAction::OpenBackupWindow {
                restore: true,
                connection_id: conn.id.clone(),
                database: conn.database.clone(),
            }),
            on_refresh: NavigatorMenuAction::RefreshConnection(conn.id.clone()),
            on_edit: NavigatorMenuAction::EditConnection(conn.id.clone()),
            on_duplicate: NavigatorMenuAction::DuplicateConnection(conn.id.clone()),
            move_targets,
            on_remove_from_group: NavigatorMenuAction::MoveConnectionToGroup {
                connection_id: conn.id.clone(),
                group: None,
            },
            on_delete: NavigatorMenuAction::DeleteConnection(conn.id.clone()),
        },
    )
}

/// React `handleDatabaseContextMenu`.
pub fn database_context_menu(
    inputs: &NavigatorMenuInputs<'_>,
    db_name: &str,
    connection_id: &str,
) -> Vec<MenuNode<NavigatorMenuAction>> {
    let entry = inputs.entry(connection_id);
    let db_session_id = entry.db_session_id.clone();
    let conn = inputs.conn(connection_id);
    let db_meta = conn.map(|c| inputs.meta(&c.database_type)).unwrap_or_default();
    let read_only = inputs.read_only(conn, &db_meta);

    let slots = SchemaTreeMenuSlots {
        // Release the driver-side pool for this database and drop the tree's
        // local cache; the session itself stays connected. Tabs opened on
        // that database are closed with it.
        on_close_database: db_session_id.clone().map(|db_session_id| {
            NavigatorMenuAction::CloseDatabase {
                connection_id: connection_id.to_owned(),
                db_session_id,
                db_name: db_name.to_owned(),
            }
        }),
        on_refresh: db_session_id.as_ref().map(|_| NavigatorMenuAction::RefreshDatabase {
            connection_id: connection_id.to_owned(),
            db_name: db_name.to_owned(),
        }),
        on_new_query: Some(NavigatorMenuAction::NewQuery {
            connection_id: connection_id.to_owned(),
            sql: None,
            database: Some(db_name.to_owned()),
            schema: None,
        }),
        on_query_history: Some(NavigatorMenuAction::QueryHistory {
            connection_id: connection_id.to_owned(),
            pending_connect: None,
        }),
        on_copy_database_name: Some(NavigatorMenuAction::CopyDatabaseName(db_name.to_owned())),
        // Pin the clicked database, then pass it explicitly so a reused ER
        // tab re-binds even if this menu fired without a fresh render.
        on_view_er_diagram: Some(NavigatorMenuAction::ViewErDiagram {
            connection_id: connection_id.to_owned(),
            db_name: db_name.to_owned(),
            focus_table: None,
        }),
        on_execute_sql_file: (inputs.view.open_sql_file && !read_only && !inputs.safe_mode)
            .then(|| NavigatorMenuAction::ExecuteSqlFile {
                connection_id: connection_id.to_owned(),
            }),
        on_new_table: inputs.view.create_table.then(|| NavigatorMenuAction::NewTable {
            connection_id: connection_id.to_owned(),
        }),
        on_create_schema: (db_meta.supports_create_schema
            && db_session_id.is_some())
            .then(|| NavigatorMenuAction::OpenCreateSchema {
                connection_id: connection_id.to_owned(),
                db_name: db_name.to_owned(),
                db_session_id: db_session_id.clone().unwrap_or_default(),
            }),
        on_drop_database: db_session_id.clone().map(|db_session_id| {
            NavigatorMenuAction::DropDatabase {
                connection_id: connection_id.to_owned(),
                db_session_id,
                db_name: db_name.to_owned(),
                configured_db: conn.and_then(|c| c.database.clone()),
            }
        }),
        on_data_transfer: inputs
            .features
            .data_transfer
            .then_some(NavigatorMenuAction::OpenDataTransferWindow),
        on_compare_schema: inputs
            .features
            .schema_diff
            .then_some(NavigatorMenuAction::OpenSchemaDiffWindow),
        on_compare_data: inputs
            .features
            .data_sync
            .then_some(NavigatorMenuAction::OpenDataSyncWindow),
        on_backup: db_meta.supports_backup.then(|| NavigatorMenuAction::OpenBackupWindow {
            restore: false,
            connection_id: connection_id.to_owned(),
            database: Some(db_name.to_owned()),
        }),
        on_restore: db_meta.supports_backup.then(|| NavigatorMenuAction::OpenBackupWindow {
            restore: true,
            connection_id: connection_id.to_owned(),
            database: Some(db_name.to_owned()),
        }),
        ..SchemaTreeMenuSlots::empty()
    };

    build_schema_tree_context_menu_items(BuildSchemaTreeMenuArgs {
        kind: SchemaTreeNodeKind::Database,
        labels: &inputs.labels.schema,
        handlers: slots,
        read_only,
        safe_mode: inputs.safe_mode,
        show_new_table: true,
        category_id: None,
        features: inputs.features,
        show_open_structure: false,
        show_er_focus: false,
        show_export: None,
        show_batch_export: None,
    })
}

/// React `handleSchemaContextMenu`.
pub fn schema_context_menu(
    inputs: &NavigatorMenuInputs<'_>,
    schema_name: &str,
    db_name: &str,
    connection_id: &str,
) -> Vec<MenuNode<NavigatorMenuAction>> {
    let entry = inputs.entry(connection_id);
    let db_session_id = entry.db_session_id.clone();
    let conn = inputs.conn(connection_id);
    let db_meta = conn.map(|c| inputs.meta(&c.database_type)).unwrap_or_default();
    let read_only = inputs.read_only(conn, &db_meta);
    // React gates drop-schema on the non-system prefixes.
    let droppable =
        db_session_id.is_some() && !schema_name.starts_with("pg_") && schema_name != "information_schema";

    let slots = SchemaTreeMenuSlots {
        on_refresh: Some(NavigatorMenuAction::RefreshSchema {
            connection_id: connection_id.to_owned(),
            db_name: db_name.to_owned(),
            schema_name: schema_name.to_owned(),
        }),
        on_new_query: Some(NavigatorMenuAction::NewQuery {
            connection_id: connection_id.to_owned(),
            sql: None,
            database: Some(db_name.to_owned()),
            schema: None,
        }),
        on_query_history: Some(NavigatorMenuAction::QueryHistory {
            connection_id: connection_id.to_owned(),
            pending_connect: None,
        }),
        on_execute_sql_file: (inputs.view.open_sql_file && !read_only && !inputs.safe_mode)
            .then(|| NavigatorMenuAction::ExecuteSqlFile {
                connection_id: connection_id.to_owned(),
            }),
        on_new_table: inputs.view.create_table.then(|| NavigatorMenuAction::NewTable {
            connection_id: connection_id.to_owned(),
        }),
        on_copy_name: Some(NavigatorMenuAction::CopyText(schema_name.to_owned())),
        // Same contract as the database menu: pin + explicit target.
        on_view_er_diagram: Some(NavigatorMenuAction::ViewErDiagram {
            connection_id: connection_id.to_owned(),
            db_name: db_name.to_owned(),
            focus_table: None,
        }),
        on_drop_schema: droppable.then(|| NavigatorMenuAction::DropSchema {
            db_session_id: db_session_id.unwrap_or_default(),
            db_name: db_name.to_owned(),
            schema_name: schema_name.to_owned(),
        }),
        on_data_transfer: inputs
            .features
            .data_transfer
            .then_some(NavigatorMenuAction::OpenDataTransferWindow),
        on_compare_schema: inputs
            .features
            .schema_diff
            .then_some(NavigatorMenuAction::OpenSchemaDiffWindow),
        on_compare_data: inputs
            .features
            .data_sync
            .then_some(NavigatorMenuAction::OpenDataSyncWindow),
        ..SchemaTreeMenuSlots::empty()
    };

    build_schema_tree_context_menu_items(BuildSchemaTreeMenuArgs {
        kind: SchemaTreeNodeKind::Schema,
        labels: &inputs.labels.schema,
        handlers: slots,
        read_only,
        safe_mode: inputs.safe_mode,
        show_new_table: false,
        category_id: None,
        features: inputs.features,
        show_open_structure: false,
        show_er_focus: false,
        show_export: None,
        show_batch_export: None,
    })
}

/// React `handleTableContextMenu`'s argument tuple.
pub struct TableMenuArgs<'a> {
    /// `'table' | 'view'`.
    pub is_table: bool,
    pub name: &'a str,
    pub schema: Option<&'a str>,
    pub db_name: &'a str,
    pub connection_id: &'a str,
    pub db_session_id: &'a str,
}

/// React `handleTableContextMenu`.
pub fn table_context_menu(
    inputs: &NavigatorMenuInputs<'_>,
    args: TableMenuArgs<'_>,
) -> Vec<MenuNode<NavigatorMenuAction>> {
    let TableMenuArgs { is_table, name, schema, db_name, connection_id, db_session_id } = args;
    let conn = inputs.conn(connection_id);
    let database_type = conn
        .map(|c| c.database_type.clone())
        .unwrap_or_else(|| "postgresql".to_owned());
    let db_meta = inputs.meta(&database_type);
    let read_only = inputs.read_only(conn, &db_meta);
    let quoted = quote_relation_name(name, schema, quote_char_of(&db_meta));
    let is_view = !is_table;
    // `dbMeta?.supportsErDiagram !== false` — a missing meta keeps the item.
    let supports_er_diagram = db_meta.supports_er_diagram != Some(false);

    let gen = |kind: &'static str| NavigatorMenuAction::GenerateTableSql {
        kind,
        connection_id: connection_id.to_owned(),
        db_session_id: db_session_id.to_owned(),
        db_name: db_name.to_owned(),
        name: name.to_owned(),
        schema: schema.map(str::to_owned),
        database_type: database_type.clone(),
    };

    let slots = SchemaTreeMenuSlots {
        on_open: Some(NavigatorMenuAction::OpenTable {
            connection_id: connection_id.to_owned(),
            db_session_id: db_session_id.to_owned(),
            db_name: db_name.to_owned(),
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
        }),
        on_generate_select: is_table.then(|| gen("select")),
        on_open_structure: (is_table && inputs.view.open_table_structure).then(|| {
            NavigatorMenuAction::OpenTableStructure { name: name.to_owned() }
        }),
        on_generate_insert: is_table.then(|| gen("insert")),
        on_generate_update: is_table.then(|| gen("update")),
        on_generate_delete: is_table.then(|| gen("delete")),
        on_generate_ddl: is_table.then(|| NavigatorMenuAction::GenerateDdl {
            connection_id: connection_id.to_owned(),
            db_session_id: db_session_id.to_owned(),
            db_name: db_name.to_owned(),
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            database_type: database_type.clone(),
        }),
        on_copy_name: Some(NavigatorMenuAction::CopyText(name.to_owned())),
        on_copy_ddl: Some(NavigatorMenuAction::CopyDdl {
            db_session_id: db_session_id.to_owned(),
            db_name: db_name.to_owned(),
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            database_type: database_type.clone(),
            is_view,
        }),
        on_focus_er: (is_table && supports_er_diagram).then(|| NavigatorMenuAction::FocusEr {
            connection_id: connection_id.to_owned(),
            name: name.to_owned(),
            db_name: db_name.to_owned(),
        }),
        on_new_query: Some(NavigatorMenuAction::NewQueryForTable {
            is_table,
            connection_id: connection_id.to_owned(),
            db_session_id: db_session_id.to_owned(),
            db_name: db_name.to_owned(),
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            database_type: database_type.clone(),
        }),
        on_truncate: (is_table && !read_only && !inputs.safe_mode).then(|| {
            NavigatorMenuAction::TruncateTable {
                db_session_id: db_session_id.to_owned(),
                db_name: db_name.to_owned(),
                name: name.to_owned(),
                schema: schema.map(str::to_owned),
                quoted: quoted.clone(),
            }
        }),
        on_drop: (!read_only && !inputs.safe_mode).then(|| NavigatorMenuAction::DropRelation {
            connection_id: connection_id.to_owned(),
            db_session_id: db_session_id.to_owned(),
            db_name: db_name.to_owned(),
            is_view,
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            quoted: quoted.clone(),
        }),
        ..SchemaTreeMenuSlots::empty()
    };

    build_schema_tree_context_menu_items(BuildSchemaTreeMenuArgs {
        kind: if is_table { SchemaTreeNodeKind::Table } else { SchemaTreeNodeKind::View },
        labels: &inputs.labels.schema,
        handlers: slots,
        read_only,
        safe_mode: inputs.safe_mode,
        show_er_focus: supports_er_diagram,
        // The navigator offers open-structure only when the host wired it.
        show_open_structure: inputs.view.open_table_structure,
        show_export: None,
        show_batch_export: None,
        show_new_table: false,
        category_id: None,
        features: inputs.features,
    })
}

/// React `handleCategoryContextMenu`.
pub fn category_context_menu(
    inputs: &NavigatorMenuInputs<'_>,
    cat_key: &str,
    cat_id: &str,
    connection_id: &str,
) -> Vec<MenuNode<NavigatorMenuAction>> {
    let conn = inputs.conn(connection_id);
    let db_meta = conn.map(|c| inputs.meta(&c.database_type)).unwrap_or_default();
    let read_only = inputs.read_only(conn, &db_meta);
    let entry = inputs.entry(connection_id);
    let db_session_id = entry.db_session_id.clone();

    let slots = SchemaTreeMenuSlots {
        on_refresh: (db_session_id.is_some() && conn.is_some()).then(|| {
            // React splits the catKey: `connectionId::dbName::[schema::]cat`.
            let parts: Vec<&str> = cat_key.split("::").collect();
            let db_name = parts.get(1).copied().unwrap_or_default().to_owned();
            let via_tables = cat_id == "tables" || cat_id == "views";
            let multi_db = conn
                .map(|c| {
                    should_use_multi_database_tree(
                        db_meta.has_multi_database,
                        &db_meta.database_field_type,
                        c.database.as_deref(),
                    )
                })
                .unwrap_or(false);
            NavigatorMenuAction::RefreshCategory {
                connection_id: connection_id.to_owned(),
                db_session_id: db_session_id.clone().unwrap_or_default(),
                db_name,
                cat_key: cat_key.to_owned(),
                cat_id: cat_id.to_owned(),
                via_tables,
                multi_db,
            }
        }),
        on_new_table: (cat_id == "tables" && !read_only && inputs.view.create_table).then(|| {
            NavigatorMenuAction::NewTableForCategory { connection_id: connection_id.to_owned() }
        }),
        ..SchemaTreeMenuSlots::empty()
    };

    build_schema_tree_context_menu_items(BuildSchemaTreeMenuArgs {
        kind: SchemaTreeNodeKind::Category,
        labels: &inputs.labels.schema,
        handlers: slots,
        read_only,
        safe_mode: inputs.safe_mode,
        show_new_table: false,
        category_id: Some(cat_id.to_owned()),
        features: inputs.features,
        show_open_structure: false,
        show_er_focus: false,
        show_export: None,
        show_batch_export: None,
    })
}

/// React `handleObjectContextMenu` — a bare copy-name menu.
pub fn object_context_menu(
    inputs: &NavigatorMenuInputs<'_>,
    name: &str,
) -> Vec<MenuNode<NavigatorMenuAction>> {
    vec![MenuNode::item(
        "copy-name",
        &inputs.labels.schema.copy_name,
        NavigatorMenuAction::CopyText(name.to_owned()),
    )]
}
