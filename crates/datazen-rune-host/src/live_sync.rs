//! Bounded, read-only SQLite Data Sync preview. It never creates an apply plan.
use std::collections::HashMap;
use std::sync::Arc;

use datazen::connection::application::ConnectionService;
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::migration::data_sync::filter_validation::resolve_key_contracts;
use datazen::migration::data_sync::keyset_source::DriverKeysetSource;
use datazen::migration::data_sync::{
    check_table_gate, compare_table_pages, DataSyncError, Row, RowPageSource, SyncOptions,
};
use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver, SyncKeyContract, SyncKeyValue, SyncSourceAdapter, Value,
};
use datazen_ui_api::native_transfer::{
    NativeSyncPreview, SqliteReadComparison, TransferEndpoint, NATIVE_SYNC_ROW_LIMIT,
};

use super::live_transfer::{driver, identity, profile, tables};

const FAMILY: &str = "sqlite";
const CATALOG: &str = "main";

/// SQLite does not expose a repeatable-read handle through the shared keyset
/// reader. A single SELECT per endpoint is exhaustive within this limit.
/// Reject larger inputs rather than compare only their prefixes or mixing pages
/// from different committed versions of either file.
struct SingleRead {
    inner: DriverKeysetSource,
    oversized: bool,
}

#[async_trait::async_trait]
impl RowPageSource for SingleRead {
    async fn next_page(
        &mut self,
        after: Option<&[Value]>,
        limit: u32,
    ) -> Result<Vec<Row>, DataSyncError> {
        if after.is_some() {
            return Ok(Vec::new());
        }
        let rows = self.inner.next_page(None, limit + 1).await?;
        if rows.len() > limit as usize {
            self.oversized = true;
            return Err(DataSyncError::validation(
                "SQLite preview input exceeds its row bound",
            ));
        }
        Ok(rows)
    }

    fn normalize_key(&self, key: &[Value]) -> Result<Vec<SyncKeyValue>, DataSyncError> {
        self.inner.normalize_key(key)
    }
}

pub(super) async fn preview(
    service: &ConnectionService,
    registry: &DriverRegistry,
    request: SqliteReadComparison,
) -> Result<NativeSyncPreview, String> {
    if request.source.connection_id == request.target.connection_id
        || request.source.identity == request.target.identity
    {
        return Err("Select two different SQLite files.".into());
    }
    let mut source = profile(service, &request.source.connection_id).await?;
    let mut target = profile(service, &request.target.connection_id).await?;
    source.read_only = true;
    target.read_only = true;
    let driver = driver(registry).await?;
    let source_handle = driver
        .connect(&source)
        .await
        .map_err(|_| "Could not reopen the source SQLite file read-only.")?;
    let result = async {
        let target_handle = driver
            .connect(&target)
            .await
            .map_err(|_| "Could not reopen the target SQLite file read-only.")?;
        let preview = compare(&driver, &source_handle, &target_handle, request).await;
        let _ = driver.disconnect(target_handle).await;
        preview
    }
    .await;
    let _ = driver.disconnect(source_handle).await;
    result
}

async fn compare(
    driver: &Arc<dyn DatabaseDriver>,
    source: &ConnectionHandle,
    target: &ConnectionHandle,
    request: SqliteReadComparison,
) -> Result<NativeSyncPreview, String> {
    for (handle, endpoint) in [(source, &request.source), (target, &request.target)] {
        if identity(driver.as_ref(), handle).await? != endpoint.identity {
            return Err("A selected SQLite file changed. Select it again.".into());
        }
        if !tables(driver.as_ref(), handle)
            .await?
            .iter()
            .any(|table| table.name == endpoint.table)
        {
            return Err("Both selected base tables must still exist.".into());
        }
    }
    let source_schema = driver
        .get_table_schema(source, &request.source.table, CATALOG, None)
        .await
        .map_err(|_| "Could not read the selected source table.")?;
    let target_schema = driver
        .get_table_schema(target, &request.target.table, CATALOG, None)
        .await
        .map_err(|_| "Could not read the selected target table.")?;
    let keys = check_table_gate(FAMILY, &source_schema, &target_schema)
        .into_result()
        .map_err(|error| error.to_string())?;
    let key_adapter: Arc<dyn SyncSourceAdapter> =
        Arc::new(datazen_driver_sqlite::SqliteSyncAdapter);
    let (source_keys, target_keys) = resolve_key_contracts(
        &keys,
        key_adapter.as_ref(),
        key_adapter.as_ref(),
        &source_schema,
        &target_schema,
        &request.source.table,
        &request.target.table,
    )
    .map_err(|_| "The selected primary key is not supported for Data Sync preview.")?;
    // Use a common projection by name, not the target's physical column order.
    let names: Vec<_> = source_schema
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let indexes = keys
        .iter()
        .map(|key| names.iter().position(|name| name == key))
        .collect::<Option<Vec<_>>>()
        .ok_or("The primary key is missing from the compared columns.")?;
    let open_read = |handle: &ConnectionHandle,
                     endpoint: &TransferEndpoint,
                     key_contracts: Vec<SyncKeyContract>| {
        DriverKeysetSource::new(
            driver.clone(),
            handle.clone(),
            endpoint.table.clone(),
            Some(CATALOG.to_owned()),
            None,
            names.clone(),
            keys.clone(),
            driver.quote_char(),
            FAMILY,
            key_adapter.clone(),
            key_contracts,
            None,
            HashMap::new(),
            None,
        )
        .map(|inner| SingleRead {
            inner,
            oversized: false,
        })
        .map_err(|_| "Could not assemble the read-only SQLite preview.")
    };
    let mut source_rows = open_read(source, &request.source, source_keys)?;
    let mut target_rows = open_read(target, &request.target, target_keys)?;
    let options = SyncOptions {
        insert: false,
        update: false,
        delete: false,
        batch_size: NATIVE_SYNC_ROW_LIMIT,
        ..Default::default()
    };
    let result = compare_table_pages(
        &request.source.table,
        &request.target.table,
        &indexes,
        &names,
        &options,
        &mut source_rows,
        &mut target_rows,
        None,
    )
    .await
    .map_err(|_| {
        if source_rows.oversized || target_rows.oversized {
            format!("Data Sync preview supports at most {NATIVE_SYNC_ROW_LIMIT} rows per table.")
        } else {
            // Do not expose raw driver errors, paths or data in the UI.
            "Could not compare the rows. Verify supported primary keys and small row values.".into()
        }
    })?;
    Ok(NativeSyncPreview {
        source_only: result.insert_count(),
        changed: result.update_count(),
        unchanged: result.unchanged_row_count(),
        target_only: result.delete_count(),
    })
}
