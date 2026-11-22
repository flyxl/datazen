//! The database-row context menu — React
//! `buildSchemaTreeContextMenuItems({ kind: 'database' })` with the handler
//! availability the hook decides in `handleDatabaseContextMenu`.
//!
//! The hook never passes export / import / batch-export handlers for a
//! database, so those three items are absent here even though the generic
//! builder knows how to render them. The trailing sections are separated in
//! the builder's order: migration tools, backup/restore, close, drop.

use super::{
    act, at, danger_item, item, separator, DriverCaps, MenuEntry, NavActionKind, NavTarget,
    ProductFeatures, ViewActionCaps,
};

#[allow(dead_code)] // VM mirror; supplied by the navigator host.
#[derive(Clone, Debug, Default)]
pub(crate) struct DatabaseMenuInput {
    pub connection_id: String,
    pub database: String,
    pub db_session_id: Option<String>,
    pub read_only: bool,
    pub safe_mode: bool,
    pub driver: DriverCaps,
    pub view: ViewActionCaps,
    pub features: ProductFeatures,
}

/// React `migrationToolItems` — three feature-gated entries, all optional.
#[allow(dead_code)] // menu helper; remove with the host.
pub(crate) fn migration_tool_items(features: ProductFeatures) -> Vec<MenuEntry> {
    let mut items = Vec::new();
    if features.data_transfer {
        items.push(item(
            "data-transfer",
            "common.dataTransfer",
            act(NavActionKind::DataTransfer),
        ));
    }
    if features.schema_diff {
        items.push(item(
            "compare-schema",
            "schemaTree.compareSchema",
            act(NavActionKind::CompareSchema),
        ));
    }
    if features.data_sync {
        items.push(item(
            "compare-data",
            "schemaTree.compareData",
            act(NavActionKind::CompareData),
        ));
    }
    items
}

/// React `buildSchemaTreeContextMenuItems` `case 'database'`.
#[allow(dead_code)] // menu builder; remove when the navigator host wires it.
pub(crate) fn build_database_menu(input: &DatabaseMenuInput) -> Vec<MenuEntry> {
    let session = input.db_session_id.clone().unwrap_or_default();
    let mut main: Vec<MenuEntry> = Vec::new();

    // React passes onRefresh only when a session exists.
    if !session.is_empty() {
        main.push(item(
            "refresh",
            "connWin.refresh",
            at(
                NavActionKind::RefreshDatabase,
                NavTarget {
                    connection_id: input.connection_id.clone(),
                    db_session_id: session.clone(),
                    database: input.database.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    }
    main.push(item(
        "new-query",
        "common.newQuery",
        at(
            NavActionKind::NewQuery,
            NavTarget {
                database: input.database.clone(),
                ..NavTarget::default()
            },
        ),
    ));
    main.push(item(
        "query-history",
        "main.ctx.queryHistory",
        at(
            NavActionKind::OpenQueryHistory,
            NavTarget {
                connection_id: input.connection_id.clone(),
                ..NavTarget::default()
            },
        ),
    ));
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
        "copy-database-name",
        "schemaTree.copyDatabaseName",
        at(
            NavActionKind::CopyDatabaseName,
            NavTarget {
                name: input.database.clone(),
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
    if input.driver.supports_create_schema {
        main.push(item(
            "create-schema",
            "common.createSchema",
            at(
                NavActionKind::CreateSchema,
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

    let mut backup: Vec<MenuEntry> = Vec::new();
    if input.driver.supports_backup {
        backup.push(item(
            "backup",
            "common.backupDatabase",
            at(
                NavActionKind::Backup,
                NavTarget {
                    id: input.connection_id.clone(),
                    database: input.database.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
        backup.push(item(
            "restore",
            "common.restoreDatabase",
            at(
                NavActionKind::Restore,
                NavTarget {
                    id: input.connection_id.clone(),
                    database: input.database.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    }
    if !backup.is_empty() {
        parts.push(separator());
        parts.extend(backup);
    }

    if !session.is_empty() {
        parts.push(separator());
        parts.push(item(
            "close-database-connection",
            "schemaTree.closeDatabaseConnection",
            at(
                NavActionKind::CloseDatabase,
                NavTarget {
                    connection_id: input.connection_id.clone(),
                    db_session_id: session.clone(),
                    database: input.database.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    }

    if !input.read_only && !session.is_empty() {
        parts.push(separator());
        parts.push(danger_item(
            "drop-database",
            "schemaTree.dropDatabase",
            at(
                NavActionKind::DropDatabase,
                NavTarget {
                    connection_id: input.connection_id.clone(),
                    db_session_id: session,
                    database: input.database.clone(),
                    ..NavTarget::default()
                },
            ),
        ));
    }
    parts
}
