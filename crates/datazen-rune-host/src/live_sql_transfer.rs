//! Read-only review and single-table, insert-only PostgreSQL → MySQL transfer.
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use datazen::connection::application::ConnectionService;
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::migration::data_transfer::metadata::load_target_character_metadata;
use datazen::migration::data_transfer::model::{
    ColumnMapping, Endpoint, TableExecutionOutcome, TableMappingStatus, TransferOptions,
};
use datazen::migration::data_transfer::plan_store::fingerprint_schemas;
use datazen::migration::data_transfer::structure::enrich_source_types;
use datazen::migration::data_transfer::transfer::adapter_registry::SyncAdapterRegistry;
use datazen::migration::data_transfer::{
    build_preview, column_ir_types_by_source, enforce_transfer_pairing, execute_transfer_data,
    inspect_tables, source_schema_to_target_ir, TableInspectResult, TableMapping, TransferJob,
    TransferMode, TransferPreviewAdapters, ValueFormatter, WriteMode,
};
use datazen_driver_api::{
    ConnectionConfig, ConnectionHandle, DatabaseDriver, SyncSourceAdapter, SyncTargetAdapter,
    TableInfo, TableSchema, TableType,
};
use datazen_ui_api::native_transfer::{
    NativeSqlScope, NativeTransferRequest, TransferCatalog, TransferChoice, TransferEndpoint,
};

fn scope(config: &ConnectionConfig) -> Result<NativeSqlScope, String> {
    let required = |value: &Option<String>, error: &str| {
        value
            .clone()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| error.to_owned())
    };
    let schema = match config.database_type.as_str() {
        "postgresql" => Some(
            config
                .schema
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("public")
                .to_owned(),
        ),
        "mysql" => None,
        _ => return Err("This connection is not supported by native SQL Data Transfer.".into()),
    };
    Ok(NativeSqlScope {
        engine: config.database_type.clone(),
        database: required(
            &config.database,
            "Save an explicit database in the connection profile.",
        )?,
        schema,
    })
}

async fn driver(
    registry: &DriverRegistry,
    scope: &NativeSqlScope,
) -> Result<Arc<dyn DatabaseDriver>, String> {
    registry
        .get(&scope.engine)
        .await
        .ok_or_else(|| "The transfer driver is unavailable.".into())
}

async fn identity(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    scope: &NativeSqlScope,
) -> Result<String, String> {
    driver
        .physical_database_identity(handle, &scope.database)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| "Could not verify the physical database identity.".into())
}

async fn tables(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    scope: &NativeSqlScope,
) -> Result<Vec<TableInfo>, String> {
    let mut tables = driver
        .get_tables(handle, &scope.database, scope.schema.as_deref())
        .await
        .map_err(|_| "Could not read the selected table catalog.")?;
    tables.retain(|table| {
        matches!(table.table_type, TableType::Table)
            && scope
                .schema
                .as_ref()
                .is_none_or(|name| table.schema.as_ref() == Some(name))
    });
    tables.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(tables)
}

async fn schema(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    scope: &NativeSqlScope,
    table: &str,
) -> Result<TableSchema, String> {
    driver
        .get_table_schema(handle, table, &scope.database, scope.schema.as_deref())
        .await
        .map_err(|_| "Could not read the selected table structure.".into())
}

pub(super) async fn catalog(
    registry: &DriverRegistry,
    mut config: ConnectionConfig,
    mut choice: TransferChoice,
) -> Result<TransferCatalog, String> {
    let scope = scope(&config)?;
    let read_only = config.read_only;
    config.read_only = true;
    let driver = driver(registry, &scope).await?;
    let opened = driver
        .connect(&config)
        .await
        .map_err(|_| "Could not open the database for catalog review.")?;
    let result = async {
        let identity = identity(driver.as_ref(), &opened, &scope).await?;
        let tables: Vec<_> = tables(driver.as_ref(), &opened, &scope)
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
        let columns = match &choice.table {
            Some(table) => schema(driver.as_ref(), &opened, &scope, table)
                .await?
                .columns
                .into_iter()
                .map(|c| c.name)
                .collect(),
            None => Vec::new(),
        };
        Ok(TransferCatalog {
            choice,
            tables,
            columns,
            identity,
            read_only,
            sql_scope: Some(scope),
        })
    }
    .await;
    let _ = driver.disconnect(opened).await;
    result
}

struct Side {
    scope: NativeSqlScope,
    config: ConnectionConfig,
    driver: Arc<dyn DatabaseDriver>,
}

impl Side {
    async fn load(
        service: &ConnectionService,
        registry: &DriverRegistry,
        reviewed: &TransferEndpoint,
    ) -> Result<Self, String> {
        let config = super::live_transfer::read_profile(service, &reviewed.connection_id).await?;
        let scope = scope(&config)?;
        if Some(&scope) != reviewed.sql_scope.as_ref() {
            return Err("The selected connection scope changed. Select the table again.".into());
        }
        let driver = driver(registry, &scope).await?;
        Ok(Self {
            scope,
            config,
            driver,
        })
    }

    async fn open(
        &self,
        endpoint: &TransferEndpoint,
        read_only: bool,
    ) -> Result<ConnectionHandle, String> {
        if !read_only && self.config.read_only {
            return Err("The target is read-only.".into());
        }
        let mut config = self.config.clone();
        config.read_only = read_only;
        let handle = self
            .driver
            .connect(&config)
            .await
            .map_err(|_| "Could not open the selected database.")?;
        if identity(self.driver.as_ref(), &handle, &self.scope)
            .await
            .as_deref()
            != Ok(endpoint.identity.as_str())
        {
            let _ = self.driver.disconnect(handle).await;
            return Err(
                "The selected database identity could not be reverified. Select it again.".into(),
            );
        }
        Ok(handle)
    }

    fn endpoint(&self, handle: &ConnectionHandle) -> Endpoint {
        Endpoint {
            db_session_id: handle.id.clone(),
            database: self.scope.database.clone(),
            schema: self.scope.schema.clone(),
        }
    }
}

struct Plan {
    job: TransferJob,
    inspected: Vec<TableInspectResult>,
    source_schemas: HashMap<String, TableSchema>,
    src_adapter: Arc<dyn SyncSourceAdapter>,
    tgt_adapter: Arc<dyn SyncTargetAdapter>,
    fingerprint: String,
}

async fn prepare(
    source: &Side,
    sh: &ConnectionHandle,
    target: &Side,
    th: &ConnectionHandle,
    request: &NativeTransferRequest,
) -> Result<Plan, String> {
    if target.config.read_only {
        return Err("The target is read-only.".into());
    }
    let src = source.driver.as_ref();
    let tgt = target.driver.as_ref();
    let source_tables = tables(src, sh, &source.scope).await?;
    let target_tables = tables(tgt, th, &target.scope).await?;
    if !source_tables.iter().any(|t| t.name == request.source.table)
        || !target_tables.iter().any(|t| t.name == request.target.table)
    {
        return Err("Both selected tables must still exist.".into());
    }
    let source_table = schema(src, sh, &source.scope, &request.source.table).await?;
    let target_table = schema(tgt, th, &target.scope, &request.target.table).await?;
    if target_table.table_options.supports_consistent_snapshot != Some(true) {
        return Err("The selected MySQL table is not proven transactional.".into());
    }
    let mut seen_source = HashSet::new();
    let mut seen_target = HashSet::new();
    if request.columns.is_empty()
        || request.columns.iter().any(|(left, right)| {
            !seen_source.insert(left)
                || !seen_target.insert(right)
                || !source_table.columns.iter().any(|c| &c.name == left)
                || !target_table.columns.iter().any(|c| &c.name == right)
        })
    {
        return Err("Review the column mappings before transfer.".into());
    }
    let mut mapping = TableMapping::auto(request.source.table.clone());
    mapping.target_table = request.target.table.clone();
    mapping.column_mappings = request
        .columns
        .iter()
        .map(|(left, right)| ColumnMapping {
            source_column: left.clone(),
            target_column: right.clone(),
            skip: false,
            target_native_type: None,
        })
        .collect();
    let job = TransferJob {
        source: source.endpoint(sh),
        target: Some(target.endpoint(th)),
        sql_file_target: None,
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![mapping],
        options: TransferOptions::default(),
    };
    let adapters = SyncAdapterRegistry::new();
    adapters.ensure_pair(&source.scope.engine, &target.scope.engine)?;
    let src_adapter = adapters
        .get_source(&source.scope.engine)
        .ok_or("The source transfer adapter is unavailable.")?;
    let tgt_adapter = adapters
        .get_target(&target.scope.engine)
        .ok_or("The target transfer adapter is unavailable.")?;
    let mut source_schemas = HashMap::from([(request.source.table.clone(), source_table)]);
    enrich_source_types(
        src_adapter.as_ref(),
        src,
        sh,
        &job.source,
        &mut source_schemas,
    )
    .await
    .map_err(|_| "Could not verify the source column types.")?;
    let schema_fingerprint = fingerprint_schemas([
        (
            "source".into(),
            source_schemas.get(&request.source.table).cloned(),
        ),
        ("target".into(), Some(target_table.clone())),
    ])
    .map_err(|_| "Could not identify the reviewed table structures.")?;
    let targets = HashMap::from([(request.target.table.clone(), target_table)]);
    let mut inspected = inspect_tables(
        &source_tables,
        &target_tables,
        &job.tables,
        &source_schemas,
        &targets,
        job.mode,
        &HashMap::new(),
    );
    let metadata = load_target_character_metadata(
        tgt_adapter.as_ref(),
        tgt,
        th,
        &target.endpoint(th),
        &request.target.table,
    )
    .await
    .map_err(|_| "Could not verify target character metadata.")?
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    let fingerprint = serde_json::to_string(&(schema_fingerprint, &metadata))
        .map_err(|_| "Could not identify the reviewed table structures.")?;
    if request
        .reviewed_schema
        .as_ref()
        .is_some_and(|reviewed| reviewed != &fingerprint)
    {
        return Err("A table structure changed. Review the transfer again.".into());
    }
    for entry in &mut inspected {
        for (name, (charset, collation)) in &metadata {
            if let Some(column) = entry.target_column_types.get_mut(name) {
                column.character_set.clone_from(charset);
                column.collation.clone_from(collation);
            }
        }
    }
    let pairing = enforce_transfer_pairing(&source.scope.engine, &target.scope.engine)
        .map_err(|_| "The SQL transfer pairing is unavailable.")?;
    let preview = build_preview(
        &job,
        &inspected,
        &pairing,
        &source_schemas,
        true,
        Some(TransferPreviewAdapters {
            src_adapter: src_adapter.as_ref(),
            tgt_adapter: tgt_adapter.as_ref(),
        }),
    )
    .map_err(|e| e.to_string())?;
    if !preview.can_execute
        || !inspected
            .iter()
            .any(|t| t.enabled && t.status == TableMappingStatus::Matched)
    {
        return Err(preview
            .block_reason
            .unwrap_or_else(|| "The transfer plan cannot execute.".into()));
    }
    Ok(Plan {
        job,
        inspected,
        source_schemas,
        src_adapter,
        tgt_adapter,
        fingerprint,
    })
}

async fn sides(
    service: &ConnectionService,
    registry: &DriverRegistry,
    request: &NativeTransferRequest,
) -> Result<(Side, Side), String> {
    if request.source.connection_id == request.target.connection_id
        || request.source.identity == request.target.identity
    {
        return Err("Select two different databases.".into());
    }
    if request.source.sql_scope.as_ref().map(|s| s.engine.as_str()) != Some("postgresql")
        || request.target.sql_scope.as_ref().map(|s| s.engine.as_str()) != Some("mysql")
    {
        return Err("Server transfer currently supports PostgreSQL to MySQL only.".into());
    }
    Ok((
        Side::load(service, registry, &request.source).await?,
        Side::load(service, registry, &request.target).await?,
    ))
}

pub(super) async fn review(
    service: &ConnectionService,
    registry: &DriverRegistry,
    request: NativeTransferRequest,
) -> Result<String, String> {
    let (source, target) = sides(service, registry, &request).await?;
    let sh = source.open(&request.source, true).await?;
    let result = async {
        let th = target.open(&request.target, true).await?;
        let result = prepare(&source, &sh, &target, &th, &request)
            .await
            .map(|plan| plan.fingerprint);
        let _ = target.driver.disconnect(th).await;
        result
    }
    .await;
    let _ = source.driver.disconnect(sh).await;
    result
}

pub(super) async fn run(
    service: &ConnectionService,
    registry: &DriverRegistry,
    request: NativeTransferRequest,
) -> Result<u64, String> {
    if request.reviewed_schema.is_none() {
        return Err("Review the transfer before inserting rows.".into());
    }
    // Recheck on a non-writing connection before opening the writable target.
    review(service, registry, request.clone()).await?;
    let (source, target) = sides(service, registry, &request).await?;
    let sh = source.open(&request.source, true).await?;
    let result = async {
        let th = target.open(&request.target, false).await?;
        let result = async {
            let plan = prepare(&source, &sh, &target, &th, &request).await?;
            let source_table = plan
                .source_schemas
                .get(&request.source.table)
                .ok_or("Source table structure is unavailable.")?;
            let ir = source_schema_to_target_ir(
                plan.src_adapter.as_ref(),
                source_table,
                None,
                &request.target.table,
            );
            let ir_types =
                HashMap::from([(request.source.table.clone(), column_ir_types_by_source(&ir))]);
            let formatter = ValueFormatter::Ir {
                tgt_adapter: plan.tgt_adapter.as_ref(),
                source_column_ir_types: &ir_types,
            };
            let result = execute_transfer_data(
                source.driver.as_ref(),
                &sh,
                target.driver.as_ref(),
                &th,
                &plan.job,
                &plan.inspected,
                &plan.source_schemas,
                &formatter,
                None,
                false,
                None,
                None,
            )
            .await
            .map_err(|_| "Data Transfer failed before reporting a commit.")?;
            if result.tables.len() == 1
                && result.tables[0].outcome == Some(TableExecutionOutcome::Committed)
                && !result.cancelled
                && !result.partial
            {
                Ok(result.rows_inserted)
            } else if result
                .tables
                .iter()
                .any(|t| t.outcome == Some(TableExecutionOutcome::Unknown))
            {
                Err("The commit outcome is unknown. Check the target before retrying.".into())
            } else {
                Err("The transfer did not commit. Check the mappings and table constraints.".into())
            }
        }
        .await;
        let _ = target.driver.disconnect(th).await;
        result
    }
    .await;
    let _ = source.driver.disconnect(sh).await;
    result
}
