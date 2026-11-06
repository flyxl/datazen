//! Static SQL target scope for MCP `query` / `execute` calls.
//!
//! Scope is a security boundary, not a hint: a database/namespace allowlist
//! must restrict every relation a statement can read or write, including
//! relations reached through CTEs, subqueries, joins, unions, and DML. The
//! parser is used only when a scope is active; an unparseable statement fails
//! closed because the host cannot prove where it points.
//!
//! Resolution follows the engine family:
//! - MySQL/ClickHouse/SQLite: two-part `db.table` is a database qualifier.
//! - PostgreSQL/DuckDB/SQL Server: two-part `schema.table` is a namespace
//!   qualifier; three-part names are `database.schema.table`.
//! - A one-part reference resolves through the actual connection defaults.

use std::collections::{BTreeSet, HashSet};
use std::ops::ControlFlow;

use sqlparser::ast::{ObjectName, ObjectNamePart, ObjectType, Statement, Visit, Visitor};
use sqlparser::dialect::{
    ClickHouseDialect, DuckDbDialect, MsSqlDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect,
};
use sqlparser::parser::Parser;

/// Parser dialect family used for SQL scope. Kept explicit because the scope
/// shape (database vs namespace) and the parser grammar must agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlScopeDialect {
    MySql,
    PostgreSql,
    MsSql,
    DuckDb,
    ClickHouse,
    Sqlite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeFamily {
    Database,
    Namespace,
}

impl SqlScopeDialect {
    /// Map a driver id (including wire-reuse aliases) to its SQL family.
    /// Unknown/non-SQL drivers return `None`; an active scope then fails closed
    /// instead of guessing.
    pub fn from_driver_id(driver_id: &str) -> Option<Self> {
        let id = driver_id.trim().to_ascii_lowercase();
        if [
            "mysql",
            "mariadb",
            "doris",
            "starrocks",
            "oceanbase",
            "manticore",
        ]
        .iter()
        .any(|prefix| id.starts_with(prefix))
        {
            return Some(Self::MySql);
        }
        if [
            "postgres",
            "questdb",
            "cloudberry",
            "timescale",
            "risingwave",
        ]
        .iter()
        .any(|prefix| id.starts_with(prefix))
        {
            return Some(Self::PostgreSql);
        }
        if ["sqlserver", "mssql"]
            .iter()
            .any(|prefix| id.starts_with(prefix))
        {
            return Some(Self::MsSql);
        }
        if id.starts_with("duckdb") {
            return Some(Self::DuckDb);
        }
        if id.starts_with("clickhouse") {
            return Some(Self::ClickHouse);
        }
        if ["sqlite", "rqlite", "turso"]
            .iter()
            .any(|prefix| id.starts_with(prefix))
        {
            return Some(Self::Sqlite);
        }
        None
    }

    /// Whether this engine has a schema/namespace level in addition to its
    /// database/catalog. Used to decide whether a null profile schema must be
    /// probed.
    pub fn has_namespace_level(self) -> bool {
        matches!(self.family(), ScopeFamily::Namespace)
    }

    fn family(self) -> ScopeFamily {
        match self {
            Self::MySql | Self::ClickHouse | Self::Sqlite => ScopeFamily::Database,
            Self::PostgreSql | Self::MsSql | Self::DuckDb => ScopeFamily::Namespace,
        }
    }

    fn parse(self, sql: &str) -> Result<Vec<Statement>, sqlparser::parser::ParserError> {
        match self {
            Self::MySql => Parser::parse_sql(&MySqlDialect {}, sql),
            Self::PostgreSql => Parser::parse_sql(&PostgreSqlDialect {}, sql),
            Self::MsSql => Parser::parse_sql(&MsSqlDialect {}, sql),
            Self::DuckDb => Parser::parse_sql(&DuckDbDialect {}, sql),
            Self::ClickHouse => Parser::parse_sql(&ClickHouseDialect {}, sql),
            Self::Sqlite => Parser::parse_sql(&SQLiteDialect {}, sql),
        }
    }
}

/// Actual targets proven by one statement set. Used for audit rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SqlScopeTargets {
    pub databases: BTreeSet<String>,
    pub namespaces: BTreeSet<String>,
}

impl SqlScopeTargets {
    pub fn database_label(&self) -> Option<String> {
        (!self.databases.is_empty())
            .then(|| self.databases.iter().cloned().collect::<Vec<_>>().join(","))
    }

    pub fn namespace_label(&self) -> Option<String> {
        (!self.namespaces.is_empty()).then(|| {
            self.namespaces
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(",")
        })
    }
}

/// Validate every relation in `sql` against the active database/namespace
/// allowlists. Empty allowlists mean "no restriction for this dimension".
///
/// `default_database` and `default_namespace` are the **actual** connection
/// defaults (resolved from the driver when the profile left them null), never
/// guesses. A restricted dimension whose actual default is unknown fails
/// closed.
pub fn validate_sql_scope(
    sql: &str,
    dialect: SqlScopeDialect,
    default_database: Option<&str>,
    default_namespace: Option<&str>,
    allowed_databases: &[String],
    allowed_namespaces: &[String],
) -> Result<SqlScopeTargets, String> {
    let statements = dialect
        .parse(sql)
        .map_err(|_| "MCP SQL scope: SQL could not be parsed; refusing to execute".to_string())?;
    if statements.is_empty() {
        return Err("MCP SQL scope: empty SQL cannot be verified".to_string());
    }
    if dialect.family() == ScopeFamily::Database && !allowed_namespaces.is_empty() {
        return Err(
            "MCP SQL scope: this driver has no verifiable namespace level; namespace scope cannot be enforced"
                .to_string(),
        );
    }

    let mut collector = RelationCollector::default();
    for statement in &statements {
        match statement {
            Statement::Use(_) => {
                return Err(
                    "MCP SQL scope: USE changes the default database/schema and is not allowed"
                        .to_string(),
                );
            }
            Statement::Set(_) => {
                return Err(
                    "MCP SQL scope: SET changes session context (for example search_path) and is not allowed"
                        .to_string(),
                );
            }
            Statement::Drop {
                object_type: ObjectType::Table,
                names,
                ..
            } => {
                for name in names {
                    collector.relations.push(name.clone());
                }
            }
            _ => {}
        }
        let _ = statement.visit(&mut collector);
    }

    let mut targets = SqlScopeTargets::default();
    for relation in &collector.relations {
        let parts = relation_parts(relation)?;
        if parts.is_empty() {
            continue;
        }
        // A CTE name is not a physical relation. It is skipped globally, which
        // can only be conservative: the CTE shadows a same-named table.
        if parts.len() == 1 && collector.cte_names.contains(&fold(&parts[0])) {
            continue;
        }

        match dialect.family() {
            ScopeFamily::Database => {
                let database = if parts.len() == 1 {
                    default_database.ok_or_else(|| {
                        "MCP SQL scope: the actual default database is unknown; refusing unqualified SQL"
                            .to_string()
                    })?
                } else {
                    // MySQL/ClickHouse catalog.db.table: the database is the
                    // penultimate qualifier.
                    parts
                        .get(parts.len().saturating_sub(2))
                        .map(String::as_str)
                        .ok_or_else(|| "MCP SQL scope: malformed relation name".to_string())?
                };
                ensure_allowed("database", database, allowed_databases)?;
                targets.databases.insert(database.to_string());
            }
            ScopeFamily::Namespace => {
                let (database, namespace) = match parts.len() {
                    1 => (default_database, default_namespace),
                    2 => (default_database, Some(parts[0].as_str())),
                    3 => (Some(parts[0].as_str()), Some(parts[1].as_str())),
                    _ => {
                        return Err(
                            "MCP SQL scope: four-part relation names cannot be verified safely"
                                .to_string(),
                        );
                    }
                };
                let database = database.ok_or_else(|| {
                    "MCP SQL scope: the actual default database is unknown; refusing SQL"
                        .to_string()
                })?;
                let namespace = namespace.ok_or_else(|| {
                    "MCP SQL scope: the actual default schema is unknown; refusing SQL".to_string()
                })?;
                ensure_allowed("database", database, allowed_databases)?;
                ensure_allowed("namespace", namespace, allowed_namespaces)?;
                targets.databases.insert(database.to_string());
                targets.namespaces.insert(namespace.to_string());
            }
        }
    }

    // Deliverable shape: every statement with no relations still points at
    // the connection's actual default target, which is what audit records.
    if targets.databases.is_empty() {
        if let Some(database) = default_database {
            targets.databases.insert(database.to_string());
        }
    }
    if targets.namespaces.is_empty() {
        if let Some(namespace) = default_namespace {
            targets.namespaces.insert(namespace.to_string());
        }
    }
    Ok(targets)
}

fn ensure_allowed(label: &str, value: &str, allowed: &[String]) -> Result<(), String> {
    if allowed.is_empty()
        || allowed
            .iter()
            .any(|entry| entry.trim() == "*" || entry.trim().eq_ignore_ascii_case(value))
    {
        return Ok(());
    }
    Err(format!(
        "MCP {label} scope: '{value}' is outside the allowed {label} scope"
    ))
}

fn relation_parts(name: &ObjectName) -> Result<Vec<String>, String> {
    name.0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident) => Ok(ident.value.clone()),
            _ => Err("MCP SQL scope: relation name contains a non-identifier part".to_string()),
        })
        .collect()
}

fn fold(value: &str) -> String {
    value.to_ascii_lowercase()
}

#[derive(Default)]
struct RelationCollector {
    cte_names: HashSet<String>,
    relations: Vec<ObjectName>,
}

impl Visitor for RelationCollector {
    type Break = std::convert::Infallible;

    fn pre_visit_query(&mut self, query: &sqlparser::ast::Query) -> ControlFlow<Self::Break> {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.cte_names.insert(fold(&cte.alias.name.value));
            }
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
        self.relations.push(relation.clone());
        ControlFlow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dbs(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn mysql_cross_database_and_system_schema_are_rejected() {
        let sql = "SELECT COUNT(*) FROM datazen_demo.test_orders";
        let error = validate_sql_scope(
            sql,
            SqlScopeDialect::MySql,
            Some("mysql"),
            None,
            &dbs(&["mysql"]),
            &[],
        )
        .unwrap_err();
        assert!(error.contains("datazen_demo"), "got: {error}");

        let error = validate_sql_scope(
            "SELECT COUNT(*) FROM information_schema.tables",
            SqlScopeDialect::MySql,
            Some("mysql"),
            None,
            &dbs(&["mysql"]),
            &[],
        )
        .unwrap_err();
        assert!(error.contains("information_schema"), "got: {error}");

        assert!(validate_sql_scope(
            "SELECT COUNT(*) FROM mysql.user",
            SqlScopeDialect::MySql,
            Some("mysql"),
            None,
            &dbs(&["mysql"]),
            &[],
        )
        .is_ok());
    }

    #[test]
    fn postgres_system_schemas_and_cross_schema_are_rejected() {
        for sql in [
            "SELECT COUNT(*) FROM information_schema.tables",
            "SELECT COUNT(*) FROM pg_catalog.pg_class",
            "SELECT COUNT(*) FROM secret.users",
        ] {
            let error = validate_sql_scope(
                sql,
                SqlScopeDialect::PostgreSql,
                Some("postgres"),
                Some("public"),
                &dbs(&["postgres"]),
                &dbs(&["public"]),
            )
            .unwrap_err();
            assert!(error.contains("namespace scope"), "sql={sql} got: {error}");
        }
        assert!(validate_sql_scope(
            "SELECT COUNT(*) FROM public.users",
            SqlScopeDialect::PostgreSql,
            Some("postgres"),
            Some("public"),
            &dbs(&["postgres"]),
            &dbs(&["public"]),
        )
        .is_ok());
    }

    #[test]
    fn cte_subquery_union_join_and_dml_are_covered() {
        let scope = |sql: &str| {
            validate_sql_scope(
                sql,
                SqlScopeDialect::PostgreSql,
                Some("postgres"),
                Some("public"),
                &dbs(&["postgres"]),
                &dbs(&["public"]),
            )
        };
        for sql in [
            "WITH x AS (SELECT * FROM public.users) SELECT * FROM x",
            "SELECT * FROM (SELECT * FROM public.users) x",
            "SELECT * FROM public.users UNION SELECT * FROM public.users",
            "SELECT * FROM public.users u JOIN public.orders o ON u.id = o.user_id",
            "INSERT INTO public.users (id) SELECT id FROM public.orders",
            "UPDATE public.users SET name = 'x' WHERE id = 1",
            "DELETE FROM public.users WHERE id = 1",
        ] {
            assert!(scope(sql).is_ok(), "expected allowed: {sql}");
        }
        assert!(scope("WITH x AS (SELECT * FROM secret.users) SELECT * FROM x").is_err());
        assert!(scope("SELECT * FROM public.users u JOIN secret.orders o ON u.id = o.id").is_err());
        assert!(scope("INSERT INTO secret.users (id) VALUES (1)").is_err());
    }

    #[test]
    fn unknown_defaults_and_parse_failures_fail_closed() {
        assert!(validate_sql_scope(
            "SELECT * FROM users",
            SqlScopeDialect::PostgreSql,
            None,
            Some("public"),
            &dbs(&["postgres"]),
            &dbs(&["public"]),
        )
        .is_err());
        assert!(validate_sql_scope(
            "SELECT * FROM users",
            SqlScopeDialect::PostgreSql,
            Some("postgres"),
            None,
            &dbs(&["postgres"]),
            &dbs(&["public"]),
        )
        .is_err());
        assert!(validate_sql_scope(
            "SELECT 'unterminated",
            SqlScopeDialect::PostgreSql,
            Some("postgres"),
            Some("public"),
            &dbs(&["postgres"]),
            &dbs(&["public"]),
        )
        .is_err());
    }

    #[test]
    fn use_and_set_are_rejected() {
        for sql in ["USE other", "SET search_path TO secret"] {
            assert!(validate_sql_scope(
                sql,
                SqlScopeDialect::MySql,
                Some("mysql"),
                None,
                &dbs(&["mysql"]),
                &[],
            )
            .is_err());
        }
        assert!(validate_sql_scope(
            "SET search_path TO secret",
            SqlScopeDialect::PostgreSql,
            Some("postgres"),
            Some("public"),
            &dbs(&["postgres"]),
            &dbs(&["public"]),
        )
        .is_err());
    }

    #[test]
    fn no_relation_still_carries_actual_defaults_for_audit() {
        let targets = validate_sql_scope(
            "SELECT 1",
            SqlScopeDialect::PostgreSql,
            Some("postgres"),
            Some("public"),
            &dbs(&["postgres"]),
            &dbs(&["public"]),
        )
        .unwrap();
        assert_eq!(targets.database_label().as_deref(), Some("postgres"));
        assert_eq!(targets.namespace_label().as_deref(), Some("public"));
    }
}

#[cfg(test)]
mod family_guard_tests {
    use super::*;

    #[test]
    fn namespace_scope_on_a_database_family_fails_even_without_relations() {
        let error = validate_sql_scope(
            "SELECT 1",
            SqlScopeDialect::MySql,
            Some("mysql"),
            None,
            &[],
            &["public".to_string()],
        )
        .unwrap_err();
        assert!(error.contains("namespace scope"), "got: {error}");
    }
}
