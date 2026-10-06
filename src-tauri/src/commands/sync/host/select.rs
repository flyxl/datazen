//! The single keyed read the apply handler uses to verify one target row.
//!
//! Split out of `host/mod.rs` so the SQL it builds — and, more importantly, the
//! refusals it makes instead of building SQL — can be read and tested without
//! dragging the whole host port along.

use std::collections::HashMap;

use crate::data_sync::{quote_ident_sql, DataSyncError};

use super::invalid;

/// `SELECT <all columns> … WHERE <pk> = <placeholder> … ORDER BY <pk> …` used by
/// the apply handler to verify one target row right before the batch writes it
/// (§5.3).
///
/// The predicate must **bind the reviewed key**: `HostTargetExecutor::read_by_key`
/// passes the key values to `query_with_params` in the same order, and the
/// handler reads at most one row. A `pk IS NOT NULL` scan would instead answer
/// with the first row of the table, which silently verifies — and can accept —
/// a row the review never saw.
///
/// **An empty `pk_columns` is refused, not degraded.** There is no predicate to
/// bind and no key the handler could have read, so the only statement that
/// could be produced here is a full-table `SELECT` whose first row would be
/// taken for the reviewed row. Returning an error makes the missing key visible
/// at the boundary that owns it; the earlier "empty order clause" fallback
/// turned the same defect into a silently full-table scan.
pub(super) fn select_by_key_sql(
    driver: &dyn crate::db::DatabaseDriver,
    quote: &char,
    database: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[String],
    pk_columns: &[String],
    column_types: &HashMap<String, String>,
) -> Result<String, DataSyncError> {
    if pk_columns.is_empty() {
        return Err(invalid(format!(
            "target relation {database}.{table} has no primary key, so its reviewed row cannot be re-read before the write (§5.3)"
        )));
    }
    let mut parts = vec![database.to_string()];
    if let Some(schema) = schema {
        parts.push(schema.to_string());
    }
    parts.push(table.to_string());
    let qualified = parts
        .iter()
        .map(|part| quote_ident_sql(part, *quote))
        .collect::<Vec<_>>()
        .join(".");
    let projection = columns
        .iter()
        .map(|column| quote_ident_sql(column, *quote))
        .collect::<Vec<_>>()
        .join(", ");
    let predicate = pk_columns
        .iter()
        .enumerate()
        .map(|(offset, pk)| {
            // Placeholder indexes are 1-based and positional: the key values the
            // executor binds land in exactly this order.
            let placeholder = driver
                .parameter_placeholder(offset + 1, column_types.get(pk).map(String::as_str))
                .map_err(|error| invalid(error.to_string()))?;
            Ok(format!("{} = {placeholder}", quote_ident_sql(pk, *quote)))
        })
        .collect::<Result<Vec<_>, DataSyncError>>()?
        .join(" AND ");
    let order = pk_columns
        .iter()
        .map(|pk| quote_ident_sql(pk, *quote))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "SELECT {projection} FROM {qualified} WHERE {predicate} ORDER BY {order}"
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};

    use super::select_by_key_sql;

    fn columns() -> Vec<String> {
        ["id", "name"].map(String::from).to_vec()
    }

    fn types() -> HashMap<String, String> {
        [("id".to_string(), "int4".to_string())]
            .into_iter()
            .collect()
    }

    fn postgres() -> Arc<MockDriver> {
        MockDriver::new("postgresql", MockDriverOptions::default())
    }

    #[test]
    fn a_keyed_read_binds_the_reviewed_key() {
        let sql = select_by_key_sql(
            postgres().as_ref(),
            &'"',
            "app",
            None,
            "users",
            &columns(),
            &["id".to_string()],
            &types(),
        )
        .expect("a relation with a primary key must produce a keyed read");
        assert_eq!(
            sql, r#"SELECT "id", "name" FROM "app"."users" WHERE "id" = $1 ORDER BY "id""#,
            "the predicate must bind the reviewed key and never widen to a scan"
        );
    }

    #[test]
    fn a_composite_key_binds_placeholder_indexes_in_key_order() {
        let sql = select_by_key_sql(
            postgres().as_ref(),
            &'"',
            "app",
            Some("public"),
            "users",
            &columns(),
            &["id".to_string(), "name".to_string()],
            &types(),
        )
        .expect("a composite key must produce a keyed read");
        assert_eq!(
            sql,
            r#"SELECT "id", "name" FROM "app"."public"."users" WHERE "id" = $1 AND "name" = $2 ORDER BY "id", "name""#,
            "the executor binds key values positionally, so placeholder indexes must follow key order"
        );
    }

    #[test]
    fn a_relation_without_a_primary_key_is_refused_instead_of_scanned() {
        let error = select_by_key_sql(
            postgres().as_ref(),
            &'"',
            "app",
            None,
            "users",
            &columns(),
            &[],
            &types(),
        )
        .expect_err("no key means no reviewed row can be re-read; a full-table SELECT must never be returned");
        assert!(
            error.to_string().contains("no primary key"),
            "the refusal must name the missing key, got: {error}"
        );
    }
}
