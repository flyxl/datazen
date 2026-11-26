//! Worker-only adapter for insert-only Data Transfer between two SQLite files.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use datazen::connection::application::ConnectionService;
use datazen::connection::domain::ConnectionScope;
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::migration::data_transfer::model::{
    ColumnMapping, Endpoint, TableExecutionOutcome, TableMappingStatus, TransferOptions,
};
use datazen::migration::data_transfer::{
    build_preview, enforce_transfer_pairing, execute_transfer_data, inspect_tables, TableMapping,
    TransferJob, TransferMode, ValueFormatter, WriteMode,
};
use datazen_driver_api::{
    ConnectionConfig, ConnectionHandle, DatabaseDriver, TableInfo, TableType,
};
use datazen_ui_api::native_transfer::{NativeTransferRequest, TransferCatalog, TransferChoice};

const MAIN: &str = "main";
const SQLITE: &str = "sqlite";

async fn profile(service: &ConnectionService, id: &str) -> Result<ConnectionConfig, String> {
    let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
    let config = service
        .get(&scope, id)
        .await
        .map_err(|_| "Could not read the saved transfer profile.")?
        .ok_or("The saved transfer profile no longer exists.")?;
    if config.database_type != SQLITE {
        return Err("Native Data Transfer currently supports SQLite files only.".into());
    }
    let existing = config.database.as_deref().is_some_and(|path| {
        !path.chars().any(|ch| matches!(ch, '?' | '#' | '%'))
            && std::path::Path::new(path).is_file()
    });
    if !existing {
        return Err("Choose an existing local SQLite file without URI options.".into());
    }
    Ok(config)
}

async fn driver(registry: &DriverRegistry) -> Result<Arc<dyn DatabaseDriver>, String> {
    registry
        .get(&SQLITE.to_owned())
        .await
        .ok_or_else(|| "The SQLite driver is unavailable.".into())
}

async fn identity(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
) -> Result<String, String> {
    driver
        .physical_database_identity(handle, MAIN)
        .await
        .map_err(|_| "Could not verify the SQLite file identity.")?
        .ok_or_else(|| "Could not verify the SQLite file identity.".into())
}

async fn tables(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
) -> Result<Vec<TableInfo>, String> {
    let mut tables = driver
        .get_tables(handle, MAIN, None)
        .await
        .map_err(|_| "Could not read the SQLite table catalog.")?;
    tables.retain(|table| matches!(table.table_type, TableType::Table));
    Ok(tables)
}

pub(super) async fn catalog(
    service: &ConnectionService,
    registry: &DriverRegistry,
    mut choice: TransferChoice,
) -> Result<TransferCatalog, String> {
    let mut config = profile(service, &choice.connection_id).await?;
    let read_only = config.read_only;
    config.read_only = true;
    let driver = driver(registry).await?;
    let handle = driver
        .connect(&config)
        .await
        .map_err(|_| "Could not open the SQLite file for preview.")?;
    let result = async {
        let identity = identity(driver.as_ref(), &handle).await?;
        let tables: Vec<_> = tables(driver.as_ref(), &handle)
            .await?
            .into_iter()
            .map(|table| table.name)
            .collect();
        if choice
            .table
            .as_ref()
            .is_some_and(|table| !tables.contains(table))
        {
            return Err("The selected table no longer exists.".into());
        }
        choice.table = choice.table.or_else(|| tables.first().cloned());
        let columns = if let Some(table) = &choice.table {
            driver
                .get_table_schema(&handle, table, MAIN, None)
                .await
                .map_err(|_| "Could not read the SQLite column catalog.")?
                .columns
                .into_iter()
                .map(|column| column.name)
                .collect()
        } else {
            Vec::new()
        };
        Ok(TransferCatalog {
            choice,
            tables,
            columns,
            identity,
            read_only,
        })
    }
    .await;
    let _ = driver.disconnect(handle).await;
    result
}

pub(super) async fn run(
    service: &ConnectionService,
    registry: &DriverRegistry,
    request: NativeTransferRequest,
) -> Result<u64, String> {
    if request.source.connection_id == request.target.connection_id
        || request.source.identity == request.target.identity
    {
        return Err("Select two different SQLite files.".into());
    }
    let target = profile(service, &request.target.connection_id).await?;
    if target.read_only {
        return Err("The target is read-only.".into());
    }
    let mut source = profile(service, &request.source.connection_id).await?;
    source.read_only = true;
    let driver = driver(registry).await?;
    let source_handle = driver
        .connect(&source)
        .await
        .map_err(|_| "Could not reopen the source SQLite file.")?;
    let result = async {
        // Check the target without affecting its journal or data, even when
        // the saved path has changed since the user reviewed the mapping.
        let mut check = target.clone();
        check.read_only = true;
        let preview = driver
            .connect(&check)
            .await
            .map_err(|_| "Could not reopen the target SQLite file.")?;
        let target_identity = identity(driver.as_ref(), &preview).await;
        let _ = driver.disconnect(preview).await;
        if identity(driver.as_ref(), &source_handle).await? != request.source.identity
            || target_identity? != request.target.identity
        {
            return Err("The selected SQLite file changed. Review the transfer again.".into());
        }

        let target_handle = driver
            .connect(&target)
            .await
            .map_err(|_| "Could not open the target SQLite file for writes.")?;
        let result = async {
            if identity(driver.as_ref(), &target_handle).await? != request.target.identity {
                return Err("The target SQLite file changed. Review the transfer again.".into());
            }
            execute(driver.as_ref(), &source_handle, &target_handle, request).await
        }
        .await;
        let _ = driver.disconnect(target_handle).await;
        result
    }
    .await;
    let _ = driver.disconnect(source_handle).await;
    result
}

async fn execute(
    driver: &dyn DatabaseDriver,
    source: &ConnectionHandle,
    target: &ConnectionHandle,
    request: NativeTransferRequest,
) -> Result<u64, String> {
    let source_tables = tables(driver, source).await?;
    let target_tables = tables(driver, target).await?;
    if !source_tables
        .iter()
        .any(|table| table.name == request.source.table)
        || !target_tables
            .iter()
            .any(|table| table.name == request.target.table)
    {
        return Err("Both selected tables must still exist.".into());
    }
    let source_schema = driver
        .get_table_schema(source, &request.source.table, MAIN, None)
        .await
        .map_err(|_| "Could not read the source table structure.")?;
    let target_schema = driver
        .get_table_schema(target, &request.target.table, MAIN, None)
        .await
        .map_err(|_| "Could not read the target table structure.")?;
    let mut seen_source = HashSet::new();
    let mut seen_target = HashSet::new();
    if request.columns.is_empty()
        || request.columns.iter().any(|(left, right)| {
            !seen_source.insert(left)
                || !seen_target.insert(right)
                || !source_schema
                    .columns
                    .iter()
                    .any(|column| &column.name == left)
                || !target_schema
                    .columns
                    .iter()
                    .any(|column| &column.name == right)
        })
    {
        return Err("Review the column mappings before transfer.".into());
    }

    let mut mapping = TableMapping::auto(request.source.table.clone());
    mapping.target_table = request.target.table.clone();
    mapping.column_mappings = request
        .columns
        .into_iter()
        .map(|(source_column, target_column)| ColumnMapping {
            source_column,
            target_column,
            skip: false,
            target_native_type: None,
        })
        .collect();
    let job = TransferJob {
        source: Endpoint {
            db_session_id: source.id.clone(),
            database: MAIN.into(),
            schema: None,
        },
        target: Some(Endpoint {
            db_session_id: target.id.clone(),
            database: MAIN.into(),
            schema: None,
        }),
        sql_file_target: None,
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![mapping],
        options: TransferOptions::default(),
    };
    let source_schemas = HashMap::from([(request.source.table, source_schema)]);
    let target_schemas = HashMap::from([(request.target.table, target_schema)]);
    let inspected = inspect_tables(
        &source_tables,
        &target_tables,
        &job.tables,
        &source_schemas,
        &target_schemas,
        job.mode,
        &HashMap::new(),
    );
    let pairing = enforce_transfer_pairing(SQLITE, SQLITE)
        .map_err(|_| "The SQLite transfer pairing is unavailable.")?;
    let plan = build_preview(&job, &inspected, &pairing, &source_schemas, true, None)
        .map_err(|_| "Could not validate the SQLite transfer plan.")?;
    if !plan.can_execute
        || !inspected
            .iter()
            .any(|table| table.enabled && table.status == TableMappingStatus::Matched)
    {
        return Err("The selected SQLite transfer plan cannot execute.".into());
    }
    let result = execute_transfer_data(
        driver,
        source,
        driver,
        target,
        &job,
        &inspected,
        &source_schemas,
        &ValueFormatter::SameFamily,
        None,
        false,
        None,
        None,
    )
    .await
    .map_err(|_| "SQLite Data Transfer failed before reporting a commit.")?;
    if result.tables.len() == 1
        && result.tables[0].outcome == Some(TableExecutionOutcome::Committed)
        && !result.cancelled
        && !result.partial
    {
        Ok(result.rows_inserted)
    } else if result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::Unknown))
    {
        Err("The commit outcome is unknown. Check the target before retrying.".into())
    } else {
        Err("The transfer did not commit. Check the mappings and table constraints.".into())
    }
}
