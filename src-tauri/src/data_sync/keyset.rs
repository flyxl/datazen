//! Keyset (seek) pagination SQL for same-family Data Sync compare.

use datazen_driver_api::Value;

use super::error::DataSyncError;
use super::sql::quote_ident_sql;

/// Build a parameterized `SELECT … ORDER BY pk LIMIT n` for keyset paging.
///
/// First page omits `WHERE`; subsequent pages use tuple comparison
/// `(pk1, pk2, …) > (placeholder…)` matching the PK `ORDER BY`.
pub fn build_keyset_select_sql<P>(
    table: &str,
    database: Option<&str>,
    schema: Option<&str>,
    family: &str,
    columns: &[String],
    pk_columns: &[String],
    after_key: Option<&[Value]>,
    limit: u32,
    quote: char,
    placeholder: P,
) -> Result<(String, Vec<Value>), DataSyncError>
where
    P: Fn(usize) -> String,
{
    if pk_columns.is_empty() {
        return Err(DataSyncError::validation(
            "keyset paging requires at least one primary key column",
        ));
    }
    if columns.is_empty() {
        return Err(DataSyncError::validation(
            "keyset paging requires at least one selected column",
        ));
    }
    if let Some(key) = after_key {
        if key.len() != pk_columns.len() {
            return Err(DataSyncError::validation(format!(
                "after_key length {} does not match pk column count {}",
                key.len(),
                pk_columns.len()
            )));
        }
    }

    let select_cols = columns
        .iter()
        .map(|c| quote_ident_sql(c, quote))
        .collect::<Vec<_>>()
        .join(", ");
    let order_cols = pk_columns
        .iter()
        .map(|c| format!("{} ASC", quote_ident_sql(c, quote)))
        .collect::<Vec<_>>()
        .join(", ");

    let mut params = Vec::new();
    let where_clause = if let Some(key) = after_key {
        let pk_idents = pk_columns
            .iter()
            .map(|c| quote_ident_sql(c, quote))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders: Vec<String> = (1..=pk_columns.len()).map(|i| placeholder(i)).collect();
        params.extend_from_slice(key);
        format!(" WHERE ({pk_idents}) > ({})", placeholders.join(", "))
    } else {
        String::new()
    };

    let qualified = super::sql::qualify_relation_sql(family, database, schema, table, quote);
    let sql = format!(
        "SELECT {select_cols} FROM {qualified}{where_clause} ORDER BY {order_cols} LIMIT {limit}",
        limit = limit.max(1),
    );
    Ok((sql, params))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_sync::{mysql_placeholder, postgres_placeholder};

    fn cols() -> Vec<String> {
        vec!["id".into(), "name".into(), "age".into()]
    }

    fn pk1() -> Vec<String> {
        vec!["id".into()]
    }

    fn pk2() -> Vec<String> {
        vec!["tenant".into(), "region".into()]
    }

    #[test]
    fn mysql_first_page_single_pk() {
        let (sql, params) = build_keyset_select_sql(
            "users",
            None,
            None,
            "mysql",
            &cols(),
            &pk1(),
            None,
            100,
            '`',
            mysql_placeholder,
        )
        .unwrap();
        assert!(params.is_empty());
        assert_eq!(
            sql,
            "SELECT `id`, `name`, `age` FROM `users` ORDER BY `id` ASC LIMIT 100"
        );
    }

    #[test]
    fn mysql_catalog_qualified_table() {
        let (sql, params) = build_keyset_select_sql(
            "users",
            Some("mydb"),
            None,
            "mysql",
            &cols(),
            &pk1(),
            None,
            100,
            '`',
            mysql_placeholder,
        )
        .unwrap();
        assert!(params.is_empty());
        assert_eq!(
            sql,
            "SELECT `id`, `name`, `age` FROM `mydb`.`users` ORDER BY `id` ASC LIMIT 100"
        );
    }

    #[test]
    fn mysql_next_page_single_pk() {
        let (sql, params) = build_keyset_select_sql(
            "users",
            None,
            None,
            "mysql",
            &cols(),
            &pk1(),
            Some(&[Value::Integer(42)]),
            50,
            '`',
            mysql_placeholder,
        )
        .unwrap();
        assert_eq!(params.len(), 1);
        assert!(matches!(params[0], Value::Integer(42)));
        assert_eq!(
            sql,
            "SELECT `id`, `name`, `age` FROM `users` WHERE (`id`) > (?) ORDER BY `id` ASC LIMIT 50"
        );
    }

    #[test]
    fn mysql_next_page_composite_pk() {
        let cols = vec!["tenant".into(), "region".into(), "n".into()];
        let (sql, params) = build_keyset_select_sql(
            "shards",
            None,
            None,
            "mysql",
            &cols,
            &pk2(),
            Some(&[Value::Integer(1), Value::String("east".into())]),
            10,
            '`',
            mysql_placeholder,
        )
        .unwrap();
        assert_eq!(params.len(), 2);
        assert!(matches!(params[0], Value::Integer(1)));
        assert!(matches!(params[1], Value::String(ref s) if s == "east"));
        assert_eq!(
            sql,
            "SELECT `tenant`, `region`, `n` FROM `shards` WHERE (`tenant`, `region`) > (?, ?) \
             ORDER BY `tenant` ASC, `region` ASC LIMIT 10"
        );
    }

    #[test]
    fn postgres_first_page_single_pk() {
        let (sql, params) = build_keyset_select_sql(
            "users",
            None,
            None,
            "postgresql",
            &cols(),
            &pk1(),
            None,
            25,
            '"',
            postgres_placeholder,
        )
        .unwrap();
        assert!(params.is_empty());
        assert_eq!(
            sql,
            "SELECT \"id\", \"name\", \"age\" FROM \"users\" ORDER BY \"id\" ASC LIMIT 25"
        );
    }

    #[test]
    fn postgres_next_page_composite_pk() {
        let cols = vec!["tenant".into(), "region".into(), "n".into()];
        let (sql, params) = build_keyset_select_sql(
            "shards",
            None,
            None,
            "postgresql",
            &cols,
            &pk2(),
            Some(&[Value::Integer(2), Value::String("west".into())]),
            5,
            '"',
            postgres_placeholder,
        )
        .unwrap();
        assert_eq!(params.len(), 2);
        assert!(matches!(params[0], Value::Integer(2)));
        assert!(matches!(params[1], Value::String(ref s) if s == "west"));
        assert_eq!(
            sql,
            "SELECT \"tenant\", \"region\", \"n\" FROM \"shards\" \
             WHERE (\"tenant\", \"region\") > ($1, $2) \
             ORDER BY \"tenant\" ASC, \"region\" ASC LIMIT 5"
        );
    }

    #[test]
    fn rejects_empty_pk() {
        let err = build_keyset_select_sql(
            "t",
            None,
            None,
            "postgresql",
            &cols(),
            &[],
            None,
            1,
            '"',
            postgres_placeholder,
        )
        .unwrap_err();
        assert!(err.to_string().contains("primary key"));
    }

    #[test]
    fn rejects_mismatched_after_key() {
        let err = build_keyset_select_sql(
            "t",
            None,
            None,
            "postgresql",
            &cols(),
            &pk2(),
            Some(&[Value::Integer(1)]),
            1,
            '"',
            postgres_placeholder,
        )
        .unwrap_err();
        assert!(err.to_string().contains("after_key length"));
    }

    #[test]
    fn postgres_schema_qualified_table() {
        let (sql, params) = build_keyset_select_sql(
            "users",
            None,
            Some("public"),
            "postgresql",
            &cols(),
            &pk1(),
            None,
            10,
            '"',
            postgres_placeholder,
        )
        .unwrap();
        assert!(params.is_empty());
        assert_eq!(
            sql,
            "SELECT \"id\", \"name\", \"age\" FROM \"public\".\"users\" ORDER BY \"id\" ASC LIMIT 10"
        );
    }
}
