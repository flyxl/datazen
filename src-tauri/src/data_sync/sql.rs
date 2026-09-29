//! ChangeSet → parameterized DML + read-only preview SQL.

use datazen_driver_api::Value;
use serde::{Deserialize, Serialize};

use super::changeset::TableChangeSet;
use super::error::DataSyncError;
use super::model::{ChangeOperation, ConflictPolicy, RowChange};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlStatement {
    pub table: String,
    pub operation: ChangeOperation,
    pub sql: String,
    pub preview_sql: String,
    pub parameters: Vec<Value>,
    pub row_key: Vec<Value>,
}

pub fn quote_ident_sql(name: &str, quote: char) -> String {
    if quote == '[' {
        return format!("[{}]", name.replace(']', "]]"));
    }
    let doubled = name.replace(quote, &format!("{quote}{quote}"));
    format!("{quote}{doubled}{quote}")
}

/// Qualify `table` as `schema.table` when `schema` is non-empty (PostgreSQL etc.).
pub fn qualify_table_sql(schema: Option<&str>, table: &str, quote: char) -> String {
    match schema.map(str::trim).filter(|s| !s.is_empty()) {
        Some(schema) => format!(
            "{}.{}",
            quote_ident_sql(schema, quote),
            quote_ident_sql(table, quote)
        ),
        None => quote_ident_sql(table, quote),
    }
}

/// Qualify a table reference for DML/SELECT without switching the session catalog.
///
/// - MySQL/MariaDB/ClickHouse: `` `database`.`table` `` when `database` is set.
/// - SQL Server: `[database].[schema].[table]` with either qualifier set.
/// - PostgreSQL and similar: `"schema"."table"` when `schema` is set.
/// - Otherwise: bare `table`.
pub fn qualify_relation_sql(
    family: &str,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    quote: char,
) -> String {
    let family = family.to_ascii_lowercase();
    if matches!(family.as_str(), "mysql" | "mariadb" | "clickhouse") {
        return match database.map(str::trim).filter(|s| !s.is_empty()) {
            Some(db) => format!(
                "{}.{}",
                quote_ident_sql(db, quote),
                quote_ident_sql(table, quote)
            ),
            None => quote_ident_sql(table, quote),
        };
    }
    if family == "sqlserver" {
        return [
            database.map(str::trim).filter(|s| !s.is_empty()),
            schema.map(str::trim).filter(|s| !s.is_empty()),
            Some(table),
        ]
        .into_iter()
        .flatten()
        .map(|part| quote_ident_sql(part, quote))
        .collect::<Vec<_>>()
        .join(".");
    }
    qualify_table_sql(schema, table, quote)
}

pub fn qualify_table_ident<Q>(schema: Option<&str>, table: &str, quote_ident: Q) -> String
where
    Q: Fn(&str) -> String,
{
    match schema.map(str::trim).filter(|s| !s.is_empty()) {
        Some(schema) => format!("{}.{}", quote_ident(schema), quote_ident(table)),
        None => quote_ident(table),
    }
}

pub fn mysql_placeholder(_index: usize) -> String {
    "?".into()
}

pub fn postgres_placeholder(index: usize) -> String {
    format!("${index}")
}

/// Optional PostgreSQL cast suffix for parameterized placeholders and preview literals.
pub fn postgres_type_cast(data_type: &str) -> Option<&'static str> {
    let t = data_type.trim().to_ascii_lowercase();
    if t == "uuid" {
        return Some("uuid");
    }
    if t.contains("timestamp with time zone") || t == "timestamptz" {
        return Some("timestamptz");
    }
    if t.contains("timestamp without time zone") || t == "timestamp" {
        return Some("timestamp");
    }
    if t == "date" {
        return Some("date");
    }
    if t == "numeric" || t.starts_with("numeric(") || t == "decimal" || t.starts_with("decimal(") {
        return Some("numeric");
    }
    if t == "time without time zone" || t == "time" {
        return Some("time");
    }
    if t == "jsonb" {
        return Some("jsonb");
    }
    if t == "json" {
        return Some("json");
    }
    None
}

pub fn postgres_typed_placeholder(index: usize, data_type: Option<&str>) -> String {
    match data_type.and_then(postgres_type_cast) {
        Some(cast) => format!("${index}::{cast}"),
        None => postgres_placeholder(index),
    }
}

fn column_type<'a>(
    column_names: &[String],
    column_types: &'a [String],
    col: &str,
) -> Option<&'a str> {
    column_names
        .iter()
        .position(|c| c == col)
        .and_then(|i| column_types.get(i).map(|s| s.as_str()))
}

fn binary_literal_placeholder(bytes: &[u8]) -> String {
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "/* binary 0x{hex}; target driver literal required */ __DATAZEN_BINARY_LITERAL_REQUIRED__"
    )
}

pub fn format_literal(value: &Option<Value>) -> String {
    match value {
        None | Some(Value::Null) => "NULL".into(),
        Some(Value::Bool(true)) => "TRUE".into(),
        Some(Value::Bool(false)) => "FALSE".into(),
        Some(Value::Integer(n)) => n.to_string(),
        Some(Value::Float(n)) => n.to_string(),
        Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
        Some(Value::Bytes(bytes)) => binary_literal_placeholder(bytes),
        Some(Value::Timestamp(s)) => format!("'{}'", s.replace('\'', "''")),
        Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
    }
}

pub fn format_typed_literal(value: &Option<Value>, data_type: Option<&str>) -> String {
    let lit = format_literal(value);
    if matches!(lit.as_str(), "NULL" | "TRUE" | "FALSE") {
        return lit;
    }
    match data_type.and_then(postgres_type_cast) {
        Some(cast) if lit.starts_with('\'') => format!("{lit}::{cast}"),
        _ => lit,
    }
}

pub fn generate_table_sql<Q, P>(
    table: &TableChangeSet,
    target_schema: Option<&str>,
    pk_columns: &[String],
    column_names: &[String],
    column_types: &[String],
    quote_ident: Q,
    placeholder: P,
) -> Result<Vec<SqlStatement>, DataSyncError>
where
    Q: Fn(&str) -> String + Copy,
    P: Fn(usize, Option<&str>) -> String,
{
    generate_table_sql_with_preview_formatter_and_policy(
        table,
        target_schema,
        pk_columns,
        column_names,
        column_types,
        quote_ident,
        placeholder,
        ConflictPolicy::Abort,
        |_, value, data_type| Ok(format_typed_literal(value, data_type)),
    )
}

pub fn generate_table_sql_with_preview_formatter<Q, P, L>(
    table: &TableChangeSet,
    target_schema: Option<&str>,
    pk_columns: &[String],
    column_names: &[String],
    column_types: &[String],
    quote_ident: Q,
    placeholder: P,
    preview_literal: L,
) -> Result<Vec<SqlStatement>, DataSyncError>
where
    Q: Fn(&str) -> String + Copy,
    P: Fn(usize, Option<&str>) -> String,
    L: Fn(&str, &Option<Value>, Option<&str>) -> Result<String, DataSyncError>,
{
    generate_table_sql_with_preview_formatter_and_policy(
        table,
        target_schema,
        pk_columns,
        column_names,
        column_types,
        quote_ident,
        placeholder,
        ConflictPolicy::Abort,
        preview_literal,
    )
}

/// Generate DML with an explicit optimistic-concurrency policy.
pub fn generate_table_sql_with_preview_formatter_and_policy<Q, P, L>(
    table: &TableChangeSet,
    target_schema: Option<&str>,
    pk_columns: &[String],
    column_names: &[String],
    column_types: &[String],
    quote_ident: Q,
    placeholder: P,
    conflict_policy: ConflictPolicy,
    preview_literal: L,
) -> Result<Vec<SqlStatement>, DataSyncError>
where
    Q: Fn(&str) -> String + Copy,
    P: Fn(usize, Option<&str>) -> String,
    L: Fn(&str, &Option<Value>, Option<&str>) -> Result<String, DataSyncError>,
{
    let qualified_table = qualify_table_ident(target_schema, &table.target_table, quote_ident);
    generate_table_sql_with_qualified_table_and_policy(
        table,
        &qualified_table,
        pk_columns,
        column_names,
        column_types,
        quote_ident,
        |index, data_type| Ok(placeholder(index, data_type)),
        conflict_policy,
        preview_literal,
    )
}

/// Generate a Data Sync ChangeSet using a relation string rendered by the
/// target driver adapter and fallible driver-owned parameter placeholders.
/// This is needed for dialects such as SQL Server, whose target reference may
/// include database, schema, and table parts and whose placeholder indexes
/// have a finite driver-defined range.
pub fn generate_table_sql_with_qualified_table_and_policy<Q, P, L>(
    table: &TableChangeSet,
    qualified_table: &str,
    pk_columns: &[String],
    column_names: &[String],
    column_types: &[String],
    quote_ident: Q,
    placeholder: P,
    conflict_policy: ConflictPolicy,
    preview_literal: L,
) -> Result<Vec<SqlStatement>, DataSyncError>
where
    Q: Fn(&str) -> String + Copy,
    P: Fn(usize, Option<&str>) -> Result<String, DataSyncError>,
    L: Fn(&str, &Option<Value>, Option<&str>) -> Result<String, DataSyncError>,
{
    if pk_columns.is_empty() {
        return Err(DataSyncError::validation(
            "cannot generate SQL without primary key columns",
        ));
    }
    let mut out = Vec::new();
    for change in &table.changes {
        out.push(statement_for_change(
            &table.target_table,
            qualified_table,
            change,
            pk_columns,
            column_names,
            column_types,
            &quote_ident,
            &placeholder,
            conflict_policy,
            &preview_literal,
        )?);
    }
    Ok(out)
}

fn statement_for_change<Q, P, L>(
    table: &str,
    qualified_table: &str,
    change: &RowChange,
    pk_columns: &[String],
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
    conflict_policy: ConflictPolicy,
    preview_literal: &L,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> Result<String, DataSyncError>,
    L: Fn(&str, &Option<Value>, Option<&str>) -> Result<String, DataSyncError>,
{
    match change.operation {
        ChangeOperation::Insert => insert_sql(
            table,
            qualified_table,
            change,
            column_names,
            column_types,
            quote_ident,
            placeholder,
            preview_literal,
        ),
        ChangeOperation::Update => update_sql(
            table,
            qualified_table,
            change,
            pk_columns,
            column_names,
            column_types,
            quote_ident,
            placeholder,
            conflict_policy,
            preview_literal,
        ),
        ChangeOperation::Delete => delete_sql(
            table,
            qualified_table,
            change,
            pk_columns,
            column_names,
            column_types,
            quote_ident,
            placeholder,
            conflict_policy,
            preview_literal,
        ),
        ChangeOperation::Unchanged => Err(DataSyncError::validation(
            "unchanged rows must not generate SQL",
        )),
    }
}

fn insert_sql<Q, P, L>(
    table: &str,
    qualified_table: &str,
    change: &RowChange,
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
    preview_literal: &L,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> Result<String, DataSyncError>,
    L: Fn(&str, &Option<Value>, Option<&str>) -> Result<String, DataSyncError>,
{
    let row = change
        .source_row
        .as_ref()
        .ok_or_else(|| DataSyncError::validation("INSERT requires a source row"))?;
    if row.len() != column_names.len() {
        return Err(DataSyncError::validation(
            "INSERT row width does not match column list",
        ));
    }
    let cols = column_names
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let mut params = Vec::new();
    let mut preview_vals = Vec::new();
    let mut placeholders = Vec::new();
    for (i, cell) in row.iter().enumerate() {
        let col_type = column_types.get(i).map(|s| s.as_str());
        placeholders.push(placeholder(i + 1, col_type)?);
        params.push(cell.clone().unwrap_or(Value::Null));
        preview_vals.push(preview_literal(&column_names[i], cell, col_type)?);
    }
    Ok(SqlStatement {
        table: table.into(),
        operation: ChangeOperation::Insert,
        sql: format!(
            "INSERT INTO {qualified_table} ({cols}) VALUES ({})",
            placeholders.join(", ")
        ),
        preview_sql: format!(
            "INSERT INTO {qualified_table} ({cols}) VALUES ({})",
            preview_vals.join(", ")
        ),
        parameters: params,
        row_key: change.key.clone(),
    })
}

fn update_sql<Q, P, L>(
    table: &str,
    qualified_table: &str,
    change: &RowChange,
    pk_columns: &[String],
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
    conflict_policy: ConflictPolicy,
    preview_literal: &L,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> Result<String, DataSyncError>,
    L: Fn(&str, &Option<Value>, Option<&str>) -> Result<String, DataSyncError>,
{
    let row = change
        .source_row
        .as_ref()
        .ok_or_else(|| DataSyncError::validation("UPDATE requires a source row"))?;
    if change.changed_columns.is_empty() {
        return Err(DataSyncError::validation(
            "UPDATE requires at least one changed column",
        ));
    }
    let mut params = Vec::new();
    let mut set_ph = Vec::new();
    let mut set_lit = Vec::new();
    let mut idx = 1usize;
    for col in &change.changed_columns {
        let pos = column_names.iter().position(|c| c == col).ok_or_else(|| {
            DataSyncError::validation(format!("changed column '{col}' is not in the column list"))
        })?;
        let cell = row.get(pos).cloned().flatten();
        let col_type = column_type(column_names, column_types, col);
        set_ph.push(format!(
            "{} = {}",
            quote_ident(col),
            placeholder(idx, col_type)?
        ));
        set_lit.push(format!(
            "{} = {}",
            quote_ident(col),
            preview_literal(col, &cell, col_type)?
        ));
        params.push(cell.unwrap_or(Value::Null));
        idx += 1;
    }
    let (where_ph, where_lit, where_params) = where_pk(
        pk_columns,
        &change.key,
        idx,
        column_names,
        column_types,
        quote_ident,
        placeholder,
        preview_literal,
    )?;
    let (expected_ph, expected_lit, expected_params) = if conflict_policy != ConflictPolicy::Force {
        let expected_start = idx + where_params.len();
        where_expected_target(
            pk_columns,
            change.target_row.as_ref(),
            expected_start,
            column_names,
            column_types,
            quote_ident,
            placeholder,
            preview_literal,
        )?
    } else {
        (String::new(), String::new(), Vec::new())
    };
    params.extend(where_params);
    params.extend(expected_params);
    let where_ph = join_where_clauses(&where_ph, &expected_ph);
    let where_lit = join_where_clauses(&where_lit, &expected_lit);
    Ok(SqlStatement {
        table: table.into(),
        operation: ChangeOperation::Update,
        sql: format!(
            "UPDATE {qualified_table} SET {} WHERE {}",
            set_ph.join(", "),
            where_ph
        ),
        preview_sql: format!(
            "UPDATE {qualified_table} SET {} WHERE {}",
            set_lit.join(", "),
            where_lit
        ),
        parameters: params,
        row_key: change.key.clone(),
    })
}

fn delete_sql<Q, P, L>(
    table: &str,
    qualified_table: &str,
    change: &RowChange,
    pk_columns: &[String],
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
    conflict_policy: ConflictPolicy,
    preview_literal: &L,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> Result<String, DataSyncError>,
    L: Fn(&str, &Option<Value>, Option<&str>) -> Result<String, DataSyncError>,
{
    let target_row = change.target_row.as_ref();
    if conflict_policy != ConflictPolicy::Force {
        let target_row = target_row
            .ok_or_else(|| DataSyncError::validation("DELETE requires an expected target row"))?;
        if target_row.len() != column_names.len() {
            return Err(DataSyncError::validation(
                "DELETE target row width does not match column list",
            ));
        }
    }
    let (where_ph, where_lit, params) = where_pk(
        pk_columns,
        &change.key,
        1,
        column_names,
        column_types,
        quote_ident,
        placeholder,
        preview_literal,
    )?;
    let (expected_ph, expected_lit, expected_params) = if conflict_policy != ConflictPolicy::Force {
        let expected_start = 1 + params.len();
        where_expected_target(
            pk_columns,
            target_row,
            expected_start,
            column_names,
            column_types,
            quote_ident,
            placeholder,
            preview_literal,
        )?
    } else {
        (String::new(), String::new(), Vec::new())
    };
    let where_ph = join_where_clauses(&where_ph, &expected_ph);
    let where_lit = join_where_clauses(&where_lit, &expected_lit);
    let mut params = params;
    params.extend(expected_params);
    Ok(SqlStatement {
        table: table.into(),
        operation: ChangeOperation::Delete,
        sql: format!("DELETE FROM {qualified_table} WHERE {where_ph}"),
        preview_sql: format!("DELETE FROM {qualified_table} WHERE {where_lit}"),
        parameters: params,
        row_key: change.key.clone(),
    })
}

fn join_where_clauses(primary: &str, expected: &str) -> String {
    if expected.is_empty() {
        primary.to_string()
    } else if primary.is_empty() {
        expected.to_string()
    } else {
        format!("{primary} AND {expected}")
    }
}

/// Add optimistic-concurrency predicates for the target row captured during
/// comparison. Every non-PK column is checked with SQL NULL-safe equality.
/// The duplicated bound value is intentional: it keeps the expression
/// portable across PostgreSQL, MySQL and SQLite while never interpolating
/// target data into executable SQL.
fn where_expected_target<Q, P, L>(
    pk_columns: &[String],
    target_row: Option<&Vec<Option<Value>>>,
    start_index: usize,
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
    preview_literal: &L,
) -> Result<(String, String, Vec<Value>), DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> Result<String, DataSyncError>,
    L: Fn(&str, &Option<Value>, Option<&str>) -> Result<String, DataSyncError>,
{
    let target_row = target_row.ok_or_else(|| {
        DataSyncError::validation("UPDATE/DELETE requires an expected target row")
    })?;
    if target_row.len() != column_names.len() {
        return Err(DataSyncError::validation(
            "expected target row width does not match column list",
        ));
    }
    let mut ph = Vec::new();
    let mut lit = Vec::new();
    let mut params = Vec::new();
    let mut index = start_index;
    for (position, col) in column_names.iter().enumerate() {
        if pk_columns.iter().any(|pk| pk == col) {
            continue;
        }
        let ident = quote_ident(col);
        let expected = target_row[position].clone().unwrap_or(Value::Null);
        let expected_option = Some(expected.clone());
        let col_type = column_types.get(position).map(|s| s.as_str());
        let first = placeholder(index, col_type)?;
        let second = placeholder(index + 1, col_type)?;
        let first_lit = preview_literal(col, &expected_option, col_type)?;
        let second_lit = preview_literal(col, &expected_option, col_type)?;
        ph.push(format!(
            "({ident} = {first} OR ({ident} IS NULL AND {second} IS NULL))"
        ));
        lit.push(format!(
            "({ident} = {first_lit} OR ({ident} IS NULL AND {second_lit} IS NULL))"
        ));
        params.push(expected.clone());
        params.push(expected);
        index += 2;
    }
    Ok((ph.join(" AND "), lit.join(" AND "), params))
}

fn where_pk<Q, P, L>(
    pk_columns: &[String],
    key: &[Value],
    start_index: usize,
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
    preview_literal: &L,
) -> Result<(String, String, Vec<Value>), DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> Result<String, DataSyncError>,
    L: Fn(&str, &Option<Value>, Option<&str>) -> Result<String, DataSyncError>,
{
    if pk_columns.len() != key.len() {
        return Err(DataSyncError::validation(
            "primary key arity does not match row key",
        ));
    }
    let mut ph = Vec::new();
    let mut lit = Vec::new();
    let mut params = Vec::new();
    let mut index = start_index;
    for (col, value) in pk_columns.iter().zip(key.iter()) {
        let ident = quote_ident(col);
        match value {
            Value::Null => {
                ph.push(format!("{ident} IS NULL"));
                lit.push(format!("{ident} IS NULL"));
            }
            v => {
                let col_type = column_type(column_names, column_types, col);
                ph.push(format!("{} = {}", ident, placeholder(index, col_type)?));
                lit.push(format!(
                    "{} = {}",
                    ident,
                    preview_literal(col, &Some(v.clone()), col_type)?
                ));
                params.push(v.clone());
                index += 1;
            }
        }
    }
    Ok((ph.join(" AND "), lit.join(" AND "), params))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_sync::changeset::TableChangeSet;
    use crate::data_sync::model::{ConflictPolicy, RowChange, SyncOptions};

    fn q(name: &str) -> String {
        quote_ident_sql(name, '"')
    }

    fn opts() -> SyncOptions {
        let mut o = SyncOptions::default();
        o.delete = true;
        o
    }

    fn pg_ph(idx: usize, _: Option<&str>) -> String {
        postgres_placeholder(idx)
    }

    fn my_ph(idx: usize, _: Option<&str>) -> String {
        mysql_placeholder(idx)
    }

    #[test]
    fn insert_update_delete_parameterized_and_preview() {
        let options = opts();
        let insert = RowChange::insert(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1)), Some(Value::String("a".into()))],
            &options,
        );
        let update = RowChange::update(
            vec![Value::Integer(2)],
            vec![Some(Value::Integer(2)), Some(Value::String("b".into()))],
            vec![Some(Value::Integer(2)), Some(Value::String("old".into()))],
            vec!["name".into()],
            &options,
        );
        let mut delete = RowChange::delete(
            vec![Value::Integer(3)],
            vec![Some(Value::Integer(3)), Some(Value::String("c".into()))],
            &options,
        );
        delete.selected = true;
        let table = TableChangeSet {
            source_table: "users".into(),
            target_table: "clients".into(),
            changes: vec![insert, update, delete],
        };
        let stmts = generate_table_sql(
            &table,
            None,
            &["id".into()],
            &["id".into(), "name".into()],
            &[],
            q,
            pg_ph,
        )
        .unwrap();
        assert_eq!(stmts.len(), 3);
        assert_eq!(
            stmts[0].sql,
            r#"INSERT INTO "clients" ("id", "name") VALUES ($1, $2)"#
        );
        assert!(stmts[0].preview_sql.contains("'a'"));
        assert_eq!(
            stmts[1].sql,
            r#"UPDATE "clients" SET "name" = $1 WHERE "id" = $2 AND ("name" = $3 OR ("name" IS NULL AND $4 IS NULL))"#
        );
        assert_eq!(stmts[1].parameters.len(), 4);
        assert_eq!(
            stmts[2].sql,
            r#"DELETE FROM "clients" WHERE "id" = $1 AND ("name" = $2 OR ("name" IS NULL AND $3 IS NULL))"#
        );
        assert_eq!(stmts[2].parameters.len(), 3);
        assert_eq!(
            stmts[2].preview_sql,
            r#"DELETE FROM "clients" WHERE "id" = 3 AND ("name" = 'c' OR ("name" IS NULL AND 'c' IS NULL))"#
        );
    }

    #[test]
    fn mysql_placeholders_and_backticks() {
        let options = SyncOptions::default();
        let insert = RowChange::insert(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1))],
            &options,
        );
        let table = TableChangeSet {
            source_table: "t".into(),
            target_table: "t".into(),
            changes: vec![insert],
        };
        let stmts = generate_table_sql(
            &table,
            None,
            &["id".into()],
            &["id".into()],
            &[],
            |n| quote_ident_sql(n, '`'),
            my_ph,
        )
        .unwrap();
        assert_eq!(stmts[0].sql, "INSERT INTO `t` (`id`) VALUES (?)");
    }

    #[test]
    fn rejects_unchanged_and_bad_arity() {
        let same = RowChange::unchanged(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1))],
            vec![Some(Value::Integer(1))],
        );
        let table = TableChangeSet {
            source_table: "t".into(),
            target_table: "t".into(),
            changes: vec![same],
        };
        assert!(
            generate_table_sql(&table, None, &["id".into()], &["id".into()], &[], q, my_ph)
                .is_err()
        );
        assert!(generate_table_sql(&table, None, &[], &["id".into()], &[], q, my_ph).is_err());
    }

    #[test]
    fn mysql_catalog_qualified_target_table() {
        let options = SyncOptions::default();
        let insert = RowChange::insert(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1)), Some(Value::String("a".into()))],
            &options,
        );
        let table = TableChangeSet {
            source_table: "users".into(),
            target_table: "clients".into(),
            changes: vec![insert],
        };
        let stmts = generate_table_sql(
            &table,
            Some("mydb"),
            &["id".into()],
            &["id".into(), "name".into()],
            &[],
            |n| quote_ident_sql(n, '`'),
            my_ph,
        )
        .unwrap();
        assert_eq!(
            stmts[0].sql,
            "INSERT INTO `mydb`.`clients` (`id`, `name`) VALUES (?, ?)"
        );
    }

    #[test]
    fn qualified_target_generator_uses_driver_relation_and_parameter_placeholders() {
        let options = SyncOptions::default();
        let insert = RowChange::insert(
            vec![Value::Integer(7)],
            vec![Some(Value::Integer(7)), Some(Value::String("Ada".into()))],
            &options,
        );
        let table = TableChangeSet {
            source_table: "users".into(),
            target_table: "users".into(),
            changes: vec![insert],
        };
        let statements = generate_table_sql_with_qualified_table_and_policy(
            &table,
            "[archive].[sales].[users]",
            &["id".into()],
            &["id".into(), "name".into()],
            &["int".into(), "nvarchar(32)".into()],
            |name| quote_ident_sql(name, '['),
            |index, _| Ok(format!("@P{index}")),
            ConflictPolicy::Abort,
            |_, value, _| Ok(format_literal(value)),
        )
        .unwrap();
        assert_eq!(
            statements[0].sql,
            "INSERT INTO [archive].[sales].[users] ([id], [name]) VALUES (@P1, @P2)"
        );
        assert_eq!(statements[0].parameters.len(), 2);
    }

    #[test]
    fn qualify_relation_sql_mysql_uses_database() {
        assert_eq!(
            qualify_relation_sql("mysql", Some("mydb"), None, "users", '`'),
            "`mydb`.`users`"
        );
    }

    #[test]
    fn qualify_relation_sql_postgres_uses_schema() {
        assert_eq!(
            qualify_relation_sql("postgresql", Some("ignored"), Some("public"), "users", '"'),
            r#""public"."users""#
        );
    }

    #[test]
    fn schema_qualified_target_table() {
        let options = SyncOptions::default();
        let insert = RowChange::insert(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1)), Some(Value::String("a".into()))],
            &options,
        );
        let table = TableChangeSet {
            source_table: "users".into(),
            target_table: "clients".into(),
            changes: vec![insert],
        };
        let stmts = generate_table_sql(
            &table,
            Some("public"),
            &["id".into()],
            &["id".into(), "name".into()],
            &[],
            q,
            pg_ph,
        )
        .unwrap();
        assert_eq!(
            stmts[0].sql,
            r#"INSERT INTO "public"."clients" ("id", "name") VALUES ($1, $2)"#
        );
    }

    #[test]
    fn null_pk_uses_is_null() {
        let mut options = SyncOptions::default();
        options.delete = true;
        let mut del = RowChange::delete(vec![Value::Null], vec![None], &options);
        del.selected = true;
        let table = TableChangeSet {
            source_table: "t".into(),
            target_table: "t".into(),
            changes: vec![del],
        };
        let stmts = generate_table_sql(&table, None, &["id".into()], &["id".into()], &[], q, pg_ph)
            .unwrap();
        assert_eq!(stmts[0].sql, r#"DELETE FROM "t" WHERE "id" IS NULL"#);
        assert!(stmts[0].parameters.is_empty());
    }

    #[test]
    fn update_expected_target_is_null_safe_and_bound() {
        let options = SyncOptions::default();
        let update = RowChange::update(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1)), Some(Value::String("new".into()))],
            vec![Some(Value::Integer(1)), None],
            vec!["name".into()],
            &options,
        );
        let table = TableChangeSet {
            source_table: "t".into(),
            target_table: "t".into(),
            changes: vec![update],
        };
        let stmts = generate_table_sql(
            &table,
            None,
            &["id".into()],
            &["id".into(), "name".into()],
            &[],
            q,
            pg_ph,
        )
        .unwrap();
        assert_eq!(
            stmts[0].sql,
            r#"UPDATE "t" SET "name" = $1 WHERE "id" = $2 AND ("name" = $3 OR ("name" IS NULL AND $4 IS NULL))"#
        );
        assert!(matches!(stmts[0].parameters[2], Value::Null));
        assert!(matches!(stmts[0].parameters[3], Value::Null));
        assert!(stmts[0].preview_sql.contains("'NULL'") == false);
        assert!(stmts[0].preview_sql.contains("NULL IS NULL"));
    }

    #[test]
    fn skip_keeps_expected_predicates_but_force_removes_them() {
        let options = SyncOptions::default();
        let update = RowChange::update(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1)), Some(Value::String("new".into()))],
            vec![Some(Value::Integer(1)), Some(Value::String("old".into()))],
            vec!["name".into()],
            &options,
        );
        let table = TableChangeSet {
            source_table: "t".into(),
            target_table: "t".into(),
            changes: vec![update],
        };
        let skip = generate_table_sql_with_preview_formatter_and_policy(
            &table,
            None,
            &["id".into()],
            &["id".into(), "name".into()],
            &[],
            q,
            pg_ph,
            ConflictPolicy::Skip,
            |_, value, _| Ok(format_literal(value)),
        )
        .unwrap();
        assert!(skip[0].sql.contains("\"name\" = $3"));
        assert_eq!(skip[0].parameters.len(), 4);

        let force = generate_table_sql_with_preview_formatter_and_policy(
            &table,
            None,
            &["id".into()],
            &["id".into(), "name".into()],
            &[],
            q,
            pg_ph,
            ConflictPolicy::Force,
            |_, value, _| Ok(format_literal(value)),
        )
        .unwrap();
        assert_eq!(
            force[0].sql,
            r#"UPDATE "t" SET "name" = $1 WHERE "id" = $2"#
        );
        assert_eq!(force[0].parameters.len(), 2);
        assert!(!force[0].preview_sql.contains("\"name\" = 'old'"));
    }

    #[test]
    fn update_and_delete_reject_missing_or_mismatched_expected_target() {
        let options = SyncOptions::default();
        let mut update = RowChange::update(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1)), Some(Value::String("new".into()))],
            vec![Some(Value::Integer(1))],
            vec!["name".into()],
            &options,
        );
        update.target_row = None;
        let table = TableChangeSet {
            source_table: "t".into(),
            target_table: "t".into(),
            changes: vec![update],
        };
        assert!(generate_table_sql(
            &table,
            None,
            &["id".into()],
            &["id".into(), "name".into()],
            &[],
            q,
            pg_ph,
        )
        .is_err());
    }

    #[test]
    fn postgres_typed_placeholders_for_uuid_and_timestamptz() {
        let options = SyncOptions::default();
        let insert = RowChange::insert(
            vec![Value::Integer(1)],
            vec![
                Some(Value::Integer(1)),
                Some(Value::String("550e8400-e29b-41d4-a716-446655440000".into())),
                Some(Value::Timestamp("2024-01-15 10:30:00+00".into())),
            ],
            &options,
        );
        let table = TableChangeSet {
            source_table: "events".into(),
            target_table: "events".into(),
            changes: vec![insert],
        };
        let col_types = vec![
            "integer".into(),
            "uuid".into(),
            "timestamp with time zone".into(),
        ];
        let stmts = generate_table_sql(
            &table,
            None,
            &["id".into()],
            &["id".into(), "uid".into(), "created_at".into()],
            &col_types,
            q,
            postgres_typed_placeholder,
        )
        .unwrap();
        assert_eq!(
            stmts[0].sql,
            r#"INSERT INTO "events" ("id", "uid", "created_at") VALUES ($1, $2::uuid, $3::timestamptz)"#
        );
        assert!(stmts[0].preview_sql.contains("::uuid"));
        assert!(stmts[0].preview_sql.contains("::timestamptz"));
    }

    #[test]
    fn postgres_typed_placeholder_casts_numeric_text_values() {
        assert_eq!(
            postgres_typed_placeholder(1, Some("numeric(10,2)")),
            "$1::numeric"
        );
        assert_eq!(
            postgres_typed_placeholder(2, Some("decimal")),
            "$2::numeric"
        );
    }

    #[test]
    fn format_literal_covers_value_kinds() {
        assert_eq!(format_literal(&None), "NULL");
        assert_eq!(format_literal(&Some(Value::Bool(true))), "TRUE");
        assert_eq!(format_literal(&Some(Value::Bool(false))), "FALSE");
        assert_eq!(format_literal(&Some(Value::Float(1.5))), "1.5");
        assert_eq!(
            format_literal(&Some(Value::String("o'reilly".into()))),
            "'o''reilly'"
        );
        assert!(format_literal(&Some(Value::Bytes(vec![0, 255, 254])))
            .contains("__DATAZEN_BINARY_LITERAL_REQUIRED__"));
        assert!(format_literal(&Some(Value::Timestamp("t".into()))).contains("'t'"));
        assert!(format_literal(&Some(Value::Json(serde_json::json!({"a":1})))).contains('{'));
        assert_eq!(quote_ident_sql("na\"me", '"'), r#""na""me""#);
        assert_eq!(quote_ident_sql("na]me", '['), "[na]]me]");
    }
    #[test]
    fn test_tester_binary_preview_never_replaces_bytes_with_unicode() {
        let bytes = vec![0, 255, 254];
        let table = TableChangeSet {
            source_table: "source".into(),
            target_table: "target".into(),
            changes: vec![RowChange::insert(
                vec![Value::Integer(1)],
                vec![Some(Value::Integer(1)), Some(Value::Bytes(bytes.clone()))],
                &opts(),
            )],
        };
        let statements = generate_table_sql(
            &table,
            None,
            &["id".into()],
            &["id".into(), "payload".into()],
            &["INT".into(), "BINARY".into()],
            |name| format!("\"{name}\""),
            |_, _| "?".into(),
        )
        .unwrap();
        assert!(matches!(&statements[0].parameters[1], Value::Bytes(actual) if actual == &bytes));
        assert!(
            !statements[0].preview_sql.contains('�'),
            "binary SQL preview is lossy: {:?}",
            statements[0].preview_sql
        );
    }
}
