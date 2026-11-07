//! Resolve the table-atomic and row-chunk checkpoint contracts before writes.

use std::collections::HashMap;

use crate::data_transfer::model::{TableInspectResult, TableMappingStatus, TransferJob};
use crate::data_transfer::resume;
use datazen_driver_api::TableSchema;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};

pub(super) struct ResumePreflight {
    pub(super) table_boundary_reason: Option<String>,
    pub(super) chunk_reason: Option<String>,
}

impl ResumePreflight {
    pub(super) fn table_boundary_safe(&self) -> bool {
        self.table_boundary_reason.is_none()
    }
}

/// Inspect every selected relation before the executor can write. For a fresh
/// transfer, unknown target metadata disables replayable checkpoints while
/// allowing the ordinary writer to proceed. A requested resume token is
/// rejected before writes unless its target contract is still proven safe.
/// Source key/snapshot limitations disable only row chunks; an atomic
/// whole-table transaction may still be resumed at the table boundary when
/// every target is proven transactional.
pub(super) async fn inspect_resume_contract(
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    source_driver: &dyn DatabaseDriver,
    source_handle: &ConnectionHandle,
    target_driver: &dyn DatabaseDriver,
    target_handle: &ConnectionHandle,
) -> Result<ResumePreflight, String> {
    let target = job.database_target().map_err(|error| error.to_string())?;
    let sessions_are_distinct =
        job.source.db_session_id != target.db_session_id && source_handle.id != target_handle.id;
    let mut table_boundary_reason = (!sessions_are_distinct).then(|| {
        "source and target share a database session; resumable checkpoints are disabled".into()
    });
    let mut chunk_reason = table_boundary_reason.clone();
    let source_type = source_driver.driver_type();
    let target_type = target_driver.driver_type();

    for table in inspected.iter().filter(|table| table.enabled) {
        if table.status != TableMappingStatus::Matched || table.create_new {
            let reason = format!(
                "table '{}' is not a matched existing target; checkpoint safety is unverified",
                table.source_table
            );
            table_boundary_reason.get_or_insert_with(|| reason.clone());
            chunk_reason.get_or_insert(reason);
            continue;
        }

        let target_schema = match crate::data_transfer::metadata::load_table_schema(
            target_driver,
            target_handle,
            target,
            &table.target_table,
        )
        .await
        {
            Ok(schema) => schema,
            Err(error) => {
                let reason = format!(
                    "target table '{}' metadata could not be verified ({error}); resume tokens are disabled",
                    table.target_table
                );
                table_boundary_reason.get_or_insert_with(|| reason.clone());
                chunk_reason.get_or_insert(reason);
                continue;
            }
        };
        if target_schema.table_options.supports_consistent_snapshot != Some(true) {
            let reason = format!(
                "target table '{}' is not proven transactional (PostgreSQL ordinary table or MySQL InnoDB required); resume tokens are disabled",
                table.target_table
            );
            table_boundary_reason.get_or_insert_with(|| reason.clone());
            chunk_reason.get_or_insert(reason);
        }

        let Some(source_schema) = source_schemas.get(&table.source_table) else {
            chunk_reason.get_or_insert_with(|| {
                format!(
                    "source table '{}' metadata is unavailable for bounded keyset resume",
                    table.source_table
                )
            });
            continue;
        };
        if source_schema.table_options.supports_consistent_snapshot != Some(true) {
            chunk_reason.get_or_insert_with(|| {
                format!(
                    "source table '{}' has no verified stable read snapshot; only atomic whole-table resume can be used",
                    table.source_table
                )
            });
        }
        if !resume::supports_chunk_driver(&source_type)
            || !resume::supports_chunk_driver(&target_type)
        {
            chunk_reason.get_or_insert_with(|| {
                format!(
                    "bounded row resume requires PostgreSQL or MySQL source and target drivers; this transfer can use atomic whole-table resume only"
                )
            });
        }
        let recordset = job
            .tables
            .iter()
            .find(|mapping| mapping.source_table == table.source_table)
            .and_then(|mapping| mapping.recordset.as_ref());
        if let Err(error) = resume::resumable_primary_key(source_schema, recordset, &source_type) {
            chunk_reason.get_or_insert_with(|| {
                format!(
                    "bounded row resume is unavailable for source table '{}': {error}",
                    table.source_table
                )
            });
        }
    }

    Ok(ResumePreflight {
        table_boundary_reason,
        chunk_reason,
    })
}
