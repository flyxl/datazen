//! DuckDbSchemaObjectSql dialect SQL for the shared schema-object commands.
//!
//! The `list_objects` / `get_object_ddl` / `list_privileges` commands are
//! implemented once in the driver API; the SQL is dialect knowledge and
//! therefore lives here, next to the driver that runs it.

use datazen_driver_api::schema_object_sql::{ObjectDdlRequest, SchemaObjectSql};
use datazen_driver_api::schema_objects::{quote_ident, sql_string, ObjectKind};

/// DuckDbSchemaObjectSql's dialect SQL provider.
pub struct DuckDbSchemaObjectSql;

impl SchemaObjectSql for DuckDbSchemaObjectSql {
    fn list_objects_sql(&self, kind: ObjectKind) -> Option<String> {
        match kind {
            ObjectKind::Trigger => Some(
                "SELECT NULL AS schema, trigger_name AS name \
             FROM information_schema.triggers \
             ORDER BY 1, 2"
                    .into(),
            ),
            ObjectKind::Sequence => Some(
                "SELECT sequence_schema AS schema, sequence_name AS name \
             FROM information_schema.sequences \
             WHERE sequence_schema NOT IN ('information_schema') \
             ORDER BY 1, 2"
                    .into(),
            ),
            _ => None,
        }
    }

    fn object_ddl_sql(&self, request: &ObjectDdlRequest<'_>) -> Option<String> {
        let ObjectDdlRequest {
            kind,
            name,
            schema,
            signature,
            target_schema,
            target_name,
        } = *request;
        let _ = (signature, target_schema, target_name);
        let ident = quote_ident("duckdb", name);
        let schema_ident = schema
            .filter(|s| !s.is_empty())
            .map(|s| quote_ident("duckdb", s));
        let qualified = match &schema_ident {
            Some(s) => format!("{s}.{ident}"),
            None => ident.clone(),
        };
        // Some families qualify the name in their DDL, others build their
        // own quoted target; the value stays available for the former.
        let _ = &qualified;
        match kind {
            ObjectKind::Trigger => Some(format!(
            "SELECT sql AS ddl FROM information_schema.triggers WHERE trigger_name = {} LIMIT 1",
            sql_string(name),
        )),
            ObjectKind::Sequence => Some(format!(
                "SELECT sql AS ddl FROM duckdb_sequences() WHERE sequence_name = {} LIMIT 1",
                sql_string(name),
            )),
            _ => None,
        }
    }

    fn list_privileges_sql(&self) -> Option<String> {
        None
    }
}
