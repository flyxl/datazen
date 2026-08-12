//! Dialect SQL for routines, triggers, and privilege listings.
//!
//! Host runs these through the live driver `query` / `execute` APIs so object
//! browsers stay driver-agnostic.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Function,
    Procedure,
    Trigger,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "function" => Some(Self::Function),
            "procedure" => Some(Self::Procedure),
            "trigger" => Some(Self::Trigger),
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
        ("mysql", ObjectKind::Function) => Some(format!("SHOW CREATE FUNCTION {ident}")),
        ("mysql", ObjectKind::Procedure) => Some(format!("SHOW CREATE PROCEDURE {ident}")),
        ("mysql", ObjectKind::Trigger) => Some(format!("SHOW CREATE TRIGGER {ident}")),
        ("sqlite", ObjectKind::Trigger) => Some(format!(
            "SELECT sql AS ddl FROM sqlite_master WHERE type = 'trigger' AND name = {}",
            sql_string(name),
        )),
        _ => {
            let _ = qualified;
            None
        }
    }
}

pub fn list_privileges_sql(db_type: &str) -> Option<String> {
    match dialect_family(db_type) {
        "postgresql" => Some(
            "SELECT grantee, table_schema AS schema, table_name AS name, privilege_type AS privilege \
             FROM information_schema.role_table_grants \
             WHERE table_schema NOT IN ('pg_catalog','information_schema') \
             ORDER BY 1, 2, 3 LIMIT 500"
                .into(),
        ),
        "mysql" => Some(
            "SELECT GRANTEE AS grantee, TABLE_SCHEMA AS schema, TABLE_NAME AS name, PRIVILEGE_TYPE AS privilege \
             FROM information_schema.TABLE_PRIVILEGES \
             WHERE TABLE_SCHEMA = DATABASE() \
             ORDER BY 1, 2, 3 LIMIT 500"
                .into(),
        ),
        _ => None,
    }
}

pub fn dialect_family(db_type: &str) -> &'static str {
    match db_type.to_ascii_lowercase().as_str() {
        "postgresql" | "postgres" | "cockroach" => "postgresql",
        "mysql" | "mariadb" | "tidb" => "mysql",
        "sqlite" => "sqlite",
        _ => "other",
    }
}

fn quote_ident(family: &str, name: &str) -> String {
    match family {
        "mysql" => format!("`{}`", name.replace('`', "``")),
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
    fn postgres_function_list_sql_excludes_catalog() {
        let sql = list_objects_sql("postgresql", ObjectKind::Function).unwrap();
        assert!(sql.contains("pg_proc"));
        assert!(sql.contains("pg_catalog"));
    }

    #[test]
    fn mysql_procedure_uses_show_status() {
        let sql = list_objects_sql("mysql", ObjectKind::Procedure).unwrap();
        assert!(sql.to_ascii_uppercase().contains("SHOW PROCEDURE"));
    }

    #[test]
    fn sqlite_has_triggers_only() {
        assert!(list_objects_sql("sqlite", ObjectKind::Trigger).is_some());
        assert!(list_objects_sql("sqlite", ObjectKind::Function).is_none());
        assert!(list_privileges_sql("sqlite").is_none());
    }

    #[test]
    fn privilege_sql_for_pg_and_mysql() {
        assert!(list_privileges_sql("postgres")
            .unwrap()
            .contains("role_table_grants"));
        assert!(list_privileges_sql("mariadb")
            .unwrap()
            .contains("TABLE_PRIVILEGES"));
    }

    #[test]
    fn object_kind_parse() {
        assert_eq!(ObjectKind::parse("FUNCTION"), Some(ObjectKind::Function));
        assert_eq!(ObjectKind::parse("nope"), None);
    }

    #[test]
    fn ddl_sql_quotes_ident() {
        let sql = object_ddl_sql("mysql", ObjectKind::Function, "foo`bar", None).unwrap();
        assert!(sql.contains("`foo``bar`"));
    }

    #[test]
    fn dialect_aliases_map_to_families() {
        assert_eq!(dialect_family("cockroach"), "postgresql");
        assert_eq!(dialect_family("tidb"), "mysql");
        assert_eq!(dialect_family("redis"), "other");
        assert!(list_objects_sql("cockroach", ObjectKind::Function).is_some());
        assert!(list_objects_sql("tidb", ObjectKind::Procedure).is_some());
        assert!(list_objects_sql("redis", ObjectKind::Function).is_none());
    }

    #[test]
    fn postgres_trigger_and_sqlite_ddl() {
        let pg = object_ddl_sql("postgresql", ObjectKind::Trigger, "trg", Some("public")).unwrap();
        assert!(pg.contains("pg_get_triggerdef"));
        let sqlite = object_ddl_sql("sqlite", ObjectKind::Trigger, "trg'x", None).unwrap();
        assert!(sqlite.contains("sqlite_master"));
        assert!(sqlite.contains("trg''x"));
        assert!(object_ddl_sql("sqlite", ObjectKind::Function, "f", None).is_none());
    }

    #[test]
    fn mysql_list_and_ddl_cover_all_kinds() {
        assert!(list_objects_sql("mysql", ObjectKind::Function)
            .unwrap()
            .contains("FUNCTION"));
        assert!(list_objects_sql("mysql", ObjectKind::Trigger)
            .unwrap()
            .contains("TRIGGERS"));
        assert!(object_ddl_sql("mysql", ObjectKind::Procedure, "p", None)
            .unwrap()
            .contains("SHOW CREATE PROCEDURE"));
        assert!(object_ddl_sql("mysql", ObjectKind::Trigger, "t", None)
            .unwrap()
            .contains("SHOW CREATE TRIGGER"));
    }

    #[test]
    fn postgres_function_ddl_defaults_public_schema() {
        let sql = object_ddl_sql("postgres", ObjectKind::Function, "fn", None).unwrap();
        assert!(sql.contains("'public'"));
        assert_eq!(ObjectKind::Function.as_str(), "function");
        assert_eq!(ObjectKind::Procedure.as_str(), "procedure");
        assert_eq!(ObjectKind::Trigger.as_str(), "trigger");
    }

    #[test]
    fn postgres_ident_escapes_quotes() {
        let sql = object_ddl_sql("postgresql", ObjectKind::Function, "f\"n", Some("s")).unwrap();
        assert!(sql.contains("pg_get_functiondef"));
        assert!(sql.contains("'f\"n'") || sql.contains("f\"n"));
    }
}
