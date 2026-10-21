//! Legacy shared dialect SQL for views, routines, triggers, and privileges.
//!
//! These tables are the **fallback** for dialects that have not published a
//! [`crate::schema_object_sql::SchemaObjectSql`] provider: the shared
//! `list_objects` / `get_object_ddl` / `list_privileges` commands use them
//! when the driver has no provider of its own.
//!
//! New dialect SQL belongs in the driver crate. PostgreSQL already moved
//! out (see `packages/drivers/postgres/src/schema_object_sql.rs`); do not
//! add new `match db_type` branches here — publish a provider instead.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectKind {
    Table,
    View,
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::Type => "type",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "table" => Some(Self::Table),
            "view" => Some(Self::View),
            "function" => Some(Self::Function),
            "procedure" => Some(Self::Procedure),
            "trigger" => Some(Self::Trigger),
            "sequence" => Some(Self::Sequence),
            "type" => Some(Self::Type),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseObject {
    pub kind: String,
    pub schema: Option<String>,
    pub name: String,
    /// PostgreSQL routine identity arguments. This disambiguates overloaded
    /// functions/procedures without putting SQL syntax into `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Schema/name of the relation a trigger is attached to. A trigger name
    /// is only unique within its relation on some supported engines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegeGrant {
    pub grantee: String,
    pub object_schema: Option<String>,
    pub object_name: String,
    pub privilege: String,
}

/// List-query SQL for routines/triggers. `None` when the dialect has no objects.
pub fn list_objects_sql(db_type: &str, kind: ObjectKind) -> Option<String> {
    let family = dialect_family(db_type);
    match (family, kind) {
        ("mysql", ObjectKind::Function) => Some(
            "SELECT NULL AS `schema`, ROUTINE_NAME AS name \
             FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = DATABASE() AND ROUTINE_TYPE = 'FUNCTION' \
             ORDER BY 1, 2"
                .into(),
        ),
        ("mysql", ObjectKind::Procedure) => Some(
            "SELECT NULL AS `schema`, ROUTINE_NAME AS name \
             FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = DATABASE() AND ROUTINE_TYPE = 'PROCEDURE' \
             ORDER BY 1, 2"
                .into(),
        ),
        ("mysql", ObjectKind::Trigger) => Some(
            "SELECT NULL AS `schema`, TRIGGER_NAME AS name, \
                    NULL AS target_schema, EVENT_OBJECT_TABLE AS target_name \
             FROM information_schema.TRIGGERS \
             WHERE TRIGGER_SCHEMA = DATABASE() \
             ORDER BY 1, 2, 4"
                .into(),
        ),
        ("mysql", ObjectKind::View) => Some(
            "SELECT NULL AS `schema`, TABLE_NAME AS name \
             FROM information_schema.VIEWS WHERE TABLE_SCHEMA = DATABASE() \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlite", ObjectKind::View) => Some(
            "SELECT NULL AS schema, name FROM sqlite_master \
             WHERE type = 'view' ORDER BY name"
                .into(),
        ),
        ("sqlite", ObjectKind::Trigger) => Some(
            "SELECT NULL AS schema, name, NULL AS target_schema, tbl_name AS target_name \
                 FROM sqlite_master WHERE type = 'trigger' ORDER BY name"
                .into(),
        ),
        ("duckdb", ObjectKind::Trigger) => Some(
            "SELECT NULL AS schema, trigger_name AS name \
             FROM information_schema.triggers \
             ORDER BY 1, 2"
                .into(),
        ),
        ("duckdb", ObjectKind::Sequence) => Some(
            "SELECT sequence_schema AS schema, sequence_name AS name \
             FROM information_schema.sequences \
             WHERE sequence_schema NOT IN ('information_schema') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::View) => Some(
            "SELECT s.name AS [schema], v.name AS name \
             FROM sys.views v JOIN sys.schemas s ON s.schema_id = v.schema_id \
             WHERE v.is_ms_shipped = 0 \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Function) => Some(
            "SELECT s.name AS [schema], o.name AS name \
             FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE o.type IN ('FN','FS','FT','IF','TF') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Procedure) => Some(
            "SELECT s.name AS [schema], o.name AS name \
             FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE o.type IN ('P','PC') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Trigger) => Some(
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
        ("sqlserver", ObjectKind::Sequence) => Some(
            "SELECT s.name AS [schema], q.name AS name \
             FROM sys.sequences q \
             JOIN sys.schemas s ON s.schema_id = q.schema_id \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Type) => Some(
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

/// MySQL's *_SCHEMA columns identify databases, not a separate namespace
/// schema. Its object-list SELECTs therefore expose null schema/target_schema;
/// the caller keeps database ownership in the independent database target.
/// Build an object-list query for a specific database when the dialect needs
/// an explicit catalog filter. PostgreSQL chooses the database through
/// `query_at`; MySQL exposes all catalogs through `information_schema`, so its
/// routine/trigger predicates must name the selected database instead of
/// relying on the connection's default `DATABASE()` value.
pub fn list_objects_sql_for_database(
    db_type: &str,
    kind: ObjectKind,
    database: Option<&str>,
) -> Option<String> {
    let sql = list_objects_sql(db_type, kind)?;
    let Some(database) = database.filter(|value| !value.trim().is_empty()) else {
        return Some(sql);
    };
    if dialect_family(db_type) != "mysql" {
        return Some(sql);
    }

    Some(sql.replace("DATABASE()", &sql_string(database)))
}

pub fn object_ddl_sql(
    db_type: &str,
    kind: ObjectKind,
    name: &str,
    schema: Option<&str>,
) -> Option<String> {
    object_ddl_sql_with_metadata(db_type, kind, name, schema)
}

/// Build a metadata lookup for an object using its full identity.
///
/// Values are encoded as SQL string literals; identifiers used in DDL
/// statements are quoted with the target dialect's rules.
///
/// Dialects that need the routine identity arguments or the trigger's owning
/// relation answer from their own provider (see
/// [`crate::schema_object_sql::SchemaObjectSql`]) — the PostgreSQL dialect
/// does exactly that.
pub fn object_ddl_sql_with_metadata(
    db_type: &str,
    kind: ObjectKind,
    name: &str,
    schema: Option<&str>,
) -> Option<String> {
    let family = dialect_family(db_type);
    let ident = quote_ident(family, name);
    let schema_ident = schema
        .filter(|s| !s.is_empty())
        .map(|s| quote_ident(family, s));
    let qualified = match &schema_ident {
        Some(s) => format!("{s}.{ident}"),
        None => ident.clone(),
    };
    match (family, kind) {
        ("mysql", ObjectKind::Table) => Some(format!("SHOW CREATE TABLE {qualified}")),
        ("mysql", ObjectKind::View) => Some(format!(
            "SELECT TABLE_SCHEMA AS view_schema, VIEW_DEFINITION AS ddl, DEFINER AS view_definer, \
             SECURITY_TYPE AS view_security_type, CHECK_OPTION AS view_check_option, \
             CHARACTER_SET_CLIENT AS view_character_set_client, \
             COLLATION_CONNECTION AS view_collation_connection \
             FROM information_schema.VIEWS \
             WHERE TABLE_SCHEMA = COALESCE(NULLIF({}, ''), DATABASE()) AND TABLE_NAME = {}",
            sql_string(schema.filter(|s| !s.is_empty()).unwrap_or("")),
            sql_string(name),
        )),
        ("mysql", ObjectKind::Function) => Some(format!(
            "SHOW CREATE FUNCTION {qualified}"
        )),
        ("mysql", ObjectKind::Procedure) => Some(format!(
            "SHOW CREATE PROCEDURE {qualified}"
        )),
        ("mysql", ObjectKind::Trigger) => Some(format!("SHOW CREATE TRIGGER {qualified}")),
        ("sqlite", ObjectKind::Table) => Some(format!(
            "SELECT sql AS ddl FROM sqlite_master WHERE type = 'table' AND name = {}",
            sql_string(name),
        )),
        ("sqlite", ObjectKind::View) => Some(format!(
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
        ("sqlite", ObjectKind::Trigger) => Some(format!(
            "SELECT sql AS ddl FROM sqlite_master WHERE type = 'trigger' AND name = {}",
            sql_string(name),
        )),
        ("duckdb", ObjectKind::Trigger) => Some(format!(
            "SELECT sql AS ddl FROM information_schema.triggers WHERE trigger_name = {} LIMIT 1",
            sql_string(name),
        )),
        ("duckdb", ObjectKind::Sequence) => Some(format!(
            "SELECT sql AS ddl FROM duckdb_sequences() WHERE sequence_name = {} LIMIT 1",
            sql_string(name),
        )),
        (
            "sqlserver",
            ObjectKind::View | ObjectKind::Function | ObjectKind::Procedure | ObjectKind::Trigger,
        ) => {
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
        ("sqlserver", ObjectKind::Sequence) => {
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
        ("sqlserver", ObjectKind::Type) => {
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
        _ => {
            let _ = qualified;
            None
        }
    }
}

/// Build a MySQL `SHOW CREATE VIEW` query used to capture the definition
/// options omitted by `information_schema.VIEWS.VIEW_DEFINITION`.
pub fn mysql_show_create_view_sql(name: &str, schema: Option<&str>) -> String {
    let name = quote_ident("mysql", name);
    let qualified = schema
        .filter(|schema| !schema.is_empty())
        .map(|schema| format!("{}.{}", quote_ident("mysql", schema), name))
        .unwrap_or(name);
    format!("SHOW CREATE VIEW {qualified}")
}

pub fn list_privileges_sql(db_type: &str) -> Option<String> {
    match dialect_family(db_type) {
        "mysql" => Some(
            "SELECT GRANTEE AS grantee, '*' AS table_schema, '*' AS name, PRIVILEGE_TYPE AS privilege \
             FROM information_schema.USER_PRIVILEGES \
             UNION ALL \
             SELECT GRANTEE AS grantee, TABLE_SCHEMA AS table_schema, TABLE_NAME AS name, PRIVILEGE_TYPE AS privilege \
             FROM information_schema.TABLE_PRIVILEGES \
             WHERE TABLE_SCHEMA = DATABASE() \
             ORDER BY 1, 2, 3 LIMIT 500"
                .into(),
        ),
        "sqlserver" => Some(
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
        ),
        _ => None,
    }
}

pub fn dialect_family(db_type: &str) -> &'static str {
    match db_type.to_ascii_lowercase().as_str() {
        "postgresql" | "postgres" | "cockroach" | "cloudberry" | "questdb" => "postgresql",
        "mysql" | "mariadb" | "tidb" | "doris" | "starrocks" | "manticore" | "ob_oracle" => "mysql",
        "sqlite" | "rqlite" | "turso" => "sqlite",
        "duckdb" => "duckdb",
        "sqlserver" | "mssql" => "sqlserver",
        _ => "other",
    }
}

/// Quote an identifier the way the dialect family does.
pub fn quote_ident(family: &str, name: &str) -> String {
    match family {
        "mysql" => format!("`{}`", name.replace('`', "``")),
        "sqlserver" => format!("[{}]", name.replace(']', "]]")),
        _ => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

/// SQL string literal for a value (single quotes doubled).
pub fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_kind_parse() {
        assert_eq!(ObjectKind::parse("FUNCTION"), Some(ObjectKind::Function));
        assert_eq!(ObjectKind::parse("nope"), None);
    }

    #[test]
    fn object_kind_parses_table_and_view() {
        assert_eq!(ObjectKind::parse("table"), Some(ObjectKind::Table));
        assert_eq!(ObjectKind::parse("view"), Some(ObjectKind::View));
        assert_eq!(ObjectKind::Table.as_str(), "table");
        assert_eq!(ObjectKind::View.as_str(), "view");

        assert!(object_ddl_sql("mysql", ObjectKind::Table, "users", None).is_some());
        // The PostgreSQL family's DDL SQL lives in the PostgreSQL driver crate.
        assert!(
            object_ddl_sql("mysql", ObjectKind::View, "active_users", Some("public")).is_some()
        );
        assert!(object_ddl_sql("sqlite", ObjectKind::Table, "users", None).is_some());
    }

    #[test]
    fn dialect_aliases_map_to_families() {
        assert_eq!(dialect_family("cockroach"), "postgresql");
        assert_eq!(dialect_family("tidb"), "mysql");
        assert_eq!(dialect_family("redis"), "other");
        // The PostgreSQL family's SQL lives in the PostgreSQL driver crate, so
        // the shared tables only answer for the families that have not
        // published a provider yet.
        assert!(list_objects_sql("tidb", ObjectKind::Procedure).is_some());
        assert!(list_objects_sql("redis", ObjectKind::Function).is_none());
        // PG/MySQL reuse engines map to their protocol family.
        assert_eq!(dialect_family("cloudberry"), "postgresql");
        assert_eq!(dialect_family("questdb"), "postgresql");
        assert_eq!(dialect_family("doris"), "mysql");
        assert_eq!(dialect_family("starrocks"), "mysql");
        assert_eq!(dialect_family("manticore"), "mysql");
        assert_eq!(dialect_family("ob_oracle"), "mysql");
        // SQLite-compatible drivers map to the sqlite family.
        assert_eq!(dialect_family("rqlite"), "sqlite");
        assert_eq!(dialect_family("turso"), "sqlite");
        assert!(list_objects_sql("rqlite", ObjectKind::Trigger).is_some());
        assert!(list_objects_sql("turso", ObjectKind::Trigger).is_some());
        // DuckDB has its own sequence/trigger family.
        assert_eq!(dialect_family("duckdb"), "duckdb");
        assert!(list_objects_sql("duckdb", ObjectKind::Trigger).is_some());
        assert!(list_objects_sql("duckdb", ObjectKind::Sequence).is_some());
        assert!(object_ddl_sql("duckdb", ObjectKind::Sequence, "seq", None).is_some());
    }

    #[test]
    fn mysql_object_catalog_can_target_a_database_without_using_connection_default() {
        let sql =
            list_objects_sql_for_database("mysql", ObjectKind::Procedure, Some("manual'catalog"))
                .unwrap();
        assert!(sql.contains("ROUTINE_SCHEMA = 'manual''catalog'"));
        assert!(!sql.contains("DATABASE()"));

        let default_sql =
            list_objects_sql_for_database("mysql", ObjectKind::Function, None).unwrap();
        assert!(default_sql.contains("ROUTINE_SCHEMA = DATABASE()"));
    }

    #[test]
    fn trigger_catalog_includes_attached_relation() {
        let list = list_objects_sql("mysql", ObjectKind::Trigger).unwrap();
        assert!(list.contains("information_schema.TRIGGERS"));
        assert!(list.contains("target_name"));
    }

    #[test]
    fn unsupported_sequence_families_fail_closed() {
        assert!(list_objects_sql("mysql", ObjectKind::Sequence).is_none());
        assert!(list_objects_sql("sqlite", ObjectKind::Sequence).is_none());
        assert!(object_ddl_sql("mysql", ObjectKind::Sequence, "seq", None).is_none());
        assert!(object_ddl_sql("sqlite", ObjectKind::Sequence, "seq", None).is_none());
    }

    #[test]
    fn sqlserver_objects_cover_all_kinds() {
        assert_eq!(dialect_family("sqlserver"), "sqlserver");
        assert_eq!(dialect_family("mssql"), "sqlserver");
        for kind in [
            ObjectKind::View,
            ObjectKind::Function,
            ObjectKind::Procedure,
            ObjectKind::Trigger,
            ObjectKind::Sequence,
            ObjectKind::Type,
        ] {
            let sql = list_objects_sql("sqlserver", kind)
                .unwrap_or_else(|| panic!("sqlserver should list object kind {kind:?}"));
            let catalog = match kind {
                ObjectKind::View => "sys.views",
                ObjectKind::Sequence => "sys.sequences",
                ObjectKind::Type => "sys.types",
                _ => "sys.objects",
            };
            assert!(
                sql.contains(catalog)
                    || (kind == ObjectKind::Trigger && sql.contains("sys.triggers")),
                "sqlserver list should read catalog views for {kind:?}: {sql}"
            );
            // `schema` is a reserved keyword in T-SQL: an unquoted alias is a
            // syntax error (156), so these queries must bracket-quote it.
            assert!(
                !sql.contains("AS schema"),
                "sqlserver list must not alias a column `AS schema` (reserved word) for {kind:?}: {sql}"
            );
        }
        let fn_ddl = object_ddl_sql("sqlserver", ObjectKind::Function, "fn", Some("dbo")).unwrap();
        assert!(fn_ddl.contains("OBJECT_DEFINITION"));
        assert!(fn_ddl.contains("[dbo].[fn]"));
        let view_ddl = object_ddl_sql("sqlserver", ObjectKind::View, "v", Some("dbo")).unwrap();
        assert!(view_ddl.contains("OBJECT_DEFINITION"));
        assert!(view_ddl.contains("[dbo].[v]"));
        // Without schema it falls back to dbo.
        let no_schema = object_ddl_sql("sqlserver", ObjectKind::Procedure, "p", None).unwrap();
        assert!(no_schema.contains("[dbo].[p]"));
        // Sequences/types are reconstructed from catalog metadata.
        let seq_ddl =
            object_ddl_sql("sqlserver", ObjectKind::Sequence, "seq", Some("dbo")).unwrap();
        assert!(seq_ddl.contains("sys.sequences"));
        assert!(seq_ddl.contains("CREATE SEQUENCE"));
        let type_ddl = object_ddl_sql("sqlserver", ObjectKind::Type, "phone", Some("dbo")).unwrap();
        assert!(type_ddl.contains("sys.types"));
        assert!(type_ddl.contains("CREATE TYPE"));
        assert!(list_privileges_sql("sqlserver").is_some());
        assert!(list_privileges_sql("mssql").is_some());
        // SQL Server uses bracket-quoted identifiers.
        assert_eq!(quote_ident("sqlserver", "my table"), "[my table]");
    }

    #[test]
    fn sqlserver_object_ddl_escapes_single_quotes_and_brackets() {
        let ddl_sql = object_ddl_sql(
            "sqlserver",
            ObjectKind::Function,
            "fn'special",
            Some("custom schema"),
        )
        .unwrap();
        assert!(ddl_sql.contains("[custom schema].[fn''special]"));
        assert!(ddl_sql.contains("fn''special"));

        let seq_sql =
            object_ddl_sql("sqlserver", ObjectKind::Sequence, "seq'one", Some("dbo")).unwrap();
        assert!(seq_sql.contains("WHERE q.name = 'seq''one'"));
        assert!(seq_sql.contains("[dbo].[seq'one]"));
    }
}
