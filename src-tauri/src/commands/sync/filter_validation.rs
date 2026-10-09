//! Cross-endpoint validation for structured Data Sync filters and key ranges.

use super::super::error::CommandError;
use crate::data_sync::{quote_ident_sql, DataSyncError, SyncSourceFilter};
use crate::db::TableSchema;
use datazen_driver_api::{DatabaseDriver, SyncKeyContract, SyncSourceAdapter};

fn sqlserver_filter_type_uses_collation(data_type: &str) -> Option<bool> {
    let normalized = data_type
        .trim()
        .to_ascii_lowercase()
        .replace(['[', ']'], "");
    let base_type = normalized
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .split(['(', ' ', ','])
        .next()
        .unwrap_or_default();
    match base_type {
        "char" | "nchar" | "varchar" | "nvarchar" | "text" | "ntext" | "sysname" => Some(true),
        "bit" | "tinyint" | "smallint" | "int" | "bigint" | "decimal" | "numeric" | "money"
        | "smallmoney" | "float" | "real" | "date" | "time" | "datetime" | "smalldatetime"
        | "datetime2" | "datetimeoffset" | "uniqueidentifier" | "binary" | "varbinary"
        | "timestamp" | "rowversion" | "image" | "xml" | "geography" | "geometry"
        | "hierarchyid" => Some(false),
        // User-defined aliases and sql_variant can carry string semantics.
        // Their base storage type is not represented in TableSchema.
        _ => None,
    }
}

fn validate_sqlserver_filter_collations(
    filter: &SyncSourceFilter,
    source_driver: &dyn DatabaseDriver,
    target_driver: &dyn DatabaseDriver,
    source_schema: &TableSchema,
    target_schema: &TableSchema,
    source_table: &str,
) -> Result<(), CommandError> {
    let source_is_sqlserver =
        crate::transfer::pairing::normalize_sync_family(&source_driver.sync_family())
            == "sqlserver";
    let target_is_sqlserver =
        crate::transfer::pairing::normalize_sync_family(&target_driver.sync_family())
            == "sqlserver";
    if !source_is_sqlserver && !target_is_sqlserver {
        return Ok(());
    }

    for column in filter
        .comparison_columns(source_schema)
        .map_err(|error| CommandError::Validation(error.to_string()))?
    {
        for (is_sqlserver, side, schema) in [
            (source_is_sqlserver, "source", source_schema),
            (target_is_sqlserver, "target", target_schema),
        ] {
            if !is_sqlserver {
                continue;
            }
            let data_type = schema
                .columns
                .iter()
                .find(|candidate| candidate.name == column)
                .map(|candidate| candidate.data_type.as_str())
                .ok_or_else(|| {
                    CommandError::Validation(format!(
                        "{source_table}: SQL Server {side} filter column '{column}' is missing from inspected schema"
                    ))
                })?;
            match sqlserver_filter_type_uses_collation(data_type) {
                Some(false) => {}
                Some(true) => {
                    return Err(CommandError::Validation(format!(
                        "{source_table}: SQL Server {side} filter on text column '{column}' cannot be compared safely because default/per-column collation parity is not represented or verified by Data Sync"
                    )));
                }
                None => {
                    return Err(CommandError::Validation(format!(
                        "{source_table}: SQL Server {side} filter on column '{column}' (type '{data_type}') cannot be compared safely because Data Sync cannot verify whether this type uses text collation semantics"
                    )));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_filter_schemas(
    filter: &SyncSourceFilter,
    source_schema: &TableSchema,
    target_schema: &TableSchema,
    source_table: &str,
    target_table: &str,
) -> Result<(), CommandError> {
    filter
        .validate(source_schema)
        .map_err(|error| CommandError::Validation(error.to_string()))?;
    filter.validate(target_schema).map_err(|error| {
        CommandError::Validation(format!(
            "{source_table}: sync filter is not valid for target '{target_table}': {error}"
        ))
    })
}

pub(crate) fn resolve_key_contracts(
    pk_columns: &[String],
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncSourceAdapter,
    source_schema: &TableSchema,
    target_schema: &TableSchema,
    source_table: &str,
    target_table: &str,
) -> Result<(Vec<SyncKeyContract>, Vec<SyncKeyContract>), CommandError> {
    let mut src_contracts = Vec::with_capacity(pk_columns.len());
    let mut tgt_contracts = Vec::with_capacity(pk_columns.len());
    for pk in pk_columns {
        let source_column = source_schema
            .columns
            .iter()
            .find(|column| &column.name == pk)
            .ok_or_else(|| CommandError::Validation(format!("missing key column {pk}")))?;
        let target_column = target_schema
            .columns
            .iter()
            .find(|column| &column.name == pk)
            .ok_or_else(|| {
                CommandError::Validation(format!(
                    "target key column {pk} is missing; compare again"
                ))
            })?;
        let source_contract = src_adapter
            .sync_key_contract(source_column)
            .map_err(|reason| {
                CommandError::Validation(format!("{source_table}: source key '{pk}': {reason}"))
            })?;
        let target_contract = tgt_adapter
            .sync_key_contract(target_column)
            .map_err(|reason| {
                CommandError::Validation(format!("{target_table}: target key '{pk}': {reason}"))
            })?;
        if source_contract != target_contract {
            return Err(CommandError::Validation(format!(
                "{source_table}: key '{pk}' has incompatible source/target equality or ordering contract (source={source_contract:?}, target={target_contract:?})"
            )));
        }
        src_contracts.push(source_contract);
        tgt_contracts.push(target_contract);
    }
    Ok((src_contracts, tgt_contracts))
}

/// Ensure a bounded source filter has compatible key semantics, SQL ordering,
/// and parameter representations on both endpoints before plan construction.
pub(crate) fn validate_filter_endpoints(
    filter: &SyncSourceFilter,
    pk_columns: &[String],
    src_driver: &dyn DatabaseDriver,
    tgt_driver: &dyn DatabaseDriver,
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncSourceAdapter,
    source_schema: &TableSchema,
    target_schema: &TableSchema,
    src_contracts: &[SyncKeyContract],
    tgt_contracts: &[SyncKeyContract],
    source_table: &str,
) -> Result<(), CommandError> {
    validate_sqlserver_filter_collations(
        filter,
        src_driver,
        tgt_driver,
        source_schema,
        target_schema,
        source_table,
    )?;
    let source_key_order_expressions = pk_columns
        .iter()
        .zip(src_contracts)
        .map(|(column, contract)| {
            src_adapter.sync_key_order_expression(
                &quote_ident_sql(column, src_driver.quote_char()),
                contract,
            )
        })
        .collect::<Vec<_>>();
    let target_key_order_expressions = pk_columns
        .iter()
        .zip(tgt_contracts)
        .map(|(column, contract)| {
            tgt_adapter.sync_key_order_expression(
                &quote_ident_sql(column, tgt_driver.quote_char()),
                contract,
            )
        })
        .collect::<Vec<_>>();
    if filter
        .has_tuple_range()
        .map_err(|error| CommandError::Validation(error.to_string()))?
        && source_key_order_expressions != target_key_order_expressions
    {
        return Err(CommandError::Validation(format!(
            "{source_table}: source and target drivers provide different SQL ordering expressions for the composite key; tuple range is unsupported"
        )));
    }
    for (driver, side, adapter, table_schema, contracts, key_order_expressions) in [
        (
            src_driver,
            "source",
            src_adapter,
            source_schema,
            src_contracts,
            &source_key_order_expressions,
        ),
        (
            tgt_driver,
            "target",
            tgt_adapter,
            target_schema,
            tgt_contracts,
            &target_key_order_expressions,
        ),
    ] {
        filter
            .validate_tuple_range_order(|column, value| {
                let index = pk_columns
                    .iter()
                    .position(|candidate| candidate == column)
                    .ok_or_else(|| format!("key '{column}' has no verified key contract"))?;
                adapter.normalize_sync_key(&Some(value.clone()), &contracts[index])
            })
            .map_err(|error| {
                CommandError::Validation(format!(
                    "{source_table}: {side} driver cannot order sync tuple range: {error}"
                ))
            })?;
        filter
            .build_where_typed_with_key_order(
                driver.quote_char(),
                1,
                (pk_columns.len() == 1).then(|| pk_columns[0].as_str()),
                Some((pk_columns, key_order_expressions)),
                |column, value| {
                    let index = pk_columns
                        .iter()
                        .position(|candidate| candidate == column)
                        .ok_or_else(|| {
                            DataSyncError::validation(format!(
                                "recordset key '{column}' has no verified key contract"
                            ))
                        })?;
                    adapter
                        .sync_key_seek_value(value, &contracts[index])
                        .map_err(DataSyncError::validation)
                },
                |column| {
                    table_schema
                        .columns
                        .iter()
                        .find(|candidate| candidate.name == column)
                        .map(|candidate| candidate.data_type.clone())
                },
                |index, data_type| {
                    driver
                        .parameter_placeholder(index, data_type)
                        .map_err(|error| DataSyncError::validation(error.to_string()))
                },
            )
            .map_err(|error| {
                CommandError::Validation(format!(
                    "{source_table}: {side} driver cannot execute sync filter: {error}"
                ))
            })?;
    }
    Ok(())
}
