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
    P: Fn(usize) -> Result<String, DataSyncError>,
{
    let key_idents = pk_columns
        .iter()
        .map(|c| quote_ident_sql(c, quote))
        .collect::<Vec<_>>();
    build_keyset_select_sql_with_order(
        table,
        database,
        schema,
        family,
        columns,
        pk_columns,
        &key_idents,
        after_key,
        limit,
        quote,
        placeholder,
    )
}

/// Build keyset SQL using driver-owned expressions for the key order.
///
/// The expressions are used in both the tuple seek predicate and `ORDER BY`.
/// This is what lets a driver make binary text ordering explicit when the
/// session's default collation is case-insensitive or otherwise unstable.
pub fn build_keyset_select_sql_with_order<P>(
    table: &str,
    database: Option<&str>,
    schema: Option<&str>,
    family: &str,
    columns: &[String],
    pk_columns: &[String],
    key_order_expressions: &[String],
    after_key: Option<&[Value]>,
    limit: u32,
    quote: char,
    placeholder: P,
) -> Result<(String, Vec<Value>), DataSyncError>
where
    P: Fn(usize) -> Result<String, DataSyncError>,
{
    build_keyset_select_sql_with_order_and_filter(
        table,
        database,
        schema,
        family,
        columns,
        pk_columns,
        key_order_expressions,
        after_key,
        limit,
        quote,
        placeholder,
        None,
    )
}

/// Build a keyset page with a driver-owned pagination clause and an additional
/// server-generated, parameterized predicate. The filter placeholders must
/// already use indexes after the seek-key placeholders, and its parameters are
/// appended after the seek key.
pub fn build_keyset_select_sql_with_order_and_filter<P>(
    table: &str,
    database: Option<&str>,
    schema: Option<&str>,
    family: &str,
    columns: &[String],
    pk_columns: &[String],
    key_order_expressions: &[String],
    after_key: Option<&[Value]>,
    limit: u32,
    quote: char,
    placeholder: P,
    filter: Option<(&str, &[Value])>,
) -> Result<(String, Vec<Value>), DataSyncError>
where
    P: Fn(usize) -> Result<String, DataSyncError>,
{
    let pagination_clause = format!("LIMIT {}", limit.max(1));
    build_keyset_select_sql_with_order_filter_and_pagination(
        table,
        database,
        schema,
        family,
        columns,
        pk_columns,
        key_order_expressions,
        after_key,
        &pagination_clause,
        quote,
        placeholder,
        filter,
    )
}

pub fn build_keyset_select_sql_with_order_filter_and_pagination<P>(
    table: &str,
    database: Option<&str>,
    schema: Option<&str>,
    family: &str,
    columns: &[String],
    pk_columns: &[String],
    key_order_expressions: &[String],
    after_key: Option<&[Value]>,
    pagination_clause: &str,
    quote: char,
    placeholder: P,
    filter: Option<(&str, &[Value])>,
) -> Result<(String, Vec<Value>), DataSyncError>
where
    P: Fn(usize) -> Result<String, DataSyncError>,
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
    if pagination_clause.trim().is_empty() {
        return Err(DataSyncError::validation(
            "keyset paging requires a pagination clause from the driver",
        ));
    }
    if key_order_expressions.len() != pk_columns.len() {
        return Err(DataSyncError::validation(
            "key order expression count does not match primary key columns",
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
    let order_cols = key_order_expressions
        .iter()
        .map(|c| format!("{c} ASC"))
        .collect::<Vec<_>>()
        .join(", ");

    let mut params = Vec::new();
    let mut where_clauses = Vec::new();
    if let Some(key) = after_key {
        // T-SQL has no row-value comparison syntax. Expand the seek into a
        // portable lexicographic disjunction, keeping every expression in the
        // same order used by ORDER BY. A placeholder is emitted for each
        // occurrence so both anonymous (`?`) and positional drivers can bind
        // the predicate without dialect-specific rewriting.
        let mut next_parameter = 1usize;
        let mut alternatives = Vec::with_capacity(key.len());
        for index in 0..key.len() {
            let mut terms = Vec::with_capacity(index + 1);
            for equal_index in 0..index {
                let ph = placeholder(next_parameter)?;
                next_parameter += 1;
                terms.push(format!("{} = {ph}", key_order_expressions[equal_index]));
                params.push(key[equal_index].clone());
            }
            let ph = placeholder(next_parameter)?;
            next_parameter += 1;
            terms.push(format!("{} > {ph}", key_order_expressions[index]));
            params.push(key[index].clone());
            alternatives.push(format!("({})", terms.join(" AND ")));
        }
        where_clauses.push(if alternatives.len() == 1 {
            alternatives.remove(0)
        } else {
            format!("({})", alternatives.join(" OR "))
        });
    }
    if let Some((filter_sql, filter_params)) = filter {
        if filter_sql.trim().is_empty() {
            return Err(DataSyncError::validation("sync filter predicate is empty"));
        }
        let predicate = filter_sql
            .trim()
            .strip_prefix("WHERE ")
            .or_else(|| filter_sql.trim().strip_prefix("where "))
            .ok_or_else(|| {
                DataSyncError::validation("sync filter predicate must start with WHERE")
            })?;
        if predicate.trim().is_empty() {
            return Err(DataSyncError::validation("sync filter predicate is empty"));
        }
        where_clauses.push(format!("({predicate})"));
        params.extend_from_slice(filter_params);
    }
    let where_clause = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    let qualified = super::sql::qualify_relation_sql(family, database, schema, table, quote);
    let sql = format!(
        "SELECT {select_cols} FROM {qualified}{where_clause} ORDER BY {order_cols} {pagination_clause}"
    );
    Ok((sql, params))
}

/// Number of bound seek parameters emitted for a composite key's portable
/// lexicographic predicate, or zero when there is no `after_key`.
pub fn keyset_seek_parameter_count(key_count: usize, has_after_key: bool) -> usize {
    if has_after_key {
        key_count.saturating_mul(key_count.saturating_add(1)) / 2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mysql_placeholder, postgres_placeholder};

    fn mysql_ph(index: usize) -> Result<String, DataSyncError> {
        Ok(mysql_placeholder(index))
    }

    fn postgres_ph(index: usize) -> Result<String, DataSyncError> {
        Ok(postgres_placeholder(index))
    }

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
            mysql_ph,
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
            mysql_ph,
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
            mysql_ph,
        )
        .unwrap();
        assert_eq!(params.len(), 1);
        assert!(matches!(params[0], Value::Integer(42)));
        assert_eq!(
            sql,
            "SELECT `id`, `name`, `age` FROM `users` WHERE (`id` > ?) ORDER BY `id` ASC LIMIT 50"
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
            mysql_ph,
        )
        .unwrap();
        assert_eq!(params.len(), 3);
        assert!(matches!(params[0], Value::Integer(1)));
        assert!(matches!(params[1], Value::Integer(1)));
        assert!(matches!(params[2], Value::String(ref s) if s == "east"));
        assert_eq!(
            sql,
            "SELECT `tenant`, `region`, `n` FROM `shards` WHERE ((`tenant` > ?) OR (`tenant` = ? AND `region` > ?)) \
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
            postgres_ph,
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
            postgres_ph,
        )
        .unwrap();
        assert_eq!(params.len(), 3);
        assert!(matches!(params[0], Value::Integer(2)));
        assert!(matches!(params[1], Value::Integer(2)));
        assert!(matches!(params[2], Value::String(ref s) if s == "west"));
        assert_eq!(
            sql,
            "SELECT \"tenant\", \"region\", \"n\" FROM \"shards\" \
             WHERE ((\"tenant\" > $1) OR (\"tenant\" = $2 AND \"region\" > $3)) \
             ORDER BY \"tenant\" ASC, \"region\" ASC LIMIT 5"
        );
    }

    #[test]
    fn parameterized_filter_is_appended_after_seek_parameters() {
        let (sql, params) = build_keyset_select_sql_with_order_and_filter(
            "users",
            None,
            None,
            "postgresql",
            &cols(),
            &pk1(),
            &["\"id\"".into()],
            Some(&[Value::Integer(9)]),
            10,
            '"',
            postgres_ph,
            Some(("WHERE (\"name\" = $2)", &[Value::String("active".into())])),
        )
        .unwrap();
        assert!(matches!(
            params.as_slice(),
            [Value::Integer(9), Value::String(value)] if value == "active"
        ));
        assert_eq!(
            sql,
            "SELECT \"id\", \"name\", \"age\" FROM \"users\" WHERE (\"id\" > $1) AND ((\"name\" = $2)) ORDER BY \"id\" ASC LIMIT 10"
        );
    }

    #[test]
    fn tuple_filter_and_seek_share_driver_order_and_placeholder_sequence() {
        let columns = vec!["tenant".into(), "id".into(), "value".into()];
        let keys = vec!["tenant".into(), "id".into()];
        let order = vec![r#""tenant" COLLATE "C""#.into(), r#""id""#.into()];
        let filter_params = [Value::String("1".into()), Value::String("5".into())];
        let (sql, params) = build_keyset_select_sql_with_order_and_filter(
            "events",
            None,
            Some("public"),
            "postgresql",
            &columns,
            &keys,
            &order,
            Some(&[Value::String("0".into()), Value::Integer(9)]),
            25,
            '"',
            postgres_ph,
            Some((
                r#"WHERE ("tenant" COLLATE "C", "id") >= ($4::text, $5::integer)"#,
                &filter_params,
            )),
        )
        .unwrap();
        assert!(matches!(
            params.as_slice(),
            [
                Value::String(seek_tenant),
                Value::String(seek_tenant_equal),
                Value::Integer(seek_id),
                Value::String(filter_tenant),
                Value::String(filter_id),
            ] if seek_tenant == "0" && seek_tenant_equal == "0" && *seek_id == 9 && filter_tenant == "1" && filter_id == "5"
        ));
        assert_eq!(
            sql,
            r#"SELECT "tenant", "id", "value" FROM "public"."events" WHERE (("tenant" COLLATE "C" > $1) OR ("tenant" COLLATE "C" = $2 AND "id" > $3)) AND (("tenant" COLLATE "C", "id") >= ($4::text, $5::integer)) ORDER BY "tenant" COLLATE "C" ASC, "id" ASC LIMIT 25"#
        );
    }

    #[test]
    fn driver_pagination_clause_is_rendered_after_ordering() {
        let (sql, params) = build_keyset_select_sql_with_order_filter_and_pagination(
            "users",
            None,
            Some("dbo"),
            "sqlserver",
            &cols(),
            &pk1(),
            &["[id]".into()],
            Some(&[Value::Integer(9)]),
            "OFFSET 0 ROWS FETCH NEXT 25 ROWS ONLY",
            '[',
            |index| Ok(format!("@p{index}")),
            None,
        )
        .unwrap();
        assert!(matches!(params.as_slice(), [Value::Integer(9)]));
        assert_eq!(
            sql,
            "SELECT [id], [name], [age] FROM [dbo].[users] WHERE ([id] > @p1) ORDER BY [id] ASC OFFSET 0 ROWS FETCH NEXT 25 ROWS ONLY"
        );
    }

    #[test]
    fn sqlserver_composite_keyset_uses_lexicographic_seek_and_driver_pagination() {
        let columns = vec!["tenant".into(), "sequence".into(), "value".into()];
        let keys = vec!["tenant".into(), "sequence".into()];
        let (sql, params) = build_keyset_select_sql_with_order_filter_and_pagination(
            "events",
            Some("archive"),
            Some("dbo"),
            "sqlserver",
            &columns,
            &keys,
            &["[tenant]".into(), "[sequence]".into()],
            Some(&[Value::Integer(3), Value::Integer(12)]),
            "OFFSET 0 ROWS FETCH NEXT 25 ROWS ONLY",
            '[',
            |index| Ok(format!("@P{index}")),
            None,
        )
        .unwrap();
        assert!(matches!(
            params.as_slice(),
            [Value::Integer(3), Value::Integer(3), Value::Integer(12)]
        ));
        assert_eq!(
            sql,
            "SELECT [tenant], [sequence], [value] FROM [archive].[dbo].[events] WHERE (([tenant] > @P1) OR ([tenant] = @P2 AND [sequence] > @P3)) ORDER BY [tenant] ASC, [sequence] ASC OFFSET 0 ROWS FETCH NEXT 25 ROWS ONLY"
        );
    }

    #[test]
    fn keyset_placeholder_error_is_returned_before_query_execution() {
        let error = build_keyset_select_sql(
            "events",
            None,
            Some("dbo"),
            "sqlserver",
            &cols(),
            &pk1(),
            Some(&[Value::Integer(1)]),
            10,
            '[',
            |_| Err(DataSyncError::validation("parameter limit exceeded")),
        )
        .expect_err("unsupported placeholder must not degrade to another dialect");
        assert!(error.to_string().contains("parameter limit exceeded"));
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
            postgres_ph,
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
            postgres_ph,
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
            postgres_ph,
        )
        .unwrap();
        assert!(params.is_empty());
        assert_eq!(
            sql,
            "SELECT \"id\", \"name\", \"age\" FROM \"public\".\"users\" ORDER BY \"id\" ASC LIMIT 10"
        );
    }

    #[test]
    fn driver_order_expression_is_used_for_seek_and_order() {
        let (sql, params) = build_keyset_select_sql_with_order(
            "users",
            None,
            None,
            "postgresql",
            &cols(),
            &pk1(),
            &[r#""id" COLLATE "C""#.into()],
            Some(&[Value::String("a".into())]),
            10,
            '"',
            postgres_ph,
        )
        .unwrap();
        assert_eq!(params.len(), 1);
        assert!(sql.contains(r#"WHERE ("id" COLLATE "C" > $1)"#));
        assert!(sql.contains(r#"ORDER BY "id" COLLATE "C" ASC"#));
    }
}
