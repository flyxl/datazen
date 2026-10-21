//! MysqlSchemaObjectSql dialect SQL for the shared schema-object commands.
//!
//! The `list_objects` / `get_object_ddl` / `list_privileges` commands are
//! implemented once in the driver API; the SQL is dialect knowledge and
//! therefore lives here, next to the driver that runs it.

use datazen_driver_api::schema_object_sql::{ObjectDdlRequest, SchemaObjectSql};
use datazen_driver_api::schema_objects::{quote_ident, sql_string, ObjectKind};

/// MysqlSchemaObjectSql's dialect SQL provider.
pub struct MysqlSchemaObjectSql;

impl SchemaObjectSql for MysqlSchemaObjectSql {
    fn list_objects_sql(&self, kind: ObjectKind) -> Option<String> {
        match kind {
            ObjectKind::Function => Some(
                "SELECT NULL AS `schema`, ROUTINE_NAME AS name \
             FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = DATABASE() AND ROUTINE_TYPE = 'FUNCTION' \
             ORDER BY 1, 2"
                    .into(),
            ),
            ObjectKind::Procedure => Some(
                "SELECT NULL AS `schema`, ROUTINE_NAME AS name \
             FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = DATABASE() AND ROUTINE_TYPE = 'PROCEDURE' \
             ORDER BY 1, 2"
                    .into(),
            ),
            ObjectKind::Trigger => Some(
                "SELECT NULL AS `schema`, TRIGGER_NAME AS name, \
                    NULL AS target_schema, EVENT_OBJECT_TABLE AS target_name \
             FROM information_schema.TRIGGERS \
             WHERE TRIGGER_SCHEMA = DATABASE() \
             ORDER BY 1, 2, 4"
                    .into(),
            ),
            ObjectKind::View => Some(
                "SELECT NULL AS `schema`, TABLE_NAME AS name \
             FROM information_schema.VIEWS WHERE TABLE_SCHEMA = DATABASE() \
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
        let ident = quote_ident("mysql", name);
        let schema_ident = schema
            .filter(|s| !s.is_empty())
            .map(|s| quote_ident("mysql", s));
        let qualified = match &schema_ident {
            Some(s) => format!("{s}.{ident}"),
            None => ident.clone(),
        };
        // Some families qualify the name in their DDL, others build their
        // own quoted target; the value stays available for the former.
        let _ = &qualified;
        match kind {
            ObjectKind::Table => Some(format!("SHOW CREATE TABLE {qualified}")),
            ObjectKind::View => Some(format!(
            "SELECT TABLE_SCHEMA AS view_schema, VIEW_DEFINITION AS ddl, DEFINER AS view_definer, \
             SECURITY_TYPE AS view_security_type, CHECK_OPTION AS view_check_option, \
             CHARACTER_SET_CLIENT AS view_character_set_client, \
             COLLATION_CONNECTION AS view_collation_connection \
             FROM information_schema.VIEWS \
             WHERE TABLE_SCHEMA = COALESCE(NULLIF({}, ''), DATABASE()) AND TABLE_NAME = {}",
            sql_string(schema.filter(|s| !s.is_empty()).unwrap_or("")),
            sql_string(name),
        )),
            ObjectKind::Function => Some(format!("SHOW CREATE FUNCTION {qualified}")),
            ObjectKind::Procedure => Some(format!("SHOW CREATE PROCEDURE {qualified}")),
            ObjectKind::Trigger => Some(format!("SHOW CREATE TRIGGER {qualified}")),
            _ => None,
        }
    }

    /// MySQL's `*_SCHEMA` columns identify databases, not a separate namespace
    /// schema, so the DDL lookup resolves the catalog instead.
    fn ddl_schema_scope<'a>(
        &self,
        schema: Option<&'a str>,
        database: Option<&'a str>,
    ) -> Option<&'a str> {
        schema.or(database)
    }

    /// `information_schema.VIEWS.VIEW_DEFINITION` omits the view options, so
    /// the definition is completed from `SHOW CREATE VIEW`.
    fn view_metadata_sql(&self, name: &str, schema: Option<&str>) -> Option<String> {
        let name = quote_ident("mysql", name);
        let qualified = schema
            .filter(|schema| !schema.is_empty())
            .map(|schema| format!("{}.{}", quote_ident("mysql", schema), name))
            .unwrap_or(name);
        Some(format!("SHOW CREATE VIEW {qualified}"))
    }

    fn list_privileges_sql(&self) -> Option<String> {
        Some(
            "SELECT GRANTEE AS grantee, '*' AS table_schema, '*' AS name, PRIVILEGE_TYPE AS privilege \
             FROM information_schema.USER_PRIVILEGES \
             UNION ALL \
             SELECT GRANTEE AS grantee, TABLE_SCHEMA AS table_schema, TABLE_NAME AS name, PRIVILEGE_TYPE AS privilege \
             FROM information_schema.TABLE_PRIVILEGES \
             WHERE TABLE_SCHEMA = DATABASE() \
             ORDER BY 1, 2, 3 LIMIT 500"
                .into(),
        )
    }
}
