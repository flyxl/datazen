//! Driver-owned dialect SQL for the shared schema-object commands.
//!
//! `list_objects` / `get_object_ddl` / `list_privileges` are implemented
//! once, in the driver API, so every SQL dialect answers the same command
//! contract. The SQL itself is dialect knowledge: it belongs to the driver.
//! A driver publishes its dialect through [`SchemaObjectSql`]; a dialect that
//! has not migrated yet keeps using the legacy shared tables in
//! [`crate::schema_objects`], but new dialects must not add branches there.

use std::sync::Arc;

use crate::schema_objects::ObjectKind;

/// One object DDL request, as the shared command receives it.
#[derive(Debug, Clone, Copy)]
pub struct ObjectDdlRequest<'a> {
    pub kind: ObjectKind,
    pub name: &'a str,
    /// Schema the object lives in (already resolved for the dialect).
    pub schema: Option<&'a str>,
    /// PostgreSQL routine identity arguments (overload disambiguation).
    pub signature: Option<&'a str>,
    /// Relation a trigger is attached to.
    pub target_schema: Option<&'a str>,
    pub target_name: Option<&'a str>,
}

/// Dialect SQL for the schema-object commands.
pub trait SchemaObjectSql: Send + Sync {
    /// List-query SQL for one object kind. `None` when the dialect has no
    /// such objects.
    fn list_objects_sql(&self, kind: ObjectKind) -> Option<String>;

    /// List-query SQL for one object kind in a specific database.
    ///
    /// The default replaces the `DATABASE()` placeholder for dialects that
    /// expose all catalogs through `information_schema`; a dialect that
    /// selects its database differently can override this.
    fn list_objects_sql_for_database(
        &self,
        kind: ObjectKind,
        database: Option<&str>,
    ) -> Option<String> {
        let sql = self.list_objects_sql(kind)?;
        let Some(database) = database.filter(|value| !value.trim().is_empty()) else {
            return Some(sql);
        };
        Some(sql.replace("DATABASE()", &crate::schema_objects::sql_string(database)))
    }

    /// DDL query SQL for one object. `None` when the dialect does not expose
    /// DDL for that object kind.
    fn object_ddl_sql(&self, request: &ObjectDdlRequest<'_>) -> Option<String>;

    /// Privilege listing SQL. `None` when the dialect has no privileges.
    fn list_privileges_sql(&self) -> Option<String>;

    /// Schema used to qualify the DDL lookup.
    ///
    /// MySQL's `*_SCHEMA` columns identify databases, so its DDL lookup
    /// resolves the catalog instead of a namespace schema.
    fn ddl_schema_scope<'a>(
        &self,
        schema: Option<&'a str>,
        database: Option<&'a str>,
    ) -> Option<&'a str> {
        let _ = database;
        schema
    }

    /// Extra query whose result enriches the DDL response (MySQL views need
    /// `SHOW CREATE VIEW` to recover the body).
    fn view_metadata_sql(&self, name: &str, schema: Option<&str>) -> Option<String> {
        let _ = (name, schema);
        None
    }
}

/// Shared handle type for driver-provided dialect SQL.
pub type SharedSchemaObjectSql = Arc<dyn SchemaObjectSql>;
