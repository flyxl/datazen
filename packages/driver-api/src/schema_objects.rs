//! Dialect SQL for routines, triggers, and privilege listings.
//!
//! Dialect SQL helpers used by driver `list_objects` / `get_object_ddl` /
//! `list_privileges` commands. Host must not execute these SQL strings directly.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::Type => "type",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
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
        ("postgresql", ObjectKind::Function) => Some(
            "SELECT n.nspname AS schema, p.proname AS name \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
               AND p.prokind = 'f' \
             ORDER BY 1, 2"
                .into(),
        ),
        ("postgresql", ObjectKind::Procedure) => Some(
            "SELECT n.nspname AS schema, p.proname AS name \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
               AND p.prokind = 'p' \
             ORDER BY 1, 2"
                .into(),
        ),
        ("postgresql", ObjectKind::Trigger) => Some(
            "SELECT event_object_schema AS schema, trigger_name AS name \
             FROM information_schema.triggers \
             ORDER BY 1, 2"
                .into(),
        ),
        ("postgresql", ObjectKind::Sequence) => Some(
            "SELECT schemaname AS schema, sequencename AS name \
             FROM pg_sequences \
             WHERE schemaname NOT IN ('pg_catalog','information_schema') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("postgresql", ObjectKind::Type) => Some(
            "SELECT n.nspname AS schema, t.typname AS name \
             FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
               AND t.typtype IN ('c','e','d','r') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("mysql", ObjectKind::Function) => {
            Some("SHOW FUNCTION STATUS WHERE Db = DATABASE()".into())
        }
        ("mysql", ObjectKind::Procedure) => {
            Some("SHOW PROCEDURE STATUS WHERE Db = DATABASE()".into())
        }
        ("mysql", ObjectKind::Trigger) => Some("SHOW TRIGGERS".into()),
        ("sqlite", ObjectKind::Trigger) => {
            Some("SELECT name FROM sqlite_master WHERE type = 'trigger' ORDER BY name".into())
        }
        ("sqlserver", ObjectKind::Function) => Some(
            "SELECT s.name AS schema, o.name AS name \
             FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE o.type IN ('FN','FS','FT','IF','TF') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Procedure) => Some(
            "SELECT s.name AS schema, o.name AS name \
             FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE o.type IN ('P','PC') \
             ORDER BY 1, 2"
                .into(),
        ),
        ("sqlserver", ObjectKind::Trigger) => Some(
            "SELECT s.name AS schema, t.name AS name \
             FROM sys.triggers t \
             JOIN sys.schemas s ON s.schema_id = t.schema_id \
             WHERE t.is_ms_shipped = 0 \
             ORDER BY 1, 2"
                .into(),
        ),
        _ => None,
    }
}

pub fn object_ddl_sql(
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
        ("postgresql", ObjectKind::Function | ObjectKind::Procedure) => Some(format!(
            "SELECT pg_get_functiondef(p.oid) AS ddl \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE p.proname = {} AND n.nspname = {}",
            sql_string(name),
            sql_string(schema.unwrap_or("public")),
        )),
        ("postgresql", ObjectKind::Trigger) => Some(format!(
            "SELECT pg_get_triggerdef(t.oid) AS ddl \
             FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE t.tgname = {} AND n.nspname = {}",
            sql_string(name),
            sql_string(schema.unwrap_or("public")),
        )),
        ("postgresql", ObjectKind::Sequence) => {
            let schema_str = sql_string(schema.unwrap_or("public"));
            let name_str = sql_string(name);
            Some(format!(
                "SELECT 'CREATE SEQUENCE ' || quote_ident(schemaname) || '.' || quote_ident(sequencename) \
                 || ' AS ' || data_type \
                 || ' INCREMENT BY ' || increment_by \
                 || ' MINVALUE ' || min_value \
                 || ' MAXVALUE ' || max_value \
                 || ' START WITH ' || start_value \
                 || CASE WHEN cycle THEN ' CYCLE' ELSE ' NO CYCLE' END \
                 || ';' AS ddl \
                 FROM pg_sequences WHERE schemaname = {schema_str} AND sequencename = {name_str}"
            ))
        }
        ("postgresql", ObjectKind::Type) => {
            let schema_str = sql_string(schema.unwrap_or("public"));
            let name_str = sql_string(name);
            Some(format!(
                "SELECT pg_catalog.format_type(t.oid, NULL) || ' = ' || \
                 CASE t.typtype \
                   WHEN 'e' THEN 'ENUM (' || string_agg(quote_literal(e.enumlabel), ', ' ORDER BY e.enumsortorder) || ')' \
                   WHEN 'c' THEN 'COMPOSITE (...)' \
                   WHEN 'd' THEN 'DOMAIN ' || pg_catalog.format_type(t.typbasetype, t.typtypmod) \
                   WHEN 'r' THEN 'RANGE' \
                   ELSE t.typtype::text \
                 END AS ddl \
                 FROM pg_type t \
                 JOIN pg_namespace n ON n.oid = t.typnamespace \
                 LEFT JOIN pg_enum e ON e.enumtypid = t.oid \
                 WHERE n.nspname = {schema_str} AND t.typname = {name_str} \
                 GROUP BY t.oid, t.typtype, t.typbasetype, t.typtypmod"
            ))
        }
        ("mysql", ObjectKind::Function) => Some(format!("SHOW CREATE FUNCTION {ident}")),
        ("mysql", ObjectKind::Procedure) => Some(format!("SHOW CREATE PROCEDURE {ident}")),
        ("mysql", ObjectKind::Trigger) => Some(format!("SHOW CREATE TRIGGER {ident}")),
        ("sqlite", ObjectKind::Trigger) => Some(format!(
            "SELECT sql AS ddl FROM sqlite_master WHERE type = 'trigger' AND name = {}",
            sql_string(name),
        )),
        ("sqlserver", ObjectKind::Function | ObjectKind::Procedure | ObjectKind::Trigger) => {
            let schema_str = schema.filter(|s| !s.is_empty()).unwrap_or("dbo");
            Some(format!(
                "SELECT OBJECT_DEFINITION(OBJECT_ID('{schema_str}.{name}')) AS ddl"
            ))
        }
        _ => {
            let _ = qualified;
            None
        }
    }
}

pub fn list_privileges_sql(db_type: &str) -> Option<String> {
    match dialect_family(db_type) {
        "postgresql" => Some(
            "SELECT rolname AS grantee, '*' AS schema, '*' AS name, \
               CASE WHEN rolsuper THEN 'SUPERUSER' \
                    WHEN rolcreatedb THEN 'CREATEDB' \
                    WHEN rolcreaterole THEN 'CREATEROLE' \
                    ELSE 'LOGIN' END AS privilege \
             FROM pg_roles WHERE rolname NOT LIKE 'pg_%' AND rolcanlogin \
             UNION ALL \
             SELECT grantee, table_schema AS schema, table_name AS name, privilege_type AS privilege \
             FROM information_schema.role_table_grants \
             WHERE table_schema NOT IN ('pg_catalog','information_schema') \
             ORDER BY 1, 2, 3 LIMIT 500"
                .into(),
        ),
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
                    CASE WHEN p.class = 0 THEN '<server>' ELSE OBJECT_SCHEMA_NAME(p.major_id) END AS schema, \
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
        "sqlite" => "sqlite",
        "sqlserver" | "mssql" => "sqlserver",
        _ => "other",
    }
}

fn quote_ident(family: &str, name: &str) -> String {
    match family {
        "mysql" => format!("`{}`", name.replace('`', "``")),
        "sqlserver" => format!("[{}]", name.replace(']', "]]")),
        _ => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

fn sql_string(value: &str) -> String {
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
    fn dialect_aliases_map_to_families() {
        assert_eq!(dialect_family("cockroach"), "postgresql");
        assert_eq!(dialect_family("tidb"), "mysql");
        assert_eq!(dialect_family("redis"), "other");
        assert!(list_objects_sql("cockroach", ObjectKind::Function).is_some());
        assert!(list_objects_sql("tidb", ObjectKind::Procedure).is_some());
        assert!(list_objects_sql("redis", ObjectKind::Function).is_none());
        // PG/MySQL reuse engines map to their protocol family.
        assert_eq!(dialect_family("cloudberry"), "postgresql");
        assert_eq!(dialect_family("questdb"), "postgresql");
        assert_eq!(dialect_family("doris"), "mysql");
        assert_eq!(dialect_family("starrocks"), "mysql");
        assert_eq!(dialect_family("manticore"), "mysql");
        assert_eq!(dialect_family("ob_oracle"), "mysql");
    }

    #[test]
    fn sqlserver_objects_cover_all_kinds() {
        assert_eq!(dialect_family("sqlserver"), "sqlserver");
        assert_eq!(dialect_family("mssql"), "sqlserver");
        for kind in [
            ObjectKind::Function,
            ObjectKind::Procedure,
            ObjectKind::Trigger,
        ] {
            let sql = list_objects_sql("sqlserver", kind)
                .unwrap_or_else(|| panic!("sqlserver should list object kind {kind:?}"));
            assert!(
                sql.contains("sys.objects") || sql.contains("sys.triggers"),
                "sqlserver list should read catalog views: {sql}"
            );
        }
        let fn_ddl = object_ddl_sql("sqlserver", ObjectKind::Function, "fn", Some("dbo")).unwrap();
        assert!(fn_ddl.contains("OBJECT_DEFINITION"));
        assert!(fn_ddl.contains("dbo.fn"));
        // Without schema it falls back to dbo.
        let no_schema = object_ddl_sql("sqlserver", ObjectKind::Procedure, "p", None).unwrap();
        assert!(no_schema.contains("dbo.p"));
        assert!(list_privileges_sql("sqlserver").is_some());
        assert!(list_privileges_sql("mssql").is_some());
        // SQL Server uses bracket-quoted identifiers.
        assert_eq!(quote_ident("sqlserver", "my table"), "[my table]");
    }
}
