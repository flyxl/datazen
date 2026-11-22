//! The schema-row context menu — React
//! `buildSchemaTreeContextMenuItems({ kind: 'schema' })` with the handler
//! availability decided by `handleSchemaContextMenu`.
//!
//! Two differences from the database menu are deliberate: `refresh` is always
//! offered (the schema handler does not need a session guard), and dropping a
//! system schema (`pg_*` or `information_schema`) is never offered.

use super::database::migration_tool_items;
use super::{
    act, at, danger_item, item, separator, MenuEntry, NavActionKind, NavTarget, ProductFeatures,
    ViewActionCaps,
};

#[allow(dead_code)] // VM mirror; supplied by the navigator host.
#[derive(Clone, Debug, Default)]
pub(crate) struct SchemaMenuInput {
    pub connection_id: String,
    pub database: String,
    pub schema: String,
    pub db_session_id: Option<String>,
    pub read_only: bool,
    pub safe_mode: bool,
    pub view: ViewActionCaps,
    pub features: ProductFeatures,
}

/// React's drop-schema gate: a session, not a system schema, and writable.
#[allow(dead_code)] // pure projection; consumed by the builder and tests.
pub(crate) fn can_drop_schema(schema: &str, has_session: bool, read_only: bool) -> bool {
    has_session && !schema.starts_with("pg_") && schema != "information_schema" && !read_only
}

/// React `buildSchemaTreeContextMenuItems` `case 'schema'`.
#[allow(dead_code)] // menu builder; remove when the navigator host wires it.
pub(crate) fn build_schema_menu(input: &SchemaMenuInput) -> Vec<MenuEntry> {
    let session = input.db_session_id.clone().unwrap_or_default();
    let mut main: Vec<MenuEntry> = vec![
        item(
            "refresh",
            "connWin.refresh",
            at(
                NavActionKind::RefreshSchema,
                NavTarget {
                    connection_id: input.connection_id.clone(),
                    db_session_id: session.clone(),
                    database: input.database.clone(),
                    schema: input.schema.clone(),
                    ..NavTarget::default()
                },
            ),
        ),
        item(
            "new-query",
            "common.newQuery",
            at(
                NavActionKind::NewQuery,
                NavTarget {
                    database: input.database.clone(),
                    ..NavTarget::default()
                },
            ),
        ),
        item(
            "query-history",
            "main.ctx.queryHistory",
            at(
                NavActionKind::OpenQueryHistory,
                NavTarget {
                    connection_id: input.connection_id.clone(),
                    ..NavTarget::default()
                },
            ),
        ),
    ];
    if !input.read_only && !input.safe_mode && input.view.open_sql_file {
        main.push(item(
            "execute-sql-file",
            "common.executeSqlFile",
            at(
                NavActionKind::ExecuteSqlFile,
                NavTarget {
                    connection_id: input.connection_id.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    }
    main.push(item(
        "copy-schema-name",
        "common.copyName",
        at(
            NavActionKind::CopyName,
            NavTarget {
                name: input.schema.clone(),
                ..NavTarget::default()
            },
        ),
    ));
    main.push(item(
        "view-er-diagram",
        "schemaTree.viewErDiagram",
        at(
            NavActionKind::OpenErDiagram,
            NavTarget {
                database: input.database.clone(),
                ..NavTarget::default()
            },
        ),
    ));
    if !input.read_only && input.view.create_table {
        main.push(item(
            "new-table",
            "common.newTable",
            at(
                NavActionKind::CreateTable,
                NavTarget {
                    connection_id: input.connection_id.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    }

    let mut parts = main;
    let sync = migration_tool_items(input.features);
    if !sync.is_empty() {
        parts.push(separator());
        parts.extend(sync);
    }

    if can_drop_schema(&input.schema, !session.is_empty(), input.read_only) {
        parts.push(separator());
        parts.push(danger_item(
            "drop-schema",
            "schemaTree.dropSchema",
            at(
                NavActionKind::DropSchema,
                NavTarget {
                    connection_id: input.connection_id.clone(),
                    db_session_id: session,
                    database: input.database.clone(),
                    schema: input.schema.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    }
    parts
}
