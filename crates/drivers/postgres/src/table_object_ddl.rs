//! Read-only DDL for a basic, explicitly addressed PostgreSQL table.
use datazen_driver_api::{CommandResult, ConnectionHandle, DriverError};
use serde_json::Value;

use crate::postgres::PostgresDriver;

// Both names are bound scalar values, never regclass syntax or a search-path
// lookup. A single statement reads one consistent catalog snapshot. This is a
// definition preview for ordinary tables, not a pg_dump or restore bundle.
const TABLE_DDL_SQL: &str = r#"
WITH relation AS (
    SELECT c.oid, n.nspname, c.relname, c.relpersistence
    FROM pg_catalog.pg_class c
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname::text = $1 AND c.relname::text = $2
      AND c.relkind = 'r' AND c.relpersistence IN ('p', 'u') AND NOT c.relispartition
      AND pg_catalog.has_schema_privilege(n.oid, 'USAGE')
      AND pg_catalog.has_table_privilege(c.oid, 'SELECT')
      AND NOT EXISTS (
          SELECT 1 FROM pg_catalog.pg_inherits inheritance
          WHERE inheritance.inhrelid = c.oid OR inheritance.inhparent = c.oid
      )
      AND NOT EXISTS (
          SELECT 1 FROM pg_catalog.pg_attribute a
          WHERE a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
            AND (a.attidentity <> '' OR a.attgenerated <> '')
      )
), columns AS (
    SELECT pg_catalog.string_agg(
        pg_catalog.format('  %I %s%s%s%s',
            a.attname, pg_catalog.format_type(a.atttypid, a.atttypmod),
            CASE WHEN a.attcollation <> tp.typcollation
                THEN pg_catalog.format(' COLLATE %I.%I', cn.nspname, col.collname)
                ELSE '' END,
            CASE WHEN ad.oid IS NOT NULL
                THEN ' DEFAULT ' || pg_catalog.pg_get_expr(ad.adbin, ad.adrelid)
                ELSE '' END,
            CASE WHEN a.attnotnull THEN ' NOT NULL' ELSE '' END
        ), E',\n' ORDER BY a.attnum
    ) AS definitions
    FROM relation r
    JOIN pg_catalog.pg_attribute a ON a.attrelid = r.oid
    JOIN pg_catalog.pg_type tp ON tp.oid = a.atttypid
    LEFT JOIN pg_catalog.pg_attrdef ad ON ad.adrelid = r.oid AND ad.adnum = a.attnum
    LEFT JOIN pg_catalog.pg_collation col ON col.oid = a.attcollation
    LEFT JOIN pg_catalog.pg_namespace cn ON cn.oid = col.collnamespace
    WHERE a.attnum > 0 AND NOT a.attisdropped
), constraints AS (
    SELECT pg_catalog.string_agg(
        pg_catalog.format('  CONSTRAINT %I %s',
            con.conname, pg_catalog.pg_get_constraintdef(con.oid)),
        E',\n' ORDER BY con.conname
    ) AS definitions
    FROM relation r
    JOIN pg_catalog.pg_constraint con ON con.conrelid = r.oid
    WHERE con.contype IN ('p', 'u', 'c', 'f', 'x')
), secondary AS (
    SELECT pg_catalog.string_agg(
        pg_catalog.pg_get_indexdef(i.indexrelid) || ';', E'\n' ORDER BY idx.relname
    ) AS definitions
    FROM relation r
    JOIN pg_catalog.pg_index i ON i.indrelid = r.oid
    JOIN pg_catalog.pg_class idx ON idx.oid = i.indexrelid
    WHERE i.indisvalid AND i.indisready AND i.indislive
      AND NOT EXISTS (
          SELECT 1 FROM pg_catalog.pg_constraint con
          WHERE con.conrelid = r.oid AND con.conindid = i.indexrelid
            AND con.contype IN ('p', 'u', 'x')
      )
)
SELECT pg_catalog.format('CREATE %sTABLE %I.%I (' || E'\n' || '%s' || E'\n' || ');%s',
    CASE WHEN r.relpersistence = 'u' THEN 'UNLOGGED ' ELSE '' END,
    r.nspname, r.relname,
    pg_catalog.concat_ws(E',\n', cols.definitions, cons.definitions),
    COALESCE(E'\n' || idx.definitions, '')
)
FROM relation r CROSS JOIN columns cols CROSS JOIN constraints cons CROSS JOIN secondary idx
"#;

pub(crate) async fn table_object_ddl(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    input: &Value,
) -> Result<CommandResult, DriverError> {
    let name = input
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| {
            DriverError::InvalidConfig("An exact PostgreSQL table name is required.".into())
        })?;
    let schema = input
        .get("schema")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| {
            DriverError::InvalidConfig("An exact PostgreSQL table schema is required.".into())
        })?;
    let database = input
        .get("database")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut connection = crate::session_connection::for_session(driver, handle, database).await?;
    let ddl: Option<String> = sqlx::query_scalar(TABLE_DDL_SQL)
        .bind(schema)
        .bind(name)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|error| DriverError::QueryFailed(error.to_string()))?;
    let ddl = ddl.ok_or_else(|| DriverError::Unsupported(
        "No readable ordinary PostgreSQL table definition is available for the selected namespace.".into(),
    ))?;
    Ok(CommandResult::new(serde_json::json!({ "ddl": ddl })))
}
