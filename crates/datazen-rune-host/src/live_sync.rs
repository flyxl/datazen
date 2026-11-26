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
use datazen_driver_api::{DatabaseDriver, SyncKeyContract, SyncKeyValue, SyncSourceAdapter, Value};
use datazen_ui_api::native_transfer::{
    NativeSyncPreview, SqliteReadComparison, NATIVE_SYNC_ROW_LIMIT,
};

use super::live_transfer::{with_read_tables, ReadTable};

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
    with_read_tables(service, registry, request, compare).await
}

async fn compare(
    driver: &Arc<dyn DatabaseDriver>,
    source: ReadTable<'_>,
    target: ReadTable<'_>,
) -> Result<NativeSyncPreview, String> {
    let keys = check_table_gate(FAMILY, &source.schema, &target.schema)
        .into_result()
        .map_err(|error| error.to_string())?;
    let key_adapter: Arc<dyn SyncSourceAdapter> =
        Arc::new(datazen_driver_sqlite::SqliteSyncAdapter);
    let (source_keys, target_keys) = resolve_key_contracts(
        &keys,
        key_adapter.as_ref(),
        key_adapter.as_ref(),
        &source.schema,
        &target.schema,
        &source.schema.table_name,
        &target.schema.table_name,
    )
    .map_err(|_| "The selected primary key is not supported for Data Sync preview.")?;
    // Use a common projection by name, not the target's physical column order.
    let names: Vec<_> = source
        .schema
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let indexes = keys
        .iter()
        .map(|key| names.iter().position(|name| name == key))
        .collect::<Option<Vec<_>>>()
        .ok_or("The primary key is missing from the compared columns.")?;
    let open_read = |table: &ReadTable, key_contracts: Vec<SyncKeyContract>| {
        DriverKeysetSource::new(
            driver.clone(),
            table.handle.clone(),
            table.schema.table_name.clone(),
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
    let mut source_rows = open_read(&source, source_keys)?;
    let mut target_rows = open_read(&target, target_keys)?;
    let options = SyncOptions {
        insert: false,
        update: false,
        delete: false,
        batch_size: NATIVE_SYNC_ROW_LIMIT,
        ..Default::default()
    };
    let result = compare_table_pages(
        &source.schema.table_name,
        &target.schema.table_name,
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
