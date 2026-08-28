//! ChangeSet → parameterized DML + read-only preview SQL.

use datazen_driver_api::Value;
use serde::{Deserialize, Serialize};

use super::changeset::TableChangeSet;
use super::error::DataSyncError;
use super::model::{ChangeOperation, RowChange};

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
/// - MySQL/MariaDB: `` `database`.`table` `` when `database` is set.
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
    if matches!(family.as_str(), "mysql" | "mariadb") {
        return match database.map(str::trim).filter(|s| !s.is_empty()) {
            Some(db) => format!(
                "{}.{}",
                quote_ident_sql(db, quote),
                quote_ident_sql(table, quote)
            ),
            None => quote_ident_sql(table, quote),
        };
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

pub fn format_literal(value: &Option<Value>) -> String {
    match value {
        None | Some(Value::Null) => "NULL".into(),
        Some(Value::Bool(true)) => "TRUE".into(),
        Some(Value::Bool(false)) => "FALSE".into(),
        Some(Value::Integer(n)) => n.to_string(),
        Some(Value::Float(n)) => n.to_string(),
        Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
        Some(Value::Bytes(b)) => format!("'{}'", String::from_utf8_lossy(b).replace('\'', "''")),
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
    if pk_columns.is_empty() {
        return Err(DataSyncError::validation(
            "cannot generate SQL without primary key columns",
        ));
    }
    let mut out = Vec::new();
    for change in &table.changes {
        out.push(statement_for_change(
            &table.target_table,
            target_schema,
            change,
            pk_columns,
            column_names,
            column_types,
            &quote_ident,
            &placeholder,
        )?);
    }
    Ok(out)
}

fn statement_for_change<Q, P>(
    table: &str,
    schema: Option<&str>,
    change: &RowChange,
    pk_columns: &[String],
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> String,
{
    match change.operation {
        ChangeOperation::Insert => insert_sql(
            table,
            schema,
            change,
            column_names,
            column_types,
            quote_ident,
            placeholder,
        ),
        ChangeOperation::Update => update_sql(
            table,
            schema,
            change,
            pk_columns,
            column_names,
            column_types,
            quote_ident,
            placeholder,
        ),
        ChangeOperation::Delete => delete_sql(
            table,
            schema,
            change,
            pk_columns,
            column_names,
            column_types,
            quote_ident,
            placeholder,
        ),
        ChangeOperation::Unchanged => Err(DataSyncError::validation(
            "unchanged rows must not generate SQL",
        )),
    }
}

fn sql_table_ref<Q: Fn(&str) -> String>(
    table: &str,
    schema: Option<&str>,
    quote_ident: &Q,
) -> String {
    qualify_table_ident(schema, table, quote_ident)
}

fn insert_sql<Q, P>(
    table: &str,
    schema: Option<&str>,
    change: &RowChange,
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> String,
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
        placeholders.push(placeholder(i + 1, col_type));
        params.push(cell.clone().unwrap_or(Value::Null));
        preview_vals.push(format_typed_literal(cell, col_type));
    }
    let qtable = sql_table_ref(table, schema, quote_ident);
    Ok(SqlStatement {
        table: table.into(),
        operation: ChangeOperation::Insert,
        sql: format!(
            "INSERT INTO {qtable} ({cols}) VALUES ({})",
            placeholders.join(", ")
        ),
        preview_sql: format!(
            "INSERT INTO {qtable} ({cols}) VALUES ({})",
            preview_vals.join(", ")
        ),
        parameters: params,
        row_key: change.key.clone(),
    })
}

fn update_sql<Q, P>(
    table: &str,
    schema: Option<&str>,
    change: &RowChange,
    pk_columns: &[String],
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> String,
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
            placeholder(idx, col_type)
        ));
        set_lit.push(format!(
            "{} = {}",
            quote_ident(col),
            format_typed_literal(&cell, col_type)
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
    )?;
    params.extend(where_params);
    let qtable = sql_table_ref(table, schema, quote_ident);
    Ok(SqlStatement {
        table: table.into(),
        operation: ChangeOperation::Update,
        sql: format!(
            "UPDATE {qtable} SET {} WHERE {}",
            set_ph.join(", "),
            where_ph
        ),
        preview_sql: format!(
            "UPDATE {qtable} SET {} WHERE {}",
            set_lit.join(", "),
            where_lit
        ),
        parameters: params,
        row_key: change.key.clone(),
    })
}

fn delete_sql<Q, P>(
    table: &str,
    schema: Option<&str>,
    change: &RowChange,
    pk_columns: &[String],
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> String,
{
    let (where_ph, where_lit, params) = where_pk(
        pk_columns,
        &change.key,
        1,
        column_names,
        column_types,
        quote_ident,
        placeholder,
    )?;
    let qtable = sql_table_ref(table, schema, quote_ident);
    Ok(SqlStatement {
        table: table.into(),
        operation: ChangeOperation::Delete,
        sql: format!("DELETE FROM {qtable} WHERE {where_ph}"),
        preview_sql: format!("DELETE FROM {qtable} WHERE {where_lit}"),
        parameters: params,
        row_key: change.key.clone(),
    })
}

fn where_pk<Q, P>(
    pk_columns: &[String],
    key: &[Value],
    start_index: usize,
    column_names: &[String],
    column_types: &[String],
    quote_ident: &Q,
    placeholder: &P,
) -> Result<(String, String, Vec<Value>), DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize, Option<&str>) -> String,
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
                ph.push(format!("{} = {}", ident, placeholder(index, col_type)));
                lit.push(format!(
                    "{} = {}",
                    ident,
                    format_typed_literal(&Some(v.clone()), col_type)
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
    use crate::data_sync::model::{RowChange, SyncOptions};

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
            r#"UPDATE "clients" SET "name" = $1 WHERE "id" = $2"#
        );
        assert_eq!(stmts[1].parameters.len(), 2);
        assert_eq!(stmts[2].sql, r#"DELETE FROM "clients" WHERE "id" = $1"#);
        assert_eq!(
            stmts[2].preview_sql,
            r#"DELETE FROM "clients" WHERE "id" = 3"#
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
    fn format_literal_covers_value_kinds() {
        assert_eq!(format_literal(&None), "NULL");
        assert_eq!(format_literal(&Some(Value::Bool(true))), "TRUE");
        assert_eq!(format_literal(&Some(Value::Bool(false))), "FALSE");
        assert_eq!(format_literal(&Some(Value::Float(1.5))), "1.5");
        assert_eq!(
            format_literal(&Some(Value::String("o'reilly".into()))),
            "'o''reilly'"
        );
        assert!(format_literal(&Some(Value::Bytes(vec![65]))).contains('A'));
        assert!(format_literal(&Some(Value::Timestamp("t".into()))).contains("'t'"));
        assert!(format_literal(&Some(Value::Json(serde_json::json!({"a":1})))).contains('{'));
        assert_eq!(quote_ident_sql("na\"me", '"'), r#""na""me""#);
    }
}
