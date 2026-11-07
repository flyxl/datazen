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
    if dialect.family() == ScopeFamily::Database && !allowed_namespaces.is_empty() {
        return Err(
            "MCP SQL scope: this driver has no verifiable namespace level; namespace scope cannot be enforced"
                .to_string(),
        );
    }
    let targets = collect_sql_targets(sql, dialect, default_database, default_namespace)?;
    if !allowed_databases.is_empty() && targets.databases.is_empty() {
        return Err(
            "MCP SQL scope: the actual default database is unknown; refusing SQL".to_string(),
        );
    }
    if !allowed_namespaces.is_empty() && targets.namespaces.is_empty() {
        return Err(
            "MCP SQL scope: the actual default schema is unknown; refusing SQL".to_string(),
        );
    }
    for database in &targets.databases {
        ensure_allowed("database", database, allowed_databases)?;
    }
    for namespace in &targets.namespaces {
        ensure_allowed("namespace", namespace, allowed_namespaces)?;
    }
    Ok(targets)
}

/// Parse the SQL target(s) without applying the allowlist. The dispatcher uses
/// this only to record what a rejected statement attempted, so audit evidence
/// names `datazen_demo`/`information_schema` instead of the allowed default.
pub fn attempted_sql_targets(
    sql: &str,
    dialect: SqlScopeDialect,
    default_database: Option<&str>,
    default_namespace: Option<&str>,
) -> Option<SqlScopeTargets> {
    collect_sql_targets(sql, dialect, default_database, default_namespace).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShowTargetKind {
    /// `SHOW TABLES FROM <database/schema>` names the containing catalog or
    /// namespace, not a relation inside it.
    Database,
    /// `SHOW CREATE TABLE`, `SHOW COLUMNS`, `SHOW INDEX` name a table/view.
    Object,
}

enum ShowScope {
    /// Enumerates every database/schema and therefore leaks scope membership.
    Enumerate,
    Target(Option<ObjectName>, ShowTargetKind),
}

fn collect_sql_targets(
    sql: &str,
    dialect: SqlScopeDialect,
    default_database: Option<&str>,
    default_namespace: Option<&str>,
) -> Result<SqlScopeTargets, String> {
    let statements = match dialect.parse(sql) {
        Ok(statements) => statements,
        Err(parse_error) => {
            return match show_scope_from_text(sql) {
                Some(Ok(scope)) => {
                    let mut targets = SqlScopeTargets::default();
                    apply_show_scope(
                        &mut targets,
                        scope,
                        dialect,
                        default_database,
                        default_namespace,
                    )?;
                    fill_default_targets(&mut targets, default_database, default_namespace);
                    Ok(targets)
                }
                Some(Err(message)) => Err(message),
                None => Err(format!(
                    "MCP SQL scope: SQL could not be parsed; refusing to execute ({parse_error})"
                )),
            };
        }
    };
    if statements.is_empty() {
        return Err("MCP SQL scope: empty SQL cannot be verified".to_string());
    }

    let mut collector = RelationCollector::default();
    let mut targets = SqlScopeTargets::default();
    for statement in &statements {
        if let Some(scope) = show_scope(statement) {
            apply_show_scope(
                &mut targets,
                scope,
                dialect,
                default_database,
                default_namespace,
            )?;
            continue;
        }
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
        insert_relation_target(
            &mut targets,
            Some(relation),
            dialect,
            default_database,
            default_namespace,
        )?;
    }
    fill_default_targets(&mut targets, default_database, default_namespace);
    Ok(targets)
}

fn fill_default_targets(
    targets: &mut SqlScopeTargets,
    default_database: Option<&str>,
    default_namespace: Option<&str>,
) {
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
}

fn show_scope(statement: &Statement) -> Option<ShowScope> {
    match statement {
        Statement::ShowDatabases { .. }
        | Statement::ShowSchemas { .. }
        | Statement::ShowObjects(_) => Some(ShowScope::Enumerate),
        Statement::ShowTables { show_options, .. } | Statement::ShowViews { show_options, .. } => {
            Some(ShowScope::Target(
                show_options
                    .show_in
                    .as_ref()
                    .and_then(|show_in| show_in.parent_name.clone()),
                ShowTargetKind::Database,
            ))
        }
        Statement::ShowColumns { show_options, .. } => Some(ShowScope::Target(
            show_options
                .show_in
                .as_ref()
                .and_then(|show_in| show_in.parent_name.clone()),
            ShowTargetKind::Object,
        )),
        Statement::ShowCreate { obj_name, .. } => Some(ShowScope::Target(
            Some(obj_name.clone()),
            ShowTargetKind::Object,
        )),
        // sqlparser models several MySQL SHOW forms as a generic
        // `ShowVariable`. Extract their target so `SHOW TABLE STATUS FROM db`,
        // `SHOW INDEX FROM db.table`, etc. cannot enumerate outside scope.
        Statement::ShowVariable { variable } => {
            let first = variable.first()?.value.to_ascii_uppercase();
            let (target_bearing, kind) = match first.as_str() {
                "INDEX" | "INDEXES" | "KEYS" | "COLUMNS" | "FIELDS" => {
                    (true, ShowTargetKind::Object)
                }
                "TABLE" | "TABLES" | "TRIGGER" | "TRIGGERS" | "EVENT" | "EVENTS" | "FUNCTION"
                | "FUNCTIONS" | "PROCEDURE" | "PROCEDURES" => (true, ShowTargetKind::Database),
                _ => (false, ShowTargetKind::Object),
            };
            if !target_bearing {
                return None;
            }
            let from = variable.iter().position(|part| {
                part.value.eq_ignore_ascii_case("FROM") || part.value.eq_ignore_ascii_case("IN")
            });
            let target = from.and_then(|index| {
                let parts: Vec<ObjectNamePart> = variable[index + 1..]
                    .iter()
                    .map(|ident| ObjectNamePart::Identifier(ident.clone()))
                    .collect();
                (!parts.is_empty()).then(|| ObjectName(parts))
            });
            Some(ShowScope::Target(target, kind))
        }
        _ => None,
    }
}

fn show_scope_from_text(sql: &str) -> Option<Result<ShowScope, String>> {
    let trimmed = sql.trim();
    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("SHOW") {
        return None;
    }
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() < 2 || !words[0].eq_ignore_ascii_case("SHOW") {
        return None;
    }
    if trimmed.trim_end_matches(';').contains(';') {
        return Some(Err(
            "MCP SQL scope: multiple SHOW statements cannot be verified together".to_string(),
        ));
    }
    let mut index = 1;
    while index < words.len()
        && matches!(
            words[index].to_ascii_uppercase().as_str(),
            "FULL" | "EXTENDED" | "TERSE"
        )
    {
        index += 1;
    }
    let Some(kind) = words.get(index).map(|value| value.to_ascii_uppercase()) else {
        return Some(Err(
            "MCP SQL scope: SHOW target cannot be verified".to_string()
        ));
    };
    let result = match kind.as_str() {
        "DATABASES" | "SCHEMAS" => Err(
            "MCP SQL scope: SHOW DATABASES/SCHEMAS would enumerate targets outside the scope"
                .to_string(),
        ),
        "TABLES" | "VIEWS" => Ok(ShowScope::Target(
            show_text_target(&words, index + 1),
            ShowTargetKind::Database,
        )),
        "COLUMNS" | "FIELDS" | "INDEX" | "INDEXES" | "KEYS" => Ok(ShowScope::Target(
            show_text_target(&words, index + 1),
            ShowTargetKind::Object,
        )),
        "CREATE" => {
            let Some(object) = words.get(index + 1) else {
                return Some(Err(
                    "MCP SQL scope: SHOW CREATE target cannot be verified".to_string()
                ));
            };
            let kind = if object.eq_ignore_ascii_case("DATABASE") {
                ShowTargetKind::Database
            } else {
                ShowTargetKind::Object
            };
            Ok(ShowScope::Target(
                words
                    .get(index + 2)
                    .and_then(|token| parse_object_text(token)),
                kind,
            ))
        }
        _ => Err(format!(
            "MCP SQL scope: SHOW {kind} cannot be verified by the scope"
        )),
    };
    Some(result)
}

fn show_text_target(words: &[&str], from: usize) -> Option<ObjectName> {
    let mut index = from;
    while index < words.len() {
        if words[index].eq_ignore_ascii_case("FROM") || words[index].eq_ignore_ascii_case("IN") {
            return words
                .get(index + 1)
                .and_then(|token| parse_object_text(token));
        }
        index += 1;
    }
    None
}

fn parse_object_text(token: &str) -> Option<ObjectName> {
    let token = token.trim().trim_end_matches(';');
    if token.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for part in token.split('.') {
        let part = part
            .trim()
            .trim_matches('`')
            .trim_matches('"')
            .trim_matches('\'')
            .trim();
        if part.is_empty() || part.contains(char::is_whitespace) {
            return None;
        }
        parts.push(ObjectNamePart::Identifier(sqlparser::ast::Ident::new(part)));
    }
    (!parts.is_empty()).then(|| ObjectName(parts))
}

fn apply_show_scope(
    targets: &mut SqlScopeTargets,
    scope: ShowScope,
    dialect: SqlScopeDialect,
    default_database: Option<&str>,
    default_namespace: Option<&str>,
) -> Result<(), String> {
    match scope {
        ShowScope::Enumerate => Err(
            "MCP SQL scope: SHOW DATABASES/SCHEMAS would enumerate targets outside the scope"
                .to_string(),
        ),
        ShowScope::Target(name, ShowTargetKind::Database) => {
            insert_show_database_target(targets, name, dialect, default_database, default_namespace)
        }
        ShowScope::Target(name, ShowTargetKind::Object) => insert_relation_target(
            targets,
            name.as_ref(),
            dialect,
            default_database,
            default_namespace,
        ),
    }
}

fn insert_show_database_target(
    targets: &mut SqlScopeTargets,
    name: Option<ObjectName>,
    dialect: SqlScopeDialect,
    default_database: Option<&str>,
    default_namespace: Option<&str>,
) -> Result<(), String> {
    let parts = match name.as_ref() {
        Some(name) => relation_parts(name)?,
        None => Vec::new(),
    };
    match dialect.family() {
        ScopeFamily::Database => {
            let database = match parts.as_slice() {
                [] => default_database,
                [database] => Some(database.as_str()),
                _ => parts.get(parts.len().saturating_sub(2)).map(String::as_str),
            }
            .ok_or_else(|| {
                "MCP SQL scope: the actual default database is unknown; refusing SHOW".to_string()
            })?;
            targets.databases.insert(database.to_string());
        }
        ScopeFamily::Namespace => {
            let database = default_database.ok_or_else(|| {
                "MCP SQL scope: the actual default database is unknown; refusing SHOW".to_string()
            })?;
            let namespace = match parts.as_slice() {
                [] => default_namespace,
                [namespace] => Some(namespace.as_str()),
                _ => parts.last().map(String::as_str),
            }
            .ok_or_else(|| {
                "MCP SQL scope: the actual default schema is unknown; refusing SHOW".to_string()
            })?;
            targets.databases.insert(database.to_string());
            targets.namespaces.insert(namespace.to_string());
        }
    }
    Ok(())
}

fn insert_relation_target(
    targets: &mut SqlScopeTargets,
    name: Option<&ObjectName>,
    dialect: SqlScopeDialect,
    default_database: Option<&str>,
    default_namespace: Option<&str>,
) -> Result<(), String> {
    let parts = match name {
        Some(name) => relation_parts(name)?,
        None => Vec::new(),
    };
    match dialect.family() {
        ScopeFamily::Database => {
            let database = match parts.as_slice() {
                [] => default_database.ok_or_else(|| {
                    "MCP SQL scope: the actual default database is unknown; refusing unqualified SQL"
                        .to_string()
                })?,
                [database] => database.as_str(),
                _ => parts
                    .get(parts.len().saturating_sub(2))
                    .map(String::as_str)
                    .ok_or_else(|| "MCP SQL scope: malformed relation name".to_string())?,
            };
            targets.databases.insert(database.to_string());
        }
        ScopeFamily::Namespace => {
            let (database, namespace) = match parts.as_slice() {
                [] => (default_database, default_namespace),
                [namespace] => (default_database, Some(namespace.as_str())),
                [namespace, ..] if parts.len() == 2 => (default_database, Some(namespace.as_str())),
                [database, namespace, ..] if parts.len() == 3 => {
                    (Some(database.as_str()), Some(namespace.as_str()))
                }
                _ => {
                    return Err(
                        "MCP SQL scope: four-part relation names cannot be verified safely"
                            .to_string(),
                    );
                }
            };
            let database = database.ok_or_else(|| {
                "MCP SQL scope: the actual default database is unknown; refusing SQL".to_string()
            })?;
            let namespace = namespace.ok_or_else(|| {
                "MCP SQL scope: the actual default schema is unknown; refusing SQL".to_string()
            })?;
            targets.databases.insert(database.to_string());
            targets.namespaces.insert(namespace.to_string());
        }
    }
    Ok(())
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
mod show_tests {
    use super::*;

    fn dbs(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn mysql_show_enumeration_and_qualified_targets_are_rejected() {
        for sql in [
            "SHOW TABLES FROM datazen_demo",
            "SHOW DATABASES",
            "SHOW SCHEMAS",
            "SHOW CREATE TABLE datazen_demo.test_orders",
            "SHOW INDEX FROM datazen_demo.test_orders",
            "SHOW COLUMNS FROM information_schema.columns",
        ] {
            let result = validate_sql_scope(
                sql,
                SqlScopeDialect::MySql,
                Some("mysql"),
                None,
                &dbs(&["mysql"]),
                &[],
            );
            let error = match result {
                Ok(targets) => panic!("sql={sql} unexpectedly allowed: {targets:?}"),
                Err(error) => error,
            };
            assert!(
                error.contains("scope") || error.contains("SHOW"),
                "sql={sql} got: {error}"
            );
        }
    }

    #[test]
    fn mysql_show_inside_the_allowed_database_is_accepted_with_the_actual_target() {
        let targets = validate_sql_scope(
            "SHOW TABLES FROM mysql",
            SqlScopeDialect::MySql,
            Some("mysql"),
            None,
            &dbs(&["mysql"]),
            &[],
        )
        .expect("allowed SHOW TABLES");
        assert_eq!(targets.database_label().as_deref(), Some("mysql"));

        let targets = validate_sql_scope(
            "SHOW TABLES",
            SqlScopeDialect::MySql,
            Some("mysql"),
            None,
            &dbs(&["mysql"]),
            &[],
        )
        .expect("default database SHOW TABLES");
        assert_eq!(targets.database_label().as_deref(), Some("mysql"));
    }

    #[test]
    fn postgres_show_enumeration_and_system_schemas_are_rejected() {
        for sql in [
            "SHOW TABLES FROM information_schema",
            "SHOW SCHEMAS",
            "SHOW CREATE TABLE pg_catalog.pg_class",
            "SHOW INDEX FROM pg_catalog.pg_class",
        ] {
            let result = validate_sql_scope(
                sql,
                SqlScopeDialect::PostgreSql,
                Some("postgres"),
                Some("public"),
                &dbs(&["postgres"]),
                &dbs(&["public"]),
            );
            let error = match result {
                Ok(targets) => panic!("sql={sql} unexpectedly allowed: {targets:?}"),
                Err(error) => error,
            };
            assert!(
                error.contains("scope") || error.contains("SHOW"),
                "sql={sql} got: {error}"
            );
        }
    }

    #[test]
    fn postgres_show_inside_public_is_accepted() {
        let targets = validate_sql_scope(
            "SHOW TABLES FROM public",
            SqlScopeDialect::PostgreSql,
            Some("postgres"),
            Some("public"),
            &dbs(&["postgres"]),
            &dbs(&["public"]),
        )
        .expect("public SHOW TABLES");
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
