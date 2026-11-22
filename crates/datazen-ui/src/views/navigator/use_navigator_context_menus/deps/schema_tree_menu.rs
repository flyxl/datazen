//! React `lib/schemaTreeContextMenu.ts` — the schema tree's context-menu
//! builder, ported verbatim for the navigator (T578). Removal stage: shard
//! 2's T599 ports the file formally; this moves beside it, behaviour
//! unchanged.
//!
//! Table order aligns with TablePlus:
//! Open → Structure → New Query → Copy… → Export… → Truncate/Drop.

use super::native_menu::MenuNode;

/// React `SchemaTreeNodeKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaTreeNodeKind {
    Table,
    View,
    Database,
    Schema,
    Category,
    Blank,
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl SchemaTreeNodeKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::Database => "database",
            Self::Schema => "schema",
            Self::Category => "category",
            Self::Blank => "blank",
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::Type => "type",
        }
    }
}

/// React `SchemaTreeContextMenuLabels` — caller-supplied (from `t()`); no
/// hardcoded locale strings here, exactly as the React type demands.
#[derive(Clone, Debug, Default)]
pub(crate) struct SchemaTreeContextMenuLabels {
    pub open: String,
    pub open_structure: String,
    pub copy_name: String,
    pub copy_ddl: String,
    pub focus_er: String,
    pub export_data: String,
    pub import_data: String,
    pub refresh: String,
    pub new_query: String,
    pub copy_database_name: String,
    pub new_table: String,
    /// Batch export (toolbar / database / blank / optional table).
    pub batch_export: String,
    pub truncate: String,
    pub drop: String,
    pub drop_view: String,
    pub drop_database: String,
    pub close_database_connection: String,
    pub view_er_diagram: String,
    pub new_schema: String,
    pub create_schema: String,
    pub drop_schema: String,
    pub execute_sql_file: String,
    pub query_history: String,
    pub data_transfer: String,
    pub compare_schema: String,
    pub compare_data: String,
    pub backup: String,
    pub restore: String,
    pub generate_sql: String,
    pub generate_select: String,
    pub generate_insert: String,
    pub generate_update: String,
    pub generate_delete: String,
    pub generate_ddl: String,
}

/// React `SchemaTreeContextMenuHandlers`, as action slots. Every slot
/// optional; a handler that is not wired produces no item.
pub(crate) struct SchemaTreeMenuSlots<A> {
    pub on_open: Option<A>,
    pub on_open_structure: Option<A>,
    pub on_copy_name: Option<A>,
    pub on_copy_ddl: Option<A>,
    pub on_focus_er: Option<A>,
    pub on_export: Option<A>,
    pub on_import: Option<A>,
    pub on_refresh: Option<A>,
    pub on_new_query: Option<A>,
    pub on_generate_select: Option<A>,
    pub on_generate_insert: Option<A>,
    pub on_generate_update: Option<A>,
    pub on_generate_delete: Option<A>,
    pub on_generate_ddl: Option<A>,
    pub on_copy_database_name: Option<A>,
    pub on_new_table: Option<A>,
    /// Open BatchExportDialog (database / blank / optional table).
    pub on_batch_export: Option<A>,
    pub on_truncate: Option<A>,
    pub on_drop: Option<A>,
    pub on_drop_database: Option<A>,
    /// Release this database's backend resources without closing the session.
    pub on_close_database: Option<A>,
    pub on_view_er_diagram: Option<A>,
    pub on_new_schema: Option<A>,
    pub on_create_schema: Option<A>,
    pub on_drop_schema: Option<A>,
    pub on_execute_sql_file: Option<A>,
    pub on_query_history: Option<A>,
    pub on_data_transfer: Option<A>,
    pub on_compare_schema: Option<A>,
    pub on_compare_data: Option<A>,
    pub on_backup: Option<A>,
    pub on_restore: Option<A>,
}

impl<A> SchemaTreeMenuSlots<A> {
    /// Every handler absent — the base the handlers' functional updates
    /// spread over (React's object literal with only the wired keys).
    pub(crate) fn empty() -> Self {
        Self {
            on_open: None,
            on_open_structure: None,
            on_copy_name: None,
            on_copy_ddl: None,
            on_focus_er: None,
            on_export: None,
            on_import: None,
            on_refresh: None,
            on_new_query: None,
            on_generate_select: None,
            on_generate_insert: None,
            on_generate_update: None,
            on_generate_delete: None,
            on_generate_ddl: None,
            on_copy_database_name: None,
            on_new_table: None,
            on_batch_export: None,
            on_truncate: None,
            on_drop: None,
            on_drop_database: None,
            on_close_database: None,
            on_view_er_diagram: None,
            on_new_schema: None,
            on_create_schema: None,
            on_drop_schema: None,
            on_execute_sql_file: None,
            on_query_history: None,
            on_data_transfer: None,
            on_compare_schema: None,
            on_compare_data: None,
            on_backup: None,
            on_restore: None,
        }
    }
}

/// React `isProductFeatureEnabled` — the three feature flags the migration
/// tool items read. Injected so the builder stays pure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProductFeatures {
    pub data_transfer: bool,
    pub schema_diff: bool,
    pub data_sync: bool,
}

/// React `migrationToolItems` — the data-transfer / schema-diff / data-sync
/// trio, each present only when its product feature is on and its handler
/// is wired.
fn migration_tool_items<A: Clone>(
    labels: &SchemaTreeContextMenuLabels,
    handlers: &SchemaTreeMenuSlots<A>,
    features: ProductFeatures,
) -> Vec<MenuNode<A>> {
    let mut out = Vec::new();
    if features.data_transfer {
        out.extend(MenuNode::maybe_item(
            "data-transfer",
            &labels.data_transfer,
            handlers.on_data_transfer.clone(),
        ));
    }
    if features.schema_diff {
        out.extend(MenuNode::maybe_item(
            "compare-schema",
            &labels.compare_schema,
            handlers.on_compare_schema.clone(),
        ));
    }
    if features.data_sync {
        out.extend(MenuNode::maybe_item(
            "compare-data",
            &labels.compare_data,
            handlers.on_compare_data.clone(),
        ));
    }
    out
}

/// React `generateSqlSubmenu` — the SELECT/INSERT/UPDATE/DELETE/DDL submenu,
/// present when any generator is wired.
fn generate_sql_submenu<A: Clone>(
    labels: &SchemaTreeContextMenuLabels,
    handlers: &SchemaTreeMenuSlots<A>,
) -> Option<MenuNode<A>> {
    let has_any = handlers.on_generate_select.is_some()
        || handlers.on_generate_insert.is_some()
        || handlers.on_generate_update.is_some()
        || handlers.on_generate_delete.is_some()
        || handlers.on_generate_ddl.is_some();
    if !has_any {
        return None;
    }

    let sub_items: Vec<MenuNode<A>> = vec![
        MenuNode::maybe_item("generate-select", &labels.generate_select, handlers.on_generate_select.clone()),
        MenuNode::maybe_item("generate-insert", &labels.generate_insert, handlers.on_generate_insert.clone()),
        MenuNode::maybe_item("generate-update", &labels.generate_update, handlers.on_generate_update.clone()),
        MenuNode::maybe_item("generate-delete", &labels.generate_delete, handlers.on_generate_delete.clone()),
        MenuNode::maybe_item("generate-ddl", &labels.generate_ddl, handlers.on_generate_ddl.clone()),
    ]
    .into_iter()
    .flatten()
    .collect();
    if sub_items.is_empty() {
        return None;
    }
    Some(MenuNode::Submenu {
        id: "generate-sql".to_owned(),
        label: labels.generate_sql.clone(),
        items: sub_items,
    })
}

/// React `BuildSchemaTreeContextMenuArgs`.
pub(crate) struct BuildSchemaTreeMenuArgs<'a, A> {
    pub kind: SchemaTreeNodeKind,
    pub labels: &'a SchemaTreeContextMenuLabels,
    pub handlers: SchemaTreeMenuSlots<A>,
    /// Hide import / truncate / drop / new table when true.
    pub read_only: bool,
    /// Hide execute-sql-file and other mutating entries when safe mode
    /// disables them.
    pub safe_mode: bool,
    /// Include open-structure item for tables.
    pub show_open_structure: bool,
    /// Include ER focus item for tables.
    pub show_er_focus: bool,
    /// Include the single-table export item. Tables always export when
    /// `on_export` is set (flag defaults true). Views only include export
    /// when this is true.
    pub show_export: Option<bool>,
    /// Include the multi-table "batch export" item. Defaults to follow
    /// `show_export` when omitted, so existing callers keep their behavior.
    pub show_batch_export: Option<bool>,
    /// Include new-table on database / blank when `!read_only`.
    pub show_new_table: bool,
    /// Category id for `kind: Category` (e.g. `'tables'`).
    pub category_id: Option<String>,
    /// The product feature flags (React reads the global registry).
    pub features: ProductFeatures,
}

/// React `buildSchemaTreeContextMenuItems`.
pub(crate) fn build_schema_tree_context_menu_items<A: Clone>(
    args: BuildSchemaTreeMenuArgs<'_, A>,
) -> Vec<MenuNode<A>> {
    let BuildSchemaTreeMenuArgs {
        kind,
        labels,
        handlers,
        read_only,
        safe_mode,
        show_open_structure,
        show_er_focus,
        show_export,
        show_batch_export,
        show_new_table,
        category_id,
        features,
    } = args;

    /// Default `showBatchExport` to follow `showExport` when not explicitly
    /// provided.
    let batch_export_shown = |kind: SchemaTreeNodeKind| -> bool {
        show_batch_export
            .unwrap_or(match kind {
                SchemaTreeNodeKind::View => show_export == Some(true),
                _ => show_export != Some(false),
            })
    };

    match kind {
        SchemaTreeNodeKind::Table => {
            let include_export = show_export != Some(false);
            let danger: Vec<MenuNode<A>> = if !read_only {
                vec![
                    MenuNode::maybe_item("truncate", &labels.truncate, handlers.on_truncate.clone()),
                    MenuNode::maybe_item("drop", &labels.drop, handlers.on_drop.clone()),
                ]
                .into_iter()
                .flatten()
                .collect()
            } else {
                Vec::new()
            };
            let mut main: Vec<MenuNode<A>> = vec![
                MenuNode::maybe_item("open", &labels.open, handlers.on_open.clone()),
                if show_open_structure {
                    MenuNode::maybe_item(
                        "open-structure",
                        &labels.open_structure,
                        handlers.on_open_structure.clone(),
                    )
                } else {
                    None
                },
                if show_er_focus {
                    MenuNode::maybe_item("focus-er", &labels.focus_er, handlers.on_focus_er.clone())
                } else {
                    None
                },
                generate_sql_submenu(labels, &handlers),
                MenuNode::maybe_item("new-query", &labels.new_query, handlers.on_new_query.clone()),
                MenuNode::maybe_item("copy-name", &labels.copy_name, handlers.on_copy_name.clone()),
                MenuNode::maybe_item("copy-ddl", &labels.copy_ddl, handlers.on_copy_ddl.clone()),
                if include_export {
                    MenuNode::maybe_item("export", &labels.export_data, handlers.on_export.clone())
                } else {
                    None
                },
                if batch_export_shown(SchemaTreeNodeKind::Table) {
                    MenuNode::maybe_item(
                        "batch-export",
                        &labels.batch_export,
                        handlers.on_batch_export.clone(),
                    )
                } else {
                    None
                },
                if !read_only {
                    MenuNode::maybe_item("import", &labels.import_data, handlers.on_import.clone())
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            .collect();
            if danger.is_empty() {
                return main;
            }
            main.push(MenuNode::separator());
            main.extend(danger);
            main
        }
        SchemaTreeNodeKind::View => {
            let include_export = show_export == Some(true);
            let mut main: Vec<MenuNode<A>> = vec![
                MenuNode::maybe_item("open", &labels.open, handlers.on_open.clone()),
                MenuNode::maybe_item("copy-name", &labels.copy_name, handlers.on_copy_name.clone()),
                MenuNode::maybe_item("copy-ddl", &labels.copy_ddl, handlers.on_copy_ddl.clone()),
                if include_export {
                    MenuNode::maybe_item("export", &labels.export_data, handlers.on_export.clone())
                } else {
                    None
                },
                if batch_export_shown(SchemaTreeNodeKind::View) {
                    MenuNode::maybe_item(
                        "batch-export",
                        &labels.batch_export,
                        handlers.on_batch_export.clone(),
                    )
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            .collect();
            let drop = if !read_only {
                MenuNode::maybe_item("drop-view", &labels.drop_view, handlers.on_drop.clone())
            } else {
                None
            };
            let Some(drop) = drop else { return main };
            main.push(MenuNode::separator());
            main.push(drop);
            main
        }
        SchemaTreeNodeKind::Database => {
            let mut db_main: Vec<MenuNode<A>> = vec![
                MenuNode::maybe_item("refresh", &labels.refresh, handlers.on_refresh.clone()),
                MenuNode::maybe_item("new-query", &labels.new_query, handlers.on_new_query.clone()),
                MenuNode::maybe_item(
                    "query-history",
                    &labels.query_history,
                    handlers.on_query_history.clone(),
                ),
                if !read_only && !safe_mode {
                    MenuNode::maybe_item(
                        "execute-sql-file",
                        &labels.execute_sql_file,
                        handlers.on_execute_sql_file.clone(),
                    )
                } else {
                    None
                },
                MenuNode::maybe_item(
                    "copy-database-name",
                    &labels.copy_database_name,
                    handlers.on_copy_database_name.clone(),
                ),
                MenuNode::maybe_item(
                    "view-er-diagram",
                    &labels.view_er_diagram,
                    handlers.on_view_er_diagram.clone(),
                ),
                if batch_export_shown(SchemaTreeNodeKind::Database) {
                    MenuNode::maybe_item(
                        "batch-export",
                        &labels.batch_export,
                        handlers.on_batch_export.clone(),
                    )
                } else {
                    None
                },
                if !read_only {
                    MenuNode::maybe_item("import", &labels.import_data, handlers.on_import.clone())
                } else {
                    None
                },
                if !read_only && show_new_table {
                    MenuNode::maybe_item("new-table", &labels.new_table, handlers.on_new_table.clone())
                } else {
                    None
                },
                MenuNode::maybe_item(
                    "create-schema",
                    &labels.create_schema,
                    handlers.on_create_schema.clone(),
                ),
            ]
            .into_iter()
            .flatten()
            .collect();
            let sync_items = migration_tool_items(labels, &handlers, features);
            let backup_items: Vec<MenuNode<A>> = vec![
                MenuNode::maybe_item("backup", &labels.backup, handlers.on_backup.clone()),
                MenuNode::maybe_item("restore", &labels.restore, handlers.on_restore.clone()),
            ]
            .into_iter()
            .flatten()
            .collect();
            let drop_db = if !read_only {
                MenuNode::maybe_item(
                    "drop-database",
                    &labels.drop_database,
                    handlers.on_drop_database.clone(),
                )
            } else {
                None
            };
            let close_db = MenuNode::maybe_item(
                "close-database-connection",
                &labels.close_database_connection,
                handlers.on_close_database.clone(),
            );
            let mut parts = db_main;
            if !sync_items.is_empty() {
                parts.push(MenuNode::separator());
                parts.extend(sync_items);
            }
            if !backup_items.is_empty() {
                parts.push(MenuNode::separator());
                parts.extend(backup_items);
            }
            if let Some(close_db) = close_db {
                parts.push(MenuNode::separator());
                parts.push(close_db);
            }
            if let Some(drop_db) = drop_db {
                parts.push(MenuNode::separator());
                parts.push(drop_db);
            }
            parts
        }
        SchemaTreeNodeKind::Schema => {
            let mut schema_main: Vec<MenuNode<A>> = vec![
                MenuNode::maybe_item("refresh", &labels.refresh, handlers.on_refresh.clone()),
                MenuNode::maybe_item("new-query", &labels.new_query, handlers.on_new_query.clone()),
                MenuNode::maybe_item(
                    "query-history",
                    &labels.query_history,
                    handlers.on_query_history.clone(),
                ),
                if !read_only && !safe_mode {
                    MenuNode::maybe_item(
                        "execute-sql-file",
                        &labels.execute_sql_file,
                        handlers.on_execute_sql_file.clone(),
                    )
                } else {
                    None
                },
                MenuNode::maybe_item("copy-schema-name", &labels.copy_name, handlers.on_copy_name.clone()),
                MenuNode::maybe_item(
                    "view-er-diagram",
                    &labels.view_er_diagram,
                    handlers.on_view_er_diagram.clone(),
                ),
                if batch_export_shown(SchemaTreeNodeKind::Schema) {
                    MenuNode::maybe_item(
                        "batch-export",
                        &labels.batch_export,
                        handlers.on_batch_export.clone(),
                    )
                } else {
                    None
                },
                if !read_only {
                    MenuNode::maybe_item("import", &labels.import_data, handlers.on_import.clone())
                } else {
                    None
                },
                if !read_only {
                    MenuNode::maybe_item("new-table", &labels.new_table, handlers.on_new_table.clone())
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            .collect();
            let schema_sync_items = migration_tool_items(labels, &handlers, features);
            if !schema_sync_items.is_empty() {
                schema_main.push(MenuNode::separator());
                schema_main.extend(schema_sync_items);
            }
            let drop_sch = if !read_only {
                MenuNode::maybe_item("drop-schema", &labels.drop_schema, handlers.on_drop_schema.clone())
            } else {
                None
            };
            let Some(drop_sch) = drop_sch else { return schema_main };
            schema_main.push(MenuNode::separator());
            schema_main.push(drop_sch);
            schema_main
        }
        SchemaTreeNodeKind::Category => vec![
            MenuNode::maybe_item("refresh", &labels.refresh, handlers.on_refresh.clone()),
            if !read_only && category_id.as_deref() == Some("tables") {
                MenuNode::maybe_item("new-table", &labels.new_table, handlers.on_new_table.clone())
            } else {
                None
            },
            if !read_only {
                MenuNode::maybe_item("import", &labels.import_data, handlers.on_import.clone())
            } else {
                None
            },
        ]
        .into_iter()
        .flatten()
        .collect(),
        SchemaTreeNodeKind::Blank => vec![
            MenuNode::maybe_item("refresh", &labels.refresh, handlers.on_refresh.clone()),
            MenuNode::maybe_item("new-query", &labels.new_query, handlers.on_new_query.clone()),
            if batch_export_shown(SchemaTreeNodeKind::Blank) {
                MenuNode::maybe_item(
                    "batch-export",
                    &labels.batch_export,
                    handlers.on_batch_export.clone(),
                )
            } else {
                None
            },
            if !read_only {
                MenuNode::maybe_item("import", &labels.import_data, handlers.on_import.clone())
            } else {
                None
            },
            if !read_only && show_new_table {
                MenuNode::maybe_item("new-table", &labels.new_table, handlers.on_new_table.clone())
            } else {
                None
            },
        ]
        .into_iter()
        .flatten()
        .collect(),
        // function / procedure / trigger / sequence / type (and anything
        // else a future kind spells): the copy-name fallback.
        _ => vec![MenuNode::maybe_item("copy-name", &labels.copy_name, handlers.on_copy_name.clone())]
            .into_iter()
            .flatten()
            .collect(),
    }
}
