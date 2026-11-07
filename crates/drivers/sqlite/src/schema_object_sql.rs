//! SqliteSchemaObjectSql dialect SQL for the shared schema-object commands.
//!
//! The `list_objects` / `get_object_ddl` / `list_privileges` commands are
//! implemented once in the driver API; the SQL is dialect knowledge and
//! therefore lives here, next to the driver that runs it.

use datazen_driver_api::schema_object_sql::{ObjectDdlRequest, SchemaObjectSql};
use datazen_driver_api::schema_objects::{quote_ident, sql_string, ObjectKind};

/// SqliteSchemaObjectSql's dialect SQL provider.
pub struct SqliteSchemaObjectSql;

impl SchemaObjectSql for SqliteSchemaObjectSql {
    fn list_objects_sql(&self, kind: ObjectKind) -> Option<String> {
        match kind {
            ObjectKind::View => Some(
                "SELECT NULL AS schema, name FROM sqlite_master \
             WHERE type = 'view' ORDER BY name"
                    .into(),
            ),
            ObjectKind::Trigger => Some(
                "SELECT NULL AS schema, name, NULL AS target_schema, tbl_name AS target_name \
                 FROM sqlite_master WHERE type = 'trigger' ORDER BY name"
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
        let ident = quote_ident("sqlite", name);
        let schema_ident = schema
            .filter(|s| !s.is_empty())
            .map(|s| quote_ident("sqlite", s));
        let qualified = match &schema_ident {
            Some(s) => format!("{s}.{ident}"),
            None => ident.clone(),
        };
        // Some families qualify the name in their DDL, others build their
        // own quoted target; the value stays available for the former.
        let _ = &qualified;
        match kind {
        ObjectKind::Table => Some(format!(
            "SELECT sql AS ddl FROM sqlite_master WHERE type = 'table' AND name = {}",
            sql_string(name),
        )),
        ObjectKind::View => Some(format!(
            "WITH view_source AS ( \
             SELECT sql, instr(lower(replace(replace(replace(sql, char(13), ' '), char(10), ' '), char(9), ' ')), ' as ') AS as_marker \
             FROM sqlite_master WHERE type = 'view' AND name = {} \
             ) \
             SELECT trim(substr(sql, as_marker + 3 \
                 + length(substr(sql, as_marker + 3)) \
                 - length(ltrim(substr(sql, as_marker + 3), char(9) || char(10) || char(13) || ' '))), \
                 char(9) || char(10) || char(13) || ' ') AS ddl \
             FROM view_source",
            sql_string(name),
        )),
        ObjectKind::Trigger => Some(format!(
            "SELECT sql AS ddl FROM sqlite_master WHERE type = 'trigger' AND name = {}",
            sql_string(name),
        )),
            _ => None,
        }
    }

    fn list_privileges_sql(&self) -> Option<String> {
        None
    }
}
