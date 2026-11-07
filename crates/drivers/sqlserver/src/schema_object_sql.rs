//! SqlServerSchemaObjectSql dialect SQL for the shared schema-object commands.
//!
//! The `list_objects` / `get_object_ddl` / `list_privileges` commands are
//! implemented once in the driver API; the SQL is dialect knowledge and
//! therefore lives here, next to the driver that runs it.

use datazen_driver_api::schema_object_sql::{ObjectDdlRequest, SchemaObjectSql};
use datazen_driver_api::schema_objects::{quote_ident, sql_string, ObjectKind};

/// SqlServerSchemaObjectSql's dialect SQL provider.
pub struct SqlServerSchemaObjectSql;

impl SchemaObjectSql for SqlServerSchemaObjectSql {
    fn list_objects_sql(&self, kind: ObjectKind) -> Option<String> {
        match kind {
            ObjectKind::View => Some(
                "SELECT s.name AS [schema], v.name AS name \
             FROM sys.views v JOIN sys.schemas s ON s.schema_id = v.schema_id \
             WHERE v.is_ms_shipped = 0 \
             ORDER BY 1, 2"
                    .into(),
            ),
            ObjectKind::Function => Some(
                "SELECT s.name AS [schema], o.name AS name \
             FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE o.type IN ('FN','FS','FT','IF','TF') \
             ORDER BY 1, 2"
                    .into(),
            ),
            ObjectKind::Procedure => Some(
                "SELECT s.name AS [schema], o.name AS name \
             FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE o.type IN ('P','PC') \
             ORDER BY 1, 2"
                    .into(),
            ),
            ObjectKind::Trigger => Some(
                // `sys.triggers` has no `schema_id` column (error 207); the owning
                // schema is reached through `sys.objects`.
                "SELECT s.name AS [schema], t.name AS name \
             FROM sys.triggers t \
             JOIN sys.objects o ON o.object_id = t.object_id \
             JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE t.is_ms_shipped = 0 \
             ORDER BY 1, 2"
                    .into(),
            ),
            ObjectKind::Sequence => Some(
                "SELECT s.name AS [schema], q.name AS name \
             FROM sys.sequences q \
             JOIN sys.schemas s ON s.schema_id = q.schema_id \
             ORDER BY 1, 2"
                    .into(),
            ),
            ObjectKind::Type => Some(
                "SELECT s.name AS [schema], t.name AS name \
             FROM sys.types t \
             JOIN sys.schemas s ON s.schema_id = t.schema_id \
             WHERE t.is_user_defined = 1 \
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
        let ident = quote_ident("sqlserver", name);
        let schema_ident = schema
            .filter(|s| !s.is_empty())
            .map(|s| quote_ident("sqlserver", s));
        let qualified = match &schema_ident {
            Some(s) => format!("{s}.{ident}"),
            None => ident.clone(),
        };
        // Some families qualify the name in their DDL, others build their
        // own quoted target; the value stays available for the former.
        let _ = &qualified;
        match kind {
            ObjectKind::View
            | ObjectKind::Function
            | ObjectKind::Procedure
            | ObjectKind::Trigger => {
                let schema_str = schema.filter(|s| !s.is_empty()).unwrap_or("dbo");
                let target_ident = format!(
                    "{}.{}",
                    quote_ident("sqlserver", schema_str),
                    quote_ident("sqlserver", name)
                );
                let target_literal = sql_string(&target_ident);
                Some(format!(
                    "SELECT OBJECT_DEFINITION(OBJECT_ID({target_literal})) AS ddl"
                ))
            }
            ObjectKind::Sequence => {
                let schema_str = schema.filter(|s| !s.is_empty()).unwrap_or("dbo");
                let quoted_schema = quote_ident("sqlserver", schema_str);
                let quoted_name = quote_ident("sqlserver", name);
                let name_literal = sql_string(name);
                Some(format!(
                    "SELECT 'CREATE SEQUENCE {quoted_schema}.{quoted_name} AS [' + ty.name + '] ' \
                 + 'START WITH ' + CAST(q.start_value AS varchar) \
                 + ' INCREMENT BY ' + CAST(q.increment AS varchar) \
                 + CASE WHEN q.is_cycling = 1 THEN ' CYCLE' ELSE ' NO CYCLE' END \
                 AS ddl \
                 FROM sys.sequences q \
                 JOIN sys.types ty ON ty.user_type_id = q.user_type_id \
                 WHERE q.name = {name_literal}"
                ))
            }
            ObjectKind::Type => {
                let schema_str = schema.filter(|s| !s.is_empty()).unwrap_or("dbo");
                let quoted_schema = quote_ident("sqlserver", schema_str);
                let quoted_name = quote_ident("sqlserver", name);
                let name_literal = sql_string(name);
                let schema_literal = sql_string(schema_str);
                Some(format!(
                    "SELECT 'CREATE TYPE {quoted_schema}.{quoted_name} FROM [' + ty.name + '](' \
                 + CASE WHEN ty.max_length > 0 THEN CAST(ty.max_length AS varchar) \
                        ELSE CAST(t.precision AS varchar) + ',' + CAST(t.scale AS varchar) END \
                 + ')' AS ddl \
                 FROM sys.types t \
                 JOIN sys.types ty ON ty.user_type_id = t.system_type_id \
                 WHERE t.is_user_defined = 1 AND t.schema_id = SCHEMA_ID({schema_literal}) \
                   AND t.name = {name_literal}"
                ))
            }
            _ => None,
        }
    }

    fn list_privileges_sql(&self) -> Option<String> {
        Some(
            "SELECT pr.name AS grantee, \
                    CASE WHEN p.class = 0 THEN '<server>' ELSE OBJECT_SCHEMA_NAME(p.major_id) END AS [schema], \
                    CASE WHEN p.class = 0 THEN '*' ELSE OBJECT_NAME(p.major_id) END AS name, \
                    p.permission_name AS privilege \
             FROM sys.database_permissions p \
             JOIN sys.database_principals pr ON p.grantee_principal_id = pr.principal_id \
             WHERE pr.type IN ('S','U','G','R') \
             ORDER BY 1, 2, 3 \
             OFFSET 0 ROWS FETCH NEXT 500 ROWS ONLY"
                .into(),
        )
    }
}
