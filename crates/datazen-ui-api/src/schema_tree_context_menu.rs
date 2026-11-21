use crate::{is_product_feature_enabled, ProductFeatureKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaTreeNodeKind {
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
    pub fn as_str(self) -> &'static str {
        match self {
            SchemaTreeNodeKind::Table => "table",
            SchemaTreeNodeKind::View => "view",
            SchemaTreeNodeKind::Database => "database",
            SchemaTreeNodeKind::Schema => "schema",
            SchemaTreeNodeKind::Category => "category",
            SchemaTreeNodeKind::Blank => "blank",
            SchemaTreeNodeKind::Function => "function",
            SchemaTreeNodeKind::Procedure => "procedure",
            SchemaTreeNodeKind::Trigger => "trigger",
            SchemaTreeNodeKind::Sequence => "sequence",
            SchemaTreeNodeKind::Type => "type",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SchemaTreeNodeContextMenuPayload {
    pub kind: SchemaTreeNodeKind,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub schema: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaTreeContextMenuLabels {
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
    pub generate_sql: Option<String>,
    pub generate_select: Option<String>,
    pub generate_insert: Option<String>,
    pub generate_update: Option<String>,
    pub generate_delete: Option<String>,
    pub generate_ddl: Option<String>,
}

#[derive(Default)]
pub struct SchemaTreeContextMenuHandlers {
    pub on_open: Option<Box<dyn Fn()>>,
    pub on_open_structure: Option<Box<dyn Fn()>>,
    pub on_copy_name: Option<Box<dyn Fn()>>,
    pub on_copy_ddl: Option<Box<dyn Fn()>>,
    pub on_focus_er: Option<Box<dyn Fn()>>,
    pub on_export: Option<Box<dyn Fn()>>,
    pub on_import: Option<Box<dyn Fn()>>,
    pub on_refresh: Option<Box<dyn Fn()>>,
    pub on_new_query: Option<Box<dyn Fn()>>,
    pub on_generate_select: Option<Box<dyn Fn()>>,
    pub on_generate_insert: Option<Box<dyn Fn()>>,
    pub on_generate_update: Option<Box<dyn Fn()>>,
    pub on_generate_delete: Option<Box<dyn Fn()>>,
    pub on_generate_ddl: Option<Box<dyn Fn()>>,
    pub on_copy_database_name: Option<Box<dyn Fn()>>,
    pub on_new_table: Option<Box<dyn Fn()>>,
    pub on_batch_export: Option<Box<dyn Fn()>>,
    pub on_truncate: Option<Box<dyn Fn()>>,
    pub on_drop: Option<Box<dyn Fn()>>,
    pub on_drop_database: Option<Box<dyn Fn()>>,
    pub on_close_database: Option<Box<dyn Fn()>>,
    pub on_view_er_diagram: Option<Box<dyn Fn()>>,
    pub on_new_schema: Option<Box<dyn Fn()>>,
    pub on_create_schema: Option<Box<dyn Fn()>>,
    pub on_drop_schema: Option<Box<dyn Fn()>>,
    pub on_execute_sql_file: Option<Box<dyn Fn()>>,
    pub on_query_history: Option<Box<dyn Fn()>>,
    pub on_data_transfer: Option<Box<dyn Fn()>>,
    pub on_compare_schema: Option<Box<dyn Fn()>>,
    pub on_compare_data: Option<Box<dyn Fn()>>,
    pub on_backup: Option<Box<dyn Fn()>>,
    pub on_restore: Option<Box<dyn Fn()>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaTreeAction {
    Open,
    OpenStructure,
    FocusEr,
    GenerateSql,
    GenerateSelect,
    GenerateInsert,
    GenerateUpdate,
    GenerateDelete,
    GenerateDdl,
    NewQuery,
    CopyName,
    CopyDdl,
    Export,
    BatchExport,
    Import,
    Truncate,
    Drop,
    Refresh,
    QueryHistory,
    ExecuteSqlFile,
    CopyDatabaseName,
    ViewErDiagram,
    CreateSchema,
    DataTransfer,
    CompareSchema,
    CompareData,
    Backup,
    Restore,
    CloseDatabaseConnection,
    DropDatabase,
    DropView,
    DropSchema,
    CopySchemaName,
    NewTable,
}

impl SchemaTreeAction {
    pub fn as_str(self) -> &'static str {
        match self {
            SchemaTreeAction::Open => "open",
            SchemaTreeAction::OpenStructure => "open-structure",
            SchemaTreeAction::FocusEr => "focus-er",
            SchemaTreeAction::GenerateSql => "generate-sql",
            SchemaTreeAction::GenerateSelect => "generate-select",
            SchemaTreeAction::GenerateInsert => "generate-insert",
            SchemaTreeAction::GenerateUpdate => "generate-update",
            SchemaTreeAction::GenerateDelete => "generate-delete",
            SchemaTreeAction::GenerateDdl => "generate-ddl",
            SchemaTreeAction::NewQuery => "new-query",
            SchemaTreeAction::CopyName => "copy-name",
            SchemaTreeAction::CopyDdl => "copy-ddl",
            SchemaTreeAction::Export => "export",
            SchemaTreeAction::BatchExport => "batch-export",
            SchemaTreeAction::Import => "import",
            SchemaTreeAction::Truncate => "truncate",
            SchemaTreeAction::Drop => "drop",
            SchemaTreeAction::Refresh => "refresh",
            SchemaTreeAction::QueryHistory => "query-history",
            SchemaTreeAction::ExecuteSqlFile => "execute-sql-file",
            SchemaTreeAction::CopyDatabaseName => "copy-database-name",
            SchemaTreeAction::ViewErDiagram => "view-er-diagram",
            SchemaTreeAction::CreateSchema => "create-schema",
            SchemaTreeAction::DataTransfer => "data-transfer",
            SchemaTreeAction::CompareSchema => "compare-schema",
            SchemaTreeAction::CompareData => "compare-data",
            SchemaTreeAction::Backup => "backup",
            SchemaTreeAction::Restore => "restore",
            SchemaTreeAction::CloseDatabaseConnection => "close-database-connection",
            SchemaTreeAction::DropDatabase => "drop-database",
            SchemaTreeAction::DropView => "drop-view",
            SchemaTreeAction::DropSchema => "drop-schema",
            SchemaTreeAction::CopySchemaName => "copy-schema-name",
            SchemaTreeAction::NewTable => "new-table",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaTreeMenuItem {
    Item {
        id: String,
        label: String,
        action: SchemaTreeAction,
    },
    Separator,
    Submenu {
        id: String,
        label: String,
        items: Vec<SchemaTreeMenuItem>,
    },
}

pub struct BuildSchemaTreeContextMenuArgs {
    pub kind: SchemaTreeNodeKind,
    pub labels: SchemaTreeContextMenuLabels,
    pub handlers: SchemaTreeContextMenuHandlers,
    pub read_only: bool,
    pub safe_mode: bool,
    pub show_open_structure: bool,
    pub show_er_focus: bool,
    pub show_export: Option<bool>,
    pub show_batch_export: Option<bool>,
    pub show_new_table: bool,
    pub category_id: Option<String>,
}

fn item(id: SchemaTreeAction, label: &str, present: bool) -> Option<SchemaTreeMenuItem> {
    if !present {
        return None;
    }
    Some(SchemaTreeMenuItem::Item { id: id.as_str().to_owned(), label: label.to_owned(), action: id })
}

fn push(defs: Vec<Option<SchemaTreeMenuItem>>) -> Vec<SchemaTreeMenuItem> {
    defs.into_iter().flatten().collect()
}

fn migration_tool_items(
    labels: &SchemaTreeContextMenuLabels,
    handlers: &HandlersView,
) -> Vec<SchemaTreeMenuItem> {
    push(vec![
        if is_product_feature_enabled(ProductFeatureKey::DataTransfer) {
            item(SchemaTreeAction::DataTransfer, &labels.data_transfer, handlers.data_transfer)
        } else {
            None
        },
        if is_product_feature_enabled(ProductFeatureKey::SchemaDiff) {
            item(SchemaTreeAction::CompareSchema, &labels.compare_schema, handlers.compare_schema)
        } else {
            None
        },
        if is_product_feature_enabled(ProductFeatureKey::DataSync) {
            item(SchemaTreeAction::CompareData, &labels.compare_data, handlers.compare_data)
        } else {
            None
        },
    ])
}

fn generate_sql_submenu(
    labels: &SchemaTreeContextMenuLabels,
    handlers: &HandlersView,
) -> Option<SchemaTreeMenuItem> {
    let sub_items = push(vec![
        item(SchemaTreeAction::GenerateSelect, &labels.generate_select.clone().unwrap_or_else(|| "SELECT".to_owned()), handlers.generate_select),
        item(SchemaTreeAction::GenerateInsert, &labels.generate_insert.clone().unwrap_or_else(|| "INSERT".to_owned()), handlers.generate_insert),
        item(SchemaTreeAction::GenerateUpdate, &labels.generate_update.clone().unwrap_or_else(|| "UPDATE".to_owned()), handlers.generate_update),
        item(SchemaTreeAction::GenerateDelete, &labels.generate_delete.clone().unwrap_or_else(|| "DELETE".to_owned()), handlers.generate_delete),
        item(SchemaTreeAction::GenerateDdl, &labels.generate_ddl.clone().unwrap_or_else(|| "DDL".to_owned()), handlers.generate_ddl),
    ]);
    if sub_items.is_empty() {
        return None;
    }
    Some(SchemaTreeMenuItem::Submenu {
        id: SchemaTreeAction::GenerateSql.as_str().to_owned(),
        label: labels.generate_sql.clone().unwrap_or_else(|| "Generate SQL".to_owned()),
        items: sub_items,
    })
}

fn batch_export_shown(kind: SchemaTreeNodeKind, show_export: Option<bool>, show_batch_export: Option<bool>) -> bool {
    match show_batch_export {
        Some(value) => value,
        None => {
            if kind == SchemaTreeNodeKind::View {
                show_export == Some(true)
            } else {
                show_export != Some(false)
            }
        }
    }
}

pub fn build_schema_tree_context_menu_items(
    args: &BuildSchemaTreeContextMenuArgs,
) -> Vec<SchemaTreeMenuItem> {
    let labels = &args.labels;
    let handlers = HandlersView::from_args(args);
    let batch = |kind: SchemaTreeNodeKind| batch_export_shown(kind, args.show_export, args.show_batch_export);

    match args.kind {
        SchemaTreeNodeKind::Table => {
            let danger = if args.read_only {
                Vec::new()
            } else {
                push(vec![
                    item(SchemaTreeAction::Truncate, &labels.truncate, handlers.truncate),
                    item(SchemaTreeAction::Drop, &labels.drop, handlers.drop),
                ])
            };
            let mut main = push(vec![
                item(SchemaTreeAction::Open, &labels.open, handlers.open),
                item(SchemaTreeAction::OpenStructure, &labels.open_structure, args.show_open_structure && handlers.open_structure),
                item(SchemaTreeAction::FocusEr, &labels.focus_er, args.show_er_focus && handlers.focus_er),
                generate_sql_submenu(labels, &handlers),
                item(SchemaTreeAction::NewQuery, &labels.new_query, handlers.new_query),
                item(SchemaTreeAction::CopyName, &labels.copy_name, handlers.copy_name),
                item(SchemaTreeAction::CopyDdl, &labels.copy_ddl, handlers.copy_ddl),
                item(SchemaTreeAction::Export, &labels.export_data, args.show_export != Some(false) && handlers.export),
                item(SchemaTreeAction::BatchExport, &labels.batch_export, batch(SchemaTreeNodeKind::Table) && handlers.batch_export),
                item(SchemaTreeAction::Import, &labels.import_data, !args.read_only && handlers.import),
            ]);
            if danger.is_empty() {
                return main;
            }
            main.push(SchemaTreeMenuItem::Separator);
            main.extend(danger);
            main
        }
        SchemaTreeNodeKind::View => {
            let mut main = push(vec![
                item(SchemaTreeAction::Open, &labels.open, handlers.open),
                item(SchemaTreeAction::CopyName, &labels.copy_name, handlers.copy_name),
                item(SchemaTreeAction::CopyDdl, &labels.copy_ddl, handlers.copy_ddl),
                item(SchemaTreeAction::Export, &labels.export_data, args.show_export == Some(true) && handlers.export),
                item(SchemaTreeAction::BatchExport, &labels.batch_export, batch(SchemaTreeNodeKind::View) && handlers.batch_export),
            ]);
            if args.read_only || !handlers.drop {
                return main;
            }
            main.push(SchemaTreeMenuItem::Separator);
            main.push(SchemaTreeMenuItem::Item {
                id: SchemaTreeAction::DropView.as_str().to_owned(),
                label: labels.drop_view.clone(),
                action: SchemaTreeAction::DropView,
            });
            main
        }
        SchemaTreeNodeKind::Database => build_database_menu(args, &handlers, labels, batch(SchemaTreeNodeKind::Database)),
        SchemaTreeNodeKind::Schema => build_schema_menu(args, &handlers, labels, batch(SchemaTreeNodeKind::Schema)),
        SchemaTreeNodeKind::Category => push(vec![
            item(SchemaTreeAction::Refresh, &labels.refresh, handlers.refresh),
            item(SchemaTreeAction::NewTable, &labels.new_table, !args.read_only && args.category_id.as_deref() == Some("tables") && handlers.new_table),
            item(SchemaTreeAction::Import, &labels.import_data, !args.read_only && handlers.import),
        ]),
        SchemaTreeNodeKind::Blank => push(vec![
            item(SchemaTreeAction::Refresh, &labels.refresh, handlers.refresh),
            item(SchemaTreeAction::NewQuery, &labels.new_query, handlers.new_query),
            item(SchemaTreeAction::BatchExport, &labels.batch_export, batch(SchemaTreeNodeKind::Blank) && handlers.batch_export),
            item(SchemaTreeAction::Import, &labels.import_data, !args.read_only && handlers.import),
            item(SchemaTreeAction::NewTable, &labels.new_table, !args.read_only && args.show_new_table && handlers.new_table),
        ]),
        _ => push(vec![item(SchemaTreeAction::CopyName, &labels.copy_name, handlers.copy_name)]),
    }
}

struct HandlersView {
    open: bool,
    open_structure: bool,
    focus_er: bool,
    new_query: bool,
    copy_name: bool,
    copy_ddl: bool,
    export: bool,
    batch_export: bool,
    import: bool,
    truncate: bool,
    drop: bool,
    refresh: bool,
    query_history: bool,
    execute_sql_file: bool,
    copy_database_name: bool,
    view_er_diagram: bool,
    create_schema: bool,
    backup: bool,
    restore: bool,
    close_database: bool,
    drop_database: bool,
    drop_schema: bool,
    new_table: bool,
    generate_select: bool,
    generate_insert: bool,
    generate_update: bool,
    generate_delete: bool,
    generate_ddl: bool,
    data_transfer: bool,
    compare_schema: bool,
    compare_data: bool,
}

impl HandlersView {
    fn from_args(args: &BuildSchemaTreeContextMenuArgs) -> Self {
        let handlers = &args.handlers;
        Self {
            open: handlers.on_open.is_some(),
            open_structure: handlers.on_open_structure.is_some(),
            focus_er: handlers.on_focus_er.is_some(),
            new_query: handlers.on_new_query.is_some(),
            copy_name: handlers.on_copy_name.is_some(),
            copy_ddl: handlers.on_copy_ddl.is_some(),
            export: handlers.on_export.is_some(),
            batch_export: handlers.on_batch_export.is_some(),
            import: handlers.on_import.is_some(),
            truncate: handlers.on_truncate.is_some(),
            drop: handlers.on_drop.is_some(),
            refresh: handlers.on_refresh.is_some(),
            query_history: handlers.on_query_history.is_some(),
            execute_sql_file: handlers.on_execute_sql_file.is_some(),
            copy_database_name: handlers.on_copy_database_name.is_some(),
            view_er_diagram: handlers.on_view_er_diagram.is_some(),
            create_schema: handlers.on_create_schema.is_some(),
            backup: handlers.on_backup.is_some(),
            restore: handlers.on_restore.is_some(),
            close_database: handlers.on_close_database.is_some(),
            drop_database: handlers.on_drop_database.is_some(),
            drop_schema: handlers.on_drop_schema.is_some(),
            new_table: handlers.on_new_table.is_some(),
            generate_select: handlers.on_generate_select.is_some(),
            generate_insert: handlers.on_generate_insert.is_some(),
            generate_update: handlers.on_generate_update.is_some(),
            generate_delete: handlers.on_generate_delete.is_some(),
            generate_ddl: handlers.on_generate_ddl.is_some(),
            data_transfer: handlers.on_data_transfer.is_some(),
            compare_schema: handlers.on_compare_schema.is_some(),
            compare_data: handlers.on_compare_data.is_some(),
        }
    }
}

fn build_database_menu(
    args: &BuildSchemaTreeContextMenuArgs,
    handlers: &HandlersView,
    labels: &SchemaTreeContextMenuLabels,
    batch_export: bool,
) -> Vec<SchemaTreeMenuItem> {
    let mut parts = push(vec![
        item(SchemaTreeAction::Refresh, &labels.refresh, handlers.refresh),
        item(SchemaTreeAction::NewQuery, &labels.new_query, handlers.new_query),
        item(SchemaTreeAction::QueryHistory, &labels.query_history, handlers.query_history),
        item(SchemaTreeAction::ExecuteSqlFile, &labels.execute_sql_file, !args.read_only && !args.safe_mode && handlers.execute_sql_file),
        item(SchemaTreeAction::CopyDatabaseName, &labels.copy_database_name, handlers.copy_database_name),
        item(SchemaTreeAction::ViewErDiagram, &labels.view_er_diagram, handlers.view_er_diagram),
        item(SchemaTreeAction::BatchExport, &labels.batch_export, batch_export && handlers.batch_export),
        item(SchemaTreeAction::Import, &labels.import_data, !args.read_only && handlers.import),
        item(SchemaTreeAction::NewTable, &labels.new_table, !args.read_only && args.show_new_table && handlers.new_table),
        item(SchemaTreeAction::CreateSchema, &labels.create_schema, handlers.create_schema),
    ]);
    let sync = migration_tool_items(labels, handlers);
    if !sync.is_empty() {
        parts.push(SchemaTreeMenuItem::Separator);
        parts.extend(sync);
    }
    let backup = push(vec![
        item(SchemaTreeAction::Backup, &labels.backup, handlers.backup),
        item(SchemaTreeAction::Restore, &labels.restore, handlers.restore),
    ]);
    if !backup.is_empty() {
        parts.push(SchemaTreeMenuItem::Separator);
        parts.extend(backup);
    }
    if handlers.close_database {
        parts.push(SchemaTreeMenuItem::Separator);
        parts.push(SchemaTreeMenuItem::Item {
            id: SchemaTreeAction::CloseDatabaseConnection.as_str().to_owned(),
            label: labels.close_database_connection.clone(),
            action: SchemaTreeAction::CloseDatabaseConnection,
        });
    }
    if !args.read_only && handlers.drop_database {
        parts.push(SchemaTreeMenuItem::Separator);
        parts.push(SchemaTreeMenuItem::Item {
            id: SchemaTreeAction::DropDatabase.as_str().to_owned(),
            label: labels.drop_database.clone(),
            action: SchemaTreeAction::DropDatabase,
        });
    }
    parts
}

fn build_schema_menu(
    args: &BuildSchemaTreeContextMenuArgs,
    handlers: &HandlersView,
    labels: &SchemaTreeContextMenuLabels,
    batch_export: bool,
) -> Vec<SchemaTreeMenuItem> {
    let mut parts = push(vec![
        item(SchemaTreeAction::Refresh, &labels.refresh, handlers.refresh),
        item(SchemaTreeAction::NewQuery, &labels.new_query, handlers.new_query),
        item(SchemaTreeAction::QueryHistory, &labels.query_history, handlers.query_history),
        item(SchemaTreeAction::ExecuteSqlFile, &labels.execute_sql_file, !args.read_only && !args.safe_mode && handlers.execute_sql_file),
        item(SchemaTreeAction::CopySchemaName, &labels.copy_name, handlers.copy_name),
        item(SchemaTreeAction::ViewErDiagram, &labels.view_er_diagram, handlers.view_er_diagram),
        item(SchemaTreeAction::BatchExport, &labels.batch_export, batch_export && handlers.batch_export),
        item(SchemaTreeAction::Import, &labels.import_data, !args.read_only && handlers.import),
        item(SchemaTreeAction::NewTable, &labels.new_table, !args.read_only && handlers.new_table),
    ]);
    let sync = migration_tool_items(labels, handlers);
    if !sync.is_empty() {
        parts.push(SchemaTreeMenuItem::Separator);
        parts.extend(sync);
    }
    if !args.read_only && handlers.drop_schema {
        parts.push(SchemaTreeMenuItem::Separator);
        parts.push(SchemaTreeMenuItem::Item {
            id: SchemaTreeAction::DropSchema.as_str().to_owned(),
            label: labels.drop_schema.clone(),
            action: SchemaTreeAction::DropSchema,
        });
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(kind: SchemaTreeNodeKind) -> BuildSchemaTreeContextMenuArgs {
        BuildSchemaTreeContextMenuArgs {
            kind,
            labels: SchemaTreeContextMenuLabels {
                open: "Open".to_owned(),
                refresh: "Refresh".to_owned(),
                new_query: "New Query".to_owned(),
                ..Default::default()
            },
            handlers: SchemaTreeContextMenuHandlers::default(),
            read_only: false,
            safe_mode: false,
            show_open_structure: false,
            show_er_focus: false,
            show_export: None,
            show_batch_export: None,
            show_new_table: false,
            category_id: None,
        }
    }

    fn ids(items: &[SchemaTreeMenuItem]) -> Vec<String> {
        items
            .iter()
            .filter_map(|item| match item {
                SchemaTreeMenuItem::Item { id, .. } | SchemaTreeMenuItem::Submenu { id, .. } => Some(id.clone()),
                SchemaTreeMenuItem::Separator => None,
            })
            .collect()
    }

    #[test]
    fn kind_literals_are_pinned() {
        assert_eq!(SchemaTreeNodeKind::Table.as_str(), "table");
        assert_eq!(SchemaTreeNodeKind::Blank.as_str(), "blank");
        assert_eq!(SchemaTreeNodeKind::Sequence.as_str(), "sequence");
    }

    #[test]
    fn table_menu_orders_items_and_appends_danger_after_separator() {
        let mut input = args(SchemaTreeNodeKind::Table);
        input.handlers.on_open = Some(Box::new(|| ()));
        input.handlers.on_copy_name = Some(Box::new(|| ()));
        input.handlers.on_truncate = Some(Box::new(|| ()));
        input.handlers.on_drop = Some(Box::new(|| ()));
        let items = build_schema_tree_context_menu_items(&input);
        assert_eq!(ids(&items), vec!["open", "copy-name", "truncate", "drop"]);
        assert!(matches!(items[2], SchemaTreeMenuItem::Separator));
    }

    #[test]
    fn read_only_hides_danger_and_import() {
        let mut input = args(SchemaTreeNodeKind::Table);
        input.read_only = true;
        input.handlers.on_open = Some(Box::new(|| ()));
        input.handlers.on_import = Some(Box::new(|| ()));
        input.handlers.on_drop = Some(Box::new(|| ()));
        let items = build_schema_tree_context_menu_items(&input);
        assert_eq!(ids(&items), vec!["open"]);
    }

    #[test]
    fn generate_submenu_uses_defaults_and_lists_only_present_handlers() {
        let mut input = args(SchemaTreeNodeKind::Table);
        input.handlers.on_generate_select = Some(Box::new(|| ()));
        input.handlers.on_generate_ddl = Some(Box::new(|| ()));
        let items = build_schema_tree_context_menu_items(&input);
        let submenu = items.iter().find_map(|item| match item {
            SchemaTreeMenuItem::Submenu { label, items, .. } => Some((label, items)),
            _ => None,
        }).expect("submenu");
        assert_eq!(submenu.0, "Generate SQL");
        assert_eq!(ids(submenu.1), vec!["generate-select", "generate-ddl"]);
    }

    #[test]
    fn batch_export_default_follows_show_export_with_view_special_case() {
        let mut table = args(SchemaTreeNodeKind::Table);
        table.handlers.on_batch_export = Some(Box::new(|| ()));
        assert_eq!(ids(&build_schema_tree_context_menu_items(&table)), vec!["batch-export"]);
        table.show_export = Some(false);
        assert!(ids(&build_schema_tree_context_menu_items(&table)).is_empty(), "showExport false hides table batch export");

        let mut view = args(SchemaTreeNodeKind::View);
        view.handlers.on_batch_export = Some(Box::new(|| ()));
        assert!(ids(&build_schema_tree_context_menu_items(&view)).is_empty(), "views need explicit showExport true");
        view.show_export = Some(true);
        assert_eq!(ids(&build_schema_tree_context_menu_items(&view)), vec!["batch-export"]);
    }

    #[test]
    fn explicit_show_batch_export_overrides_the_default() {
        let mut input = args(SchemaTreeNodeKind::View);
        input.handlers.on_batch_export = Some(Box::new(|| ()));
        input.show_export = Some(false);
        input.show_batch_export = Some(true);
        assert_eq!(ids(&build_schema_tree_context_menu_items(&input)), vec!["batch-export"]);
    }

    #[test]
    fn database_menu_groups_migration_backup_and_dangers() {
        let mut input = args(SchemaTreeNodeKind::Database);
        input.handlers.on_refresh = Some(Box::new(|| ()));
        input.handlers.on_backup = Some(Box::new(|| ()));
        input.handlers.on_close_database = Some(Box::new(|| ()));
        input.handlers.on_drop_database = Some(Box::new(|| ()));
        let items = build_schema_tree_context_menu_items(&input);
        assert_eq!(
            ids(&items),
            vec!["refresh", "backup", "close-database-connection", "drop-database"]
        );
        assert_eq!(items.iter().filter(|item| matches!(item, SchemaTreeMenuItem::Separator)).count(), 3);
    }

    #[test]
    fn schema_menu_drops_at_the_end_and_category_checks_id() {
        let mut schema = args(SchemaTreeNodeKind::Schema);
        schema.handlers.on_refresh = Some(Box::new(|| ()));
        schema.handlers.on_drop_schema = Some(Box::new(|| ()));
        assert_eq!(ids(&build_schema_tree_context_menu_items(&schema)), vec!["refresh", "drop-schema"]);
        schema.read_only = true;
        assert_eq!(ids(&build_schema_tree_context_menu_items(&schema)), vec!["refresh"]);

        let mut category = args(SchemaTreeNodeKind::Category);
        category.handlers.on_new_table = Some(Box::new(|| ()));
        category.handlers.on_refresh = Some(Box::new(|| ()));
        category.category_id = Some("tables".to_owned());
        assert_eq!(ids(&build_schema_tree_context_menu_items(&category)), vec!["refresh", "new-table"]);
        category.category_id = Some("views".to_owned());
        assert_eq!(ids(&build_schema_tree_context_menu_items(&category)), vec!["refresh"]);
    }

    #[test]
    fn blank_and_default_branches() {
        let mut blank = args(SchemaTreeNodeKind::Blank);
        blank.handlers.on_refresh = Some(Box::new(|| ()));
        blank.handlers.on_new_table = Some(Box::new(|| ()));
        blank.show_new_table = true;
        assert_eq!(ids(&build_schema_tree_context_menu_items(&blank)), vec!["refresh", "new-table"]);

        let mut function = args(SchemaTreeNodeKind::Function);
        function.handlers.on_copy_name = Some(Box::new(|| ()));
        assert_eq!(ids(&build_schema_tree_context_menu_items(&function)), vec!["copy-name"]);
    }

    #[test]
    fn safe_mode_hides_execute_sql_file() {
        let mut input = args(SchemaTreeNodeKind::Database);
        input.handlers.on_execute_sql_file = Some(Box::new(|| ()));
        assert_eq!(ids(&build_schema_tree_context_menu_items(&input)), vec!["execute-sql-file"]);
        input.safe_mode = true;
        assert!(ids(&build_schema_tree_context_menu_items(&input)).is_empty());
    }

    #[test]
    fn missing_handlers_are_skipped() {
        let input = args(SchemaTreeNodeKind::Table);
        let items = build_schema_tree_context_menu_items(&input);
        assert!(items.is_empty());
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::build_schema_tree_context_menu_items;
        let _: Option<crate::SchemaTreeNodeKind> = None;
        let _: Option<crate::SchemaTreeMenuItem> = None;
        let _: Option<crate::SchemaTreeAction> = None;
        let _: Option<crate::SchemaTreeContextMenuLabels> = None;
        let _: Option<crate::SchemaTreeContextMenuHandlers> = None;
        let _: Option<crate::BuildSchemaTreeContextMenuArgs> = None;
        let _: Option<crate::SchemaTreeNodeContextMenuPayload> = None;
        assert_eq!(crate::SchemaTreeAction::BatchExport.as_str(), "batch-export");
    }
}
