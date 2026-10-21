//! Shared vocabulary for the schema-object commands (object kinds, the
//! object/privilege shapes and the SQL literal helpers).
//!
//! The dialect SQL itself lives in the driver crates: each driver publishes
//! it through [`crate::schema_object_sql::SchemaObjectSql`]. The shared
//! `list_objects` / `get_object_ddl` / `list_privileges` commands ask the
//! driver first; a driver without a provider exposes no schema objects.
//!
//! Do not add dialect SQL back here — publish a provider instead.

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
    }

    #[test]
    fn dialect_aliases_map_to_families() {
        assert_eq!(dialect_family("cockroach"), "postgresql");
        assert_eq!(dialect_family("tidb"), "mysql");
        assert_eq!(dialect_family("redis"), "other");
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
        // DuckDB has its own sequence/trigger family.
        assert_eq!(dialect_family("duckdb"), "duckdb");
        assert_eq!(dialect_family("sqlserver"), "sqlserver");
        assert_eq!(dialect_family("mssql"), "sqlserver");
    }

    #[test]
    fn identifiers_and_literals_are_quoted_per_family() {
        // SQL Server uses bracket-quoted identifiers.
        assert_eq!(quote_ident("sqlserver", "my table"), "[my table]");
        assert_eq!(quote_ident("mysql", "my`table"), "`my``table`");
        assert_eq!(sql_string("it's"), "'it''s'");
    }
}
