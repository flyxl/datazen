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

pub fn mysql_placeholder(_index: usize) -> String {
    "?".into()
}

pub fn postgres_placeholder(index: usize) -> String {
    format!("${index}")
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

pub fn generate_table_sql<Q, P>(
    table: &TableChangeSet,
    pk_columns: &[String],
    column_names: &[String],
    quote_ident: Q,
    placeholder: P,
) -> Result<Vec<SqlStatement>, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize) -> String,
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
            change,
            pk_columns,
            column_names,
            &quote_ident,
            &placeholder,
        )?);
    }
    Ok(out)
}

fn statement_for_change<Q, P>(
    table: &str,
    change: &RowChange,
    pk_columns: &[String],
    column_names: &[String],
    quote_ident: &Q,
    placeholder: &P,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize) -> String,
{
    match change.operation {
        ChangeOperation::Insert => {
            insert_sql(table, change, column_names, quote_ident, placeholder)
        }
        ChangeOperation::Update => update_sql(
            table,
            change,
            pk_columns,
            column_names,
            quote_ident,
            placeholder,
        ),
        ChangeOperation::Delete => delete_sql(table, change, pk_columns, quote_ident, placeholder),
        ChangeOperation::Unchanged => Err(DataSyncError::validation(
            "unchanged rows must not generate SQL",
        )),
    }
}

fn insert_sql<Q, P>(
    table: &str,
    change: &RowChange,
    column_names: &[String],
    quote_ident: &Q,
    placeholder: &P,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize) -> String,
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
        placeholders.push(placeholder(i + 1));
        params.push(cell.clone().unwrap_or(Value::Null));
        preview_vals.push(format_literal(cell));
    }
    let qtable = quote_ident(table);
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
    change: &RowChange,
    pk_columns: &[String],
    column_names: &[String],
    quote_ident: &Q,
    placeholder: &P,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize) -> String,
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
        set_ph.push(format!("{} = {}", quote_ident(col), placeholder(idx)));
        set_lit.push(format!("{} = {}", quote_ident(col), format_literal(&cell)));
        params.push(cell.unwrap_or(Value::Null));
        idx += 1;
    }
    let (where_ph, where_lit, where_params) =
        where_pk(pk_columns, &change.key, idx, quote_ident, placeholder)?;
    params.extend(where_params);
    let qtable = quote_ident(table);
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
    change: &RowChange,
    pk_columns: &[String],
    quote_ident: &Q,
    placeholder: &P,
) -> Result<SqlStatement, DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize) -> String,
{
    let (where_ph, where_lit, params) =
        where_pk(pk_columns, &change.key, 1, quote_ident, placeholder)?;
    let qtable = quote_ident(table);
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
    quote_ident: &Q,
    placeholder: &P,
) -> Result<(String, String, Vec<Value>), DataSyncError>
where
    Q: Fn(&str) -> String,
    P: Fn(usize) -> String,
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
                ph.push(format!("{} = {}", ident, placeholder(index)));
                lit.push(format!("{} = {}", ident, format_literal(&Some(v.clone()))));
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
            &["id".into()],
            &["id".into(), "name".into()],
            q,
            postgres_placeholder,
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
            &["id".into()],
            &["id".into()],
            |n| quote_ident_sql(n, '`'),
            mysql_placeholder,
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
            generate_table_sql(&table, &["id".into()], &["id".into()], q, mysql_placeholder)
                .is_err()
        );
        assert!(generate_table_sql(&table, &[], &["id".into()], q, mysql_placeholder).is_err());
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
        let stmts = generate_table_sql(
            &table,
            &["id".into()],
            &["id".into()],
            q,
            postgres_placeholder,
        )
        .unwrap();
        assert_eq!(stmts[0].sql, r#"DELETE FROM "t" WHERE "id" IS NULL"#);
        assert!(stmts[0].parameters.is_empty());
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
