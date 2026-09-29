//! SQL Server catalog reads used by schema diff, sync and transfer.

use datazen_driver_api::{
    CheckConstraint, ColumnSchema, DriverError, ForeignKeyDeferrability, ForeignKeyInfo, IndexInfo,
    QueryResult, Value,
};
use std::collections::{HashMap, HashSet};

fn catalog_prefix(database: &str) -> String {
    let database = database.trim();
    if database.is_empty() {
        String::new()
    } else {
        format!("[{}].", database.replace(']', "]]"))
    }
}

fn selected_table(catalog: &str) -> String {
    format!(
        "JOIN {catalog}sys.tables t ON t.object_id = o.object_id \
         JOIN {catalog}sys.schemas s ON s.schema_id = t.schema_id \
         WHERE s.name = @P1 AND t.name = @P2"
    )
}

pub(crate) fn columns_sql(database: &str) -> String {
    let catalog = catalog_prefix(database);
    format!(
        "SELECT c.name AS column_name, \
         CASE \
           WHEN tp.is_user_defined = 1 THEN QUOTENAME(tp_schema.name) + N'.' + QUOTENAME(tp.name) \
           WHEN tp.name IN ('nvarchar', 'nchar') THEN CONCAT(tp.name, '(', CASE WHEN c.max_length = -1 THEN 'max' ELSE CONVERT(varchar(10), c.max_length / 2) END, ')') \
           WHEN tp.name IN ('varchar', 'char', 'varbinary', 'binary') THEN CONCAT(tp.name, '(', CASE WHEN c.max_length = -1 THEN 'max' ELSE CONVERT(varchar(10), c.max_length) END, ')') \
           WHEN tp.name IN ('decimal', 'numeric') THEN CONCAT(tp.name, '(', c.precision, ',', c.scale, ')') \
           WHEN tp.name IN ('time', 'datetime2', 'datetimeoffset') THEN CONCAT(tp.name, '(', c.scale, ')') \
           WHEN tp.name = 'float' THEN CONCAT(tp.name, '(', c.precision, ')') \
           ELSE tp.name \
         END AS data_type, \
         c.is_nullable, c.is_identity, dc.definition AS default_value, \
         CAST(ep.value AS nvarchar(max)) AS comment, \
         CAST(CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS bit) AS is_pk, \
         ISNULL(pk.key_ordinal, 0) AS pk_ordinal, \
         CAST(CASE WHEN cc.object_id IS NULL THEN 0 ELSE 1 END AS bit) AS is_computed, \
         c.generated_always_type, c.is_filestream, c.is_sparse, c.is_column_set, \
         CAST(CASE WHEN c.default_object_id <> 0 AND dc.object_id IS NULL THEN 1 ELSE 0 END AS bit) AS has_legacy_default, \
         CAST(CASE WHEN tp.name IN ('timestamp', 'rowversion') THEN 1 ELSE 0 END AS bit) AS is_rowversion, \
         CAST(CASE WHEN ic.object_id IS NOT NULL AND \
           (TRY_CONVERT(decimal(38,0), ic.seed_value) <> 1 OR TRY_CONVERT(decimal(38,0), ic.increment_value) <> 1) \
           THEN 1 ELSE 0 END AS bit) AS has_nondefault_identity, \
         c.collation_name AS column_collation, \
         CAST(DATABASEPROPERTYEX(CASE WHEN @P3 = N'' THEN DB_NAME() ELSE @P3 END, 'Collation') AS nvarchar(128)) AS database_collation \
         FROM {catalog}sys.columns c \
         JOIN {catalog}sys.objects o ON o.object_id = c.object_id \
         JOIN {catalog}sys.types tp ON tp.user_type_id = c.user_type_id \
         LEFT JOIN {catalog}sys.schemas tp_schema ON tp_schema.schema_id = tp.schema_id \
         JOIN {catalog}sys.schemas s ON s.schema_id = o.schema_id \
         LEFT JOIN {catalog}sys.default_constraints dc ON dc.parent_object_id = c.object_id AND dc.parent_column_id = c.column_id \
         LEFT JOIN {catalog}sys.extended_properties ep ON ep.major_id = c.object_id AND ep.minor_id = c.column_id AND ep.name = 'MS_Description' \
         LEFT JOIN {catalog}sys.computed_columns cc ON cc.object_id = c.object_id AND cc.column_id = c.column_id \
         LEFT JOIN {catalog}sys.identity_columns ic ON ic.object_id = c.object_id AND ic.column_id = c.column_id \
         LEFT JOIN ( \
           SELECT ix.object_id, ix.column_id, ix.key_ordinal \
           FROM {catalog}sys.index_columns ix \
           JOIN {catalog}sys.indexes i ON i.object_id = ix.object_id AND i.index_id = ix.index_id \
           WHERE i.is_primary_key = 1 AND ix.key_ordinal > 0 \
         ) pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id \
         WHERE s.name = @P1 AND o.name = @P2 AND o.type IN ('U', 'V') \
         ORDER BY c.column_id"
    )
}

pub(crate) fn indexes_sql(database: &str) -> String {
    let catalog = catalog_prefix(database);
    format!(
        "SELECT i.index_id, i.name, i.is_unique, i.is_primary_key, i.type_desc, \
         i.is_unique_constraint, i.has_filter, i.is_disabled, i.is_hypothetical, \
         ic.key_ordinal, c.name AS column_name, ic.is_included_column, ic.is_descending_key, \
         ds.type AS data_space_type \
         FROM {catalog}sys.indexes i \
         JOIN {catalog}sys.objects o ON o.object_id = i.object_id \
         JOIN {catalog}sys.tables t ON t.object_id = o.object_id \
         JOIN {catalog}sys.schemas s ON s.schema_id = t.schema_id \
         LEFT JOIN {catalog}sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
         LEFT JOIN {catalog}sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
         LEFT JOIN {catalog}sys.data_spaces ds ON ds.data_space_id = i.data_space_id \
         WHERE s.name = @P1 AND t.name = @P2 AND i.index_id > 0 \
         ORDER BY i.index_id, ic.key_ordinal, ic.index_column_id"
    )
}

pub(crate) fn foreign_keys_sql(database: &str) -> String {
    let catalog = catalog_prefix(database);
    format!(
        "SELECT fk.name, fkc.constraint_column_id, pc.name AS parent_column, \
         rs.name AS referenced_schema, rt.name AS referenced_table, rc.name AS referenced_column, \
         fk.update_referential_action_desc, fk.delete_referential_action_desc, \
         fk.is_disabled, fk.is_not_trusted, fk.is_not_for_replication \
         FROM {catalog}sys.foreign_keys fk \
         JOIN {catalog}sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id \
         JOIN {catalog}sys.tables t ON t.object_id = fk.parent_object_id \
         JOIN {catalog}sys.schemas s ON s.schema_id = t.schema_id \
         JOIN {catalog}sys.columns pc ON pc.object_id = fkc.parent_object_id AND pc.column_id = fkc.parent_column_id \
         JOIN {catalog}sys.tables rt ON rt.object_id = fk.referenced_object_id \
         JOIN {catalog}sys.schemas rs ON rs.schema_id = rt.schema_id \
         JOIN {catalog}sys.columns rc ON rc.object_id = fkc.referenced_object_id AND rc.column_id = fkc.referenced_column_id \
         WHERE s.name = @P1 AND t.name = @P2 \
         ORDER BY fk.object_id, fkc.constraint_column_id"
    )
}

pub(crate) fn checks_sql(database: &str) -> String {
    let catalog = catalog_prefix(database);
    let selected = selected_table(&catalog);
    format!(
        "SELECT cc.name, cc.definition, cc.is_disabled, cc.is_not_trusted, cc.is_not_for_replication \
         FROM {catalog}sys.check_constraints cc \
         JOIN {catalog}sys.objects o ON o.object_id = cc.parent_object_id \
         {selected} \
         ORDER BY cc.name"
    )
}

pub(crate) fn parse_columns(
    result: QueryResult,
) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
    let mut columns = Vec::with_capacity(result.rows.len());
    let mut key_columns = Vec::new();
    for row in result.rows {
        if row.len() < 18 {
            return Err(incomplete("catalog row omitted column collation metadata"));
        }
        let name = required_text(&row, 0, "column name")?;
        let data_type = required_text(&row, 1, "declared column type")?;
        let is_primary_key = required_bool(&row, 6, "primary key flag")?;
        let key_ordinal = required_integer(&row, 7, "primary key ordinal")?;
        let database_collation = required_text(&row, 17, "database collation")?;
        if let Some(column_collation) = optional_text(&row, 16).filter(|value| !value.is_empty()) {
            if !column_collation.eq_ignore_ascii_case(&database_collation) {
                return Err(unsupported(format!(
                    "SQL Server column '{name}' uses non-default collation '{column_collation}', which ColumnSchema cannot represent safely"
                )));
            }
        }
        for (index, reason) in [
            (8, "computed columns"),
            (9, "generated-always columns"),
            (10, "FILESTREAM columns"),
            (11, "sparse columns"),
            (12, "column-set columns"),
            (14, "rowversion columns"),
            (15, "non-default IDENTITY seed/increment"),
        ] {
            if optional_bool(&row, index) == Some(true) {
                return Err(unsupported(format!(
                    "SQL Server table contains {reason}; this metadata contract cannot represent it safely"
                )));
            }
        }
        if optional_bool(&row, 13) == Some(true) {
            return Err(unsupported(
                "SQL Server table uses a legacy bound DEFAULT; its expression is not represented by sys.default_constraints",
            ));
        }
        if is_primary_key {
            if key_ordinal <= 0 {
                return Err(incomplete("primary key column has no key ordinal"));
            }
            key_columns.push((key_ordinal, name.clone()));
        }
        columns.push(ColumnSchema {
            name,
            data_type,
            nullable: required_bool(&row, 2, "nullability")?,
            default_value: optional_text(&row, 4),
            comment: optional_text(&row, 5),
            is_primary_key,
            is_auto_increment: required_bool(&row, 3, "identity flag")?,
        });
    }
    ensure_unique_ordinals(
        key_columns.iter().map(|(ordinal, _)| *ordinal),
        "primary-key",
    )?;
    key_columns.sort_by_key(|(ordinal, _)| *ordinal);
    Ok((
        columns,
        key_columns.into_iter().map(|(_, column)| column).collect(),
    ))
}

pub(crate) fn parse_indexes(result: QueryResult) -> Result<Vec<IndexInfo>, DriverError> {
    #[derive(Default)]
    struct PendingIndex {
        name: String,
        is_unique: bool,
        is_primary: bool,
        index_type: String,
        columns: Vec<(i64, String)>,
    }

    let mut ordered = Vec::new();
    let mut indexes = HashMap::<i64, PendingIndex>::new();
    for row in result.rows {
        let id = required_integer(&row, 0, "index id")?;
        let name = required_text(&row, 1, "index name")?;
        let unique = required_bool(&row, 2, "index uniqueness")?;
        let primary = required_bool(&row, 3, "primary index flag")?;
        let type_desc = required_text(&row, 4, "index type")?;
        let unique_constraint = required_bool(&row, 5, "unique constraint flag")?;
        if required_bool(&row, 6, "filtered index flag")?
            || required_bool(&row, 7, "disabled index flag")?
            || required_bool(&row, 8, "hypothetical index flag")?
        {
            return Err(unsupported(format!(
                "SQL Server index '{name}' is filtered, disabled, or hypothetical and cannot be represented safely"
            )));
        }
        let data_space = optional_text(&row, 13).unwrap_or_default();
        if data_space.eq_ignore_ascii_case("PS") {
            return Err(unsupported(format!(
                "SQL Server index '{name}' uses a partition scheme that is not represented by the migration model"
            )));
        }
        let supported_type = matches!(type_desc.as_str(), "CLUSTERED" | "NONCLUSTERED");
        if !supported_type {
            return Err(unsupported(format!(
                "SQL Server index '{name}' has unsupported type '{type_desc}'"
            )));
        }
        let key_ordinal = optional_integer(&row, 9).unwrap_or(0);
        let column = optional_text(&row, 10);
        let included = optional_bool(&row, 11).unwrap_or(false);
        let descending = optional_bool(&row, 12).unwrap_or(false);
        if included || descending {
            return Err(unsupported(format!(
                "SQL Server index '{name}' has INCLUDE or descending key columns that are not represented by IndexInfo"
            )));
        }
        let index = indexes.entry(id).or_insert_with(|| {
            ordered.push(id);
            PendingIndex {
                name: name.clone(),
                is_unique: unique,
                is_primary: primary,
                index_type: if unique_constraint {
                    format!("UNIQUE_CONSTRAINT:{type_desc}")
                } else {
                    type_desc.clone()
                },
                columns: Vec::new(),
            }
        });
        if index.name != name || index.is_unique != unique || index.is_primary != primary {
            return Err(incomplete(format!(
                "SQL Server index id {id} returned conflicting catalog rows"
            )));
        }
        if key_ordinal > 0 {
            let column = column.ok_or_else(|| incomplete("index key column is missing"))?;
            index.columns.push((key_ordinal, column));
        }
    }

    let mut parsed = Vec::with_capacity(ordered.len());
    for id in ordered {
        let mut index = indexes
            .remove(&id)
            .ok_or_else(|| incomplete(format!("index id {id} disappeared while parsing")))?;
        index.columns.sort_by_key(|(ordinal, _)| *ordinal);
        if index.columns.is_empty() {
            return Err(incomplete(format!(
                "SQL Server index '{}' has no representable key columns",
                index.name
            )));
        }
        ensure_unique_ordinals(
            index.columns.iter().map(|(ordinal, _)| *ordinal),
            &format!("index '{}' key", index.name),
        )?;
        parsed.push(IndexInfo {
            name: index.name,
            columns: index.columns.into_iter().map(|(_, name)| name).collect(),
            is_unique: index.is_unique,
            is_primary: index.is_primary,
            index_type: index.index_type,
        });
    }
    Ok(parsed)
}

pub(crate) fn parse_foreign_keys(result: QueryResult) -> Result<Vec<ForeignKeyInfo>, DriverError> {
    #[derive(Default)]
    struct PendingForeignKey {
        columns: Vec<(i64, String, String)>,
        referenced_table: Option<String>,
        on_update: Option<String>,
        on_delete: Option<String>,
    }

    let mut ordered = Vec::new();
    let mut keys = HashMap::<String, PendingForeignKey>::new();
    for row in result.rows {
        let name = required_text(&row, 0, "foreign key name")?;
        if required_bool(&row, 8, "foreign key disabled state")?
            || required_bool(&row, 9, "foreign key trust state")?
            || required_bool(&row, 10, "foreign key replication state")?
        {
            return Err(unsupported(format!(
                "SQL Server foreign key '{name}' is disabled, untrusted, or NOT FOR REPLICATION"
            )));
        }
        let ordinal = required_integer(&row, 1, "foreign key column ordinal")?;
        let local_column = required_text(&row, 2, "foreign key local column")?;
        let ref_schema = required_text(&row, 3, "foreign key referenced schema")?;
        let ref_table = required_text(&row, 4, "foreign key referenced table")?;
        let ref_column = required_text(&row, 5, "foreign key referenced column")?;
        let referenced_table = format!(
            "[{}].[{}]",
            ref_schema.replace(']', "]]"),
            ref_table.replace(']', "]]"),
        );
        let on_update = normalize_action(&required_text(&row, 6, "foreign key update action")?);
        let on_delete = normalize_action(&required_text(&row, 7, "foreign key delete action")?);
        let key = keys.entry(name.clone()).or_insert_with(|| {
            ordered.push(name.clone());
            PendingForeignKey {
                referenced_table: Some(referenced_table.clone()),
                on_update: Some(on_update.clone()),
                on_delete: Some(on_delete.clone()),
                ..PendingForeignKey::default()
            }
        });
        if key.referenced_table.as_deref() != Some(referenced_table.as_str())
            || key.on_update.as_deref() != Some(on_update.as_str())
            || key.on_delete.as_deref() != Some(on_delete.as_str())
        {
            return Err(incomplete(format!(
                "SQL Server foreign key '{name}' returned conflicting catalog rows"
            )));
        }
        key.columns.push((ordinal, local_column, ref_column));
    }

    let mut parsed = Vec::with_capacity(ordered.len());
    for name in ordered {
        let mut key = keys
            .remove(&name)
            .ok_or_else(|| incomplete(format!("foreign key '{name}' disappeared while parsing")))?;
        key.columns.sort_by_key(|(ordinal, _, _)| *ordinal);
        if key.columns.is_empty() {
            return Err(incomplete(format!(
                "SQL Server foreign key '{name}' has no columns"
            )));
        }
        ensure_unique_ordinals(
            key.columns.iter().map(|(ordinal, _, _)| *ordinal),
            &format!("foreign key '{name}' column"),
        )?;
        parsed.push(ForeignKeyInfo {
            name,
            columns: key
                .columns
                .iter()
                .map(|(_, local, _)| local.clone())
                .collect(),
            referenced_table: key
                .referenced_table
                .ok_or_else(|| incomplete("foreign key referenced table is missing"))?,
            referenced_columns: key
                .columns
                .into_iter()
                .map(|(_, _, remote)| remote)
                .collect(),
            on_update: key
                .on_update
                .ok_or_else(|| incomplete("foreign key update action is missing"))?,
            on_delete: key
                .on_delete
                .ok_or_else(|| incomplete("foreign key delete action is missing"))?,
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        });
    }
    Ok(parsed)
}

fn ensure_unique_ordinals(
    ordinals: impl IntoIterator<Item = i64>,
    label: &str,
) -> Result<(), DriverError> {
    let mut seen = HashSet::new();
    for ordinal in ordinals {
        if !seen.insert(ordinal) {
            return Err(incomplete(format!("duplicate {label} ordinal {ordinal}")));
        }
    }
    Ok(())
}

pub(crate) fn parse_checks(result: QueryResult) -> Result<Vec<CheckConstraint>, DriverError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            let name = required_text(&row, 0, "CHECK constraint name")?;
            if required_bool(&row, 2, "CHECK disabled state")?
                || required_bool(&row, 3, "CHECK trust state")?
                || required_bool(&row, 4, "CHECK replication state")?
            {
                return Err(unsupported(format!(
                    "SQL Server CHECK constraint '{name}' is disabled, untrusted, or NOT FOR REPLICATION"
                )));
            }
            Ok(CheckConstraint {
                name,
                expression: required_text(&row, 1, "CHECK expression")?,
            })
        })
        .collect()
}

fn normalize_action(value: &str) -> String {
    value.replace('_', " ").to_ascii_uppercase()
}

fn value_at<'a>(row: &'a [Option<Value>], index: usize) -> Option<&'a Value> {
    row.get(index).and_then(Option::as_ref)
}

fn display(value: &Value) -> String {
    match value {
        Value::String(value) | Value::Timestamp(value) => value.clone(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Bytes(value) => String::from_utf8_lossy(value).into_owned(),
        Value::Json(value) => value.to_string(),
        Value::Null => String::new(),
    }
}

fn required_text(row: &[Option<Value>], index: usize, label: &str) -> Result<String, DriverError> {
    value_at(row, index)
        .map(display)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| incomplete(format!("catalog row omitted {label}")))
}

fn optional_text(row: &[Option<Value>], index: usize) -> Option<String> {
    value_at(row, index).map(display)
}

fn optional_bool(row: &[Option<Value>], index: usize) -> Option<bool> {
    match value_at(row, index) {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::Integer(value)) => Some(*value != 0),
        Some(Value::String(value)) => match value.as_str() {
            "1" | "true" | "TRUE" => Some(true),
            "0" | "false" | "FALSE" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn required_bool(row: &[Option<Value>], index: usize, label: &str) -> Result<bool, DriverError> {
    optional_bool(row, index).ok_or_else(|| incomplete(format!("catalog row omitted {label}")))
}

fn optional_integer(row: &[Option<Value>], index: usize) -> Option<i64> {
    match value_at(row, index) {
        Some(Value::Integer(value)) => Some(*value),
        Some(Value::Float(value)) => Some(*value as i64),
        Some(Value::String(value)) => value.parse().ok(),
        _ => None,
    }
}

fn required_integer(row: &[Option<Value>], index: usize, label: &str) -> Result<i64, DriverError> {
    optional_integer(row, index).ok_or_else(|| incomplete(format!("catalog row omitted {label}")))
}

fn unsupported(message: impl Into<String>) -> DriverError {
    DriverError::Unsupported(message.into())
}

fn incomplete(message: impl Into<String>) -> DriverError {
    DriverError::QueryFailed(format!(
        "SQL Server returned incomplete schema metadata: {}",
        message.into()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(rows: Vec<Vec<Option<Value>>>) -> QueryResult {
        QueryResult {
            columns: Vec::new(),
            rows,
            rows_affected: None,
            execution_time_ms: 0,
        }
    }

    fn text(value: &str) -> Option<Value> {
        Some(Value::String(value.into()))
    }

    fn bit(value: bool) -> Option<Value> {
        Some(Value::Bool(value))
    }

    #[test]
    fn metadata_queries_quote_catalog_and_bind_object_names() {
        let columns = columns_sql("db]");
        assert!(columns.contains("FROM [db]]].sys.columns"));
        assert!(columns.contains("QUOTENAME(tp_schema.name)"));
        assert!(columns.contains("[db]]].sys.schemas tp_schema"));
        assert!(columns.contains("s.name = @P1 AND o.name = @P2"));
        assert!(
            columns.contains("DATABASEPROPERTYEX(CASE WHEN @P3 = N'' THEN DB_NAME() ELSE @P3 END")
        );
        assert!(!columns.contains("@P1'"));
        assert!(indexes_sql("db").contains("ic.key_ordinal"));
        assert!(foreign_keys_sql("db").contains("fkc.constraint_column_id"));
        assert!(checks_sql("db").contains("cc.is_not_trusted"));
    }

    #[test]
    fn composite_primary_key_keeps_catalog_key_order() {
        let mut a = vec![
            text("first"),
            text("int"),
            bit(false),
            bit(false),
            None,
            None,
            bit(true),
            Some(Value::Integer(2)),
            bit(false),
            Some(Value::Integer(0)),
            bit(false),
            bit(false),
            bit(false),
            bit(false),
            bit(false),
            bit(false),
            None,
            text("Latin1_General_100_CI_AS_SC_UTF8"),
        ];
        let mut b = a.clone();
        a[0] = text("a");
        b[0] = text("z");
        a[7] = Some(Value::Integer(2));
        b[7] = Some(Value::Integer(1));
        let (columns, keys) = parse_columns(result(vec![a, b])).expect("parse columns");
        assert_eq!(columns.len(), 2);
        assert_eq!(keys, vec!["z", "a"]);
    }

    #[test]
    fn duplicate_primary_key_ordinals_are_rejected() {
        let mut first = vec![
            text("first"),
            text("int"),
            bit(false),
            bit(false),
            None,
            None,
            bit(true),
            Some(Value::Integer(1)),
            bit(false),
            Some(Value::Integer(0)),
            bit(false),
            bit(false),
            bit(false),
            bit(false),
            bit(false),
            bit(false),
            None,
            text("Latin1_General_100_CI_AS_SC_UTF8"),
        ];
        let mut second = first.clone();
        first[0] = text("first");
        second[0] = text("second");

        let parsed = parse_columns(result(vec![first, second]));
        assert!(
            matches!(parsed, Err(DriverError::QueryFailed(ref message)) if message.contains("duplicate") || message.contains("ordinal")),
            "duplicate primary-key ordinals should fail as incomplete metadata, got {parsed:?}"
        );
    }

    #[test]
    fn duplicate_secondary_index_ordinals_are_rejected() {
        let index_row = |column: &str| {
            vec![
                Some(Value::Integer(2)),
                text("ix_pair"),
                bit(false),
                bit(false),
                text("NONCLUSTERED"),
                bit(false),
                bit(false),
                bit(false),
                bit(false),
                Some(Value::Integer(1)),
                text(column),
                bit(false),
                bit(false),
                text("FG"),
            ]
        };

        let parsed = parse_indexes(result(vec![index_row("first"), index_row("second")]));
        assert!(
            matches!(parsed, Err(DriverError::QueryFailed(ref message)) if message.contains("duplicate") || message.contains("ordinal")),
            "duplicate secondary-index ordinals should fail as incomplete metadata, got {parsed:?}"
        );
    }

    #[test]
    fn duplicate_foreign_key_ordinals_are_rejected() {
        let foreign_key_row = |local: &str, remote: &str| {
            vec![
                text("fk_pair"),
                Some(Value::Integer(1)),
                text(local),
                text("dbo"),
                text("parent"),
                text(remote),
                text("NO_ACTION"),
                text("NO_ACTION"),
                bit(false),
                bit(false),
                bit(false),
            ]
        };

        let parsed = parse_foreign_keys(result(vec![
            foreign_key_row("local_a", "remote_a"),
            foreign_key_row("local_b", "remote_b"),
        ]));
        assert!(
            matches!(parsed, Err(DriverError::QueryFailed(ref message)) if message.contains("duplicate") || message.contains("ordinal")),
            "duplicate foreign-key ordinals should fail as incomplete metadata, got {parsed:?}"
        );
    }

    #[test]
    fn nondefault_column_collation_is_rejected() {
        let row = vec![
            text("label"),
            text("nvarchar(40)"),
            bit(true),
            bit(false),
            None,
            None,
            bit(false),
            Some(Value::Integer(0)),
            bit(false),
            Some(Value::Integer(0)),
            bit(false),
            bit(false),
            bit(false),
            bit(false),
            bit(false),
            bit(false),
            text("Latin1_General_100_CI_AS_SC_UTF8"),
            text("SQL_Latin1_General_CP1_CI_AS"),
        ];

        assert!(matches!(
            parse_columns(result(vec![row])),
            Err(DriverError::Unsupported(message)) if message.contains("non-default collation")
        ));
    }

    #[test]
    fn filtered_and_untrusted_objects_fail_closed() {
        let index_row = vec![
            Some(Value::Integer(2)),
            text("idx_filtered"),
            bit(false),
            bit(false),
            text("NONCLUSTERED"),
            bit(false),
            bit(true),
            bit(false),
            bit(false),
            Some(Value::Integer(1)),
            text("value"),
            bit(false),
            bit(false),
            text("FG"),
        ];
        assert!(matches!(
            parse_indexes(result(vec![index_row])),
            Err(DriverError::Unsupported(message)) if message.contains("filtered")
        ));

        let check = vec![
            text("ck_positive"),
            text("([value] > 0)"),
            bit(false),
            bit(true),
            bit(false),
        ];
        assert!(matches!(
            parse_checks(result(vec![check])),
            Err(DriverError::Unsupported(message)) if message.contains("untrusted")
        ));
    }

    #[test]
    fn composite_index_order_and_fk_reference_schema_are_preserved() {
        let index_rows = vec![
            vec![
                Some(Value::Integer(2)),
                text("ix_pair"),
                bit(false),
                bit(false),
                text("NONCLUSTERED"),
                bit(false),
                bit(false),
                bit(false),
                bit(false),
                Some(Value::Integer(2)),
                text("second"),
                bit(false),
                bit(false),
                text("FG"),
            ],
            vec![
                Some(Value::Integer(2)),
                text("ix_pair"),
                bit(false),
                bit(false),
                text("NONCLUSTERED"),
                bit(false),
                bit(false),
                bit(false),
                bit(false),
                Some(Value::Integer(1)),
                text("first"),
                bit(false),
                bit(false),
                text("FG"),
            ],
        ];
        let indexes = parse_indexes(result(index_rows)).expect("parse indexes");
        assert_eq!(indexes[0].columns, vec!["first", "second"]);

        let fk_rows = vec![vec![
            text("fk_pair"),
            Some(Value::Integer(1)),
            text("left_id"),
            text("other.schema"),
            text("parent]table"),
            text("key_id"),
            text("NO_ACTION"),
            text("CASCADE"),
            bit(false),
            bit(false),
            bit(false),
        ]];
        let keys = parse_foreign_keys(result(fk_rows)).expect("parse foreign key");
        assert_eq!(keys[0].columns, vec!["left_id"]);
        assert_eq!(keys[0].referenced_table, "[other.schema].[parent]]table]");
        assert_eq!(keys[0].on_delete, "CASCADE");
        assert_eq!(
            keys[0].deferrability,
            ForeignKeyDeferrability::NotDeferrable
        );
    }
}
