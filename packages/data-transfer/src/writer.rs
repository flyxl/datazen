//! Bound DML. Identifiers come from inspected metadata; values never enter SQL.
use super::{error::TransferError, execute::ValueFormatter, model::ColumnMapping};
use datazen_driver_api::TableSchema;
use datazen_driver_api::{DatabaseDriver, Value};

pub fn bound_insert(
    driver: &dyn DatabaseDriver,
    source_table: &str,
    target_ref: &str,
    columns: &[&ColumnMapping],
    target_schema: &TableSchema,
    row: &[Option<Value>],
    formatter: &ValueFormatter<'_>,
) -> Result<(String, Vec<Value>), TransferError> {
    bound_insert_batch(
        driver,
        source_table,
        target_ref,
        columns,
        target_schema,
        &[row.to_vec()],
        formatter,
    )
}

/// Build one parameterized INSERT for a batch of projected rows.
///
/// Keeping the complete batch in one statement reduces target round-trips while
/// retaining bound values and the target driver's native placeholder/cast rules.
pub fn bound_insert_batch(
    driver: &dyn DatabaseDriver,
    source_table: &str,
    target_ref: &str,
    columns: &[&ColumnMapping],
    target_schema: &TableSchema,
    rows: &[Vec<Option<Value>>],
    formatter: &ValueFormatter<'_>,
) -> Result<(String, Vec<Value>), TransferError> {
    if columns.is_empty() || rows.is_empty() {
        return Err(TransferError::validation(
            "INSERT batch does not contain an active projection",
        ));
    }
    let parameter_count = rows.len().checked_mul(columns.len()).ok_or_else(|| {
        TransferError::validation("INSERT batch parameter count exceeds the supported range")
    })?;
    if parameter_count > driver.max_bound_parameters() {
        return Err(TransferError::validation(format!(
            "INSERT batch requires {parameter_count} bound parameters, exceeding the driver's {}-parameter limit",
            driver.max_bound_parameters()
        )));
    }
    let mut names = std::collections::HashSet::new();
    let mut target_types = Vec::with_capacity(columns.len());
    for binding in columns {
        if !names.insert(binding.target_column.as_str()) {
            return Err(TransferError::validation("duplicate target column"));
        }
        let target = target_schema
            .columns
            .iter()
            .find(|c| c.name == binding.target_column)
            .ok_or_else(|| {
                TransferError::validation(format!(
                    "target column '{}' not found",
                    binding.target_column
                ))
            })?;
        target_types.push(target.data_type.as_str());
    }
    let column_sql = columns
        .iter()
        .map(|c| driver.quote_ident(&c.target_column))
        .collect::<Vec<_>>()
        .join(", ");
    let has_explicit_identity = columns.iter().any(|mapping| {
        target_schema
            .columns
            .iter()
            .any(|column| column.name == mapping.target_column && column.is_auto_increment)
    });
    let identity_insert_clause = has_explicit_identity
        .then(|| driver.transfer_explicit_identity_insert_clause())
        .flatten()
        .map(|clause| format!(" {clause}"))
        .unwrap_or_default();

    let mut values_sql = Vec::with_capacity(rows.len());
    let mut parameters = Vec::with_capacity(rows.len() * columns.len());
    for row in rows {
        if row.len() != columns.len() {
            return Err(TransferError::validation(
                "INSERT row does not match active projection",
            ));
        }
        let mut placeholders = Vec::with_capacity(columns.len());
        for (column_index, (binding, value)) in columns.iter().zip(row).enumerate() {
            let parameter_index = parameters.len() + 1;
            placeholders.push(
                driver
                    .parameter_placeholder(parameter_index, Some(target_types[column_index]))
                    .map_err(|e| TransferError::validation(e.to_string()))?,
            );
            let value = match formatter {
                ValueFormatter::SameFamily => value.clone(),
                ValueFormatter::Ir {
                    tgt_adapter,
                    source_column_ir_types,
                } => {
                    let ir_type = source_column_ir_types
                        .get(source_table)
                        .and_then(|types| types.get(&binding.source_column))
                        .ok_or_else(|| {
                            TransferError::validation(format!(
                                "missing IR type for {source_table}.{}",
                                binding.source_column
                            ))
                        })?;
                    let transformed = tgt_adapter.transform_value(value, ir_type);
                    if value.is_some() && transformed.is_none() {
                        return Err(TransferError::validation(
                            "conversion discarded a non-null source value",
                        ));
                    }
                    transformed
                }
            };
            parameters.push(value.unwrap_or(Value::Null));
        }
        values_sql.push(format!("({})", placeholders.join(", ")));
    }

    Ok((
        format!(
            "INSERT INTO {target_ref} ({column_sql}){identity_insert_clause} VALUES {}",
            values_sql.join(", ")
        ),
        parameters,
    ))
}
