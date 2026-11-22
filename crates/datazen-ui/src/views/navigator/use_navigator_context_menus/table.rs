//! The table/view-row context menu — React
//! `buildSchemaTreeContextMenuItems({ kind: 'table' | 'view' })` with the
//! handler availability from `handleTableContextMenu`.
//!
//! Table order follows TablePlus: open → structure → ER → generate-sql →
//! new query → copy → (export) → danger. The hook supplies no export/import
//! handlers, so those items are absent. Views are a shorter menu with a single
//! `drop-view` danger item. The five SQL-generation entries form one submenu.

use super::{
    act, at, danger_item, dynamic_item, item, separator, submenu, MenuEntry, NavAction,
    NavActionKind, NavTarget, SqlKind, ViewActionCaps,
};

#[allow(dead_code)] // VM mirror; supplied by the navigator host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableKind {
    Table,
    View,
}

#[allow(dead_code)] // VM mirror; supplied by the navigator host.
#[derive(Clone, Debug)]
pub(crate) struct TableMenuInput {
    pub kind: TableKind,
    pub name: String,
    pub schema: Option<String>,
    pub database: String,
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: String,
    pub read_only: bool,
    pub safe_mode: bool,
    /// React `dbMeta?.supportsErDiagram !== false`.
    pub supports_er_diagram: bool,
    pub view: ViewActionCaps,
}

fn target(input: &TableMenuInput) -> NavTarget {
    NavTarget {
        connection_id: input.connection_id.clone(),
        db_session_id: input.db_session_id.clone(),
        database: input.database.clone(),
        schema: input.schema.clone().unwrap_or_default(),
        name: input.name.clone(),
        database_type: input.database_type.clone(),
        ..NavTarget::default()
    }
}

fn generate_action(input: &TableMenuInput, kind: SqlKind) -> NavAction {
    let mut payload = target(input);
    payload.sql_kind = kind;
    at(NavActionKind::GenerateSql, payload)
}

/// React `generateSqlSubmenu` — present only for tables with at least one
/// generator handler; the hook supplies all five.
#[allow(dead_code)] // menu helper; remove with the host.
pub(crate) fn generate_sql_submenu(input: &TableMenuInput) -> Option<MenuEntry> {
    if input.kind != TableKind::Table {
        return None;
    }
    let specs = [
        (SqlKind::Select, "generate-select", "schemaTree.generateSelect"),
        (SqlKind::Insert, "generate-insert", "schemaTree.generateInsert"),
        (SqlKind::Update, "generate-update", "schemaTree.generateUpdate"),
        (SqlKind::Delete, "generate-delete", "schemaTree.generateDelete"),
        (SqlKind::Ddl, "generate-ddl", "schemaTree.generateDdl"),
    ];
    let children = specs
        .into_iter()
        .map(|(kind, id, key)| item(id, key, generate_action(input, kind)))
        .collect();
    Some(submenu("generate-sql", "schemaTree.generateSql", children))
}

/// React `buildSchemaTreeContextMenuItems` `case 'table'` / `case 'view'`.
#[allow(dead_code)] // menu builder; remove when the navigator host wires it.
pub(crate) fn build_table_menu(input: &TableMenuInput) -> Vec<MenuEntry> {
    let is_table = input.kind == TableKind::Table;
    let mut main: Vec<MenuEntry> = vec![item(
        "open",
        "schemaTree.openTable",
        at(NavActionKind::OpenTable, target(input)),
    )];

    if is_table {
        if input.view.open_table_structure {
            main.push(item(
                "open-structure",
                "schemaTree.openStructure",
                at(NavActionKind::OpenTableStructure, target(input)),
            ));
        }
        if input.supports_er_diagram {
            // React's `schemaLabels.focusEr` is the empty string — there is no
            // locale key behind it, so it is dynamic text, not a key.
            main.push(dynamic_item(
                "focus-er".to_owned(),
                String::new(),
                at(NavActionKind::FocusEr, target(input)),
            ));
        }
        if let Some(submenu) = generate_sql_submenu(input) {
            main.push(submenu);
        }
    }

    // React's view case omits new-query; only the table case has it.
    if is_table {
        main.push(item(
            "new-query",
            "common.newQuery",
            at(NavActionKind::NewQuery, target(input)),
        ));
    }
    main.push(item(
        "copy-name",
        "common.copyName",
        at(NavActionKind::CopyName, target(input)),
    ));
    main.push(item(
        "copy-ddl",
        "common.copyDdl",
        at(NavActionKind::CopyDdl, target(input)),
    ));

    let writable = !input.read_only && !input.safe_mode;
    let mut danger: Vec<MenuEntry> = Vec::new();
    if is_table && writable {
        danger.push(danger_item(
            "truncate",
            "schemaTree.truncate",
            at(NavActionKind::TruncateTable, target(input)),
        ));
    }
    if writable {
        danger.push(danger_item(
            if is_table { "drop" } else { "drop-view" },
            if is_table {
                "schemaTree.drop"
            } else {
                "schemaTree.dropView"
            },
            at(
                NavActionKind::DropRelation,
                NavTarget {
                    is_view: !is_table,
                    ..target(input)
                },
            ),
        ));
    }
    if !danger.is_empty() {
        main.push(separator());
        main.extend(danger);
    }
    main
}
