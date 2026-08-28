//! Shared types for schema diff plans and deploy results.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSnapshot {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChangedColumnDiff {
    pub name: String,
    pub source: ColumnSnapshot,
    pub target: ColumnSnapshot,
    pub changes: Vec<String>,
}

/// Column-level diff with **source = desired**.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableColumnDiff {
    pub table: String,
    /// Present on source, missing on target → ADD on deploy.
    pub missing_on_target: Vec<ColumnSnapshot>,
    /// Present on target only → DROP on deploy (destructive).
    pub extra_on_target: Vec<ColumnSnapshot>,
    pub changed: Vec<ChangedColumnDiff>,
    /// Legacy alias of `missing_on_target` (source-desired ADD).
    pub added: Vec<ColumnSnapshot>,
    /// Legacy alias of `extra_on_target` (source-desired DROP).
    pub removed: Vec<ColumnSnapshot>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatementRisk {
    Additive,
    Destructive,
    Rewrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanStatement {
    pub sql: String,
    pub risk: StatementRisk,
    pub rollback_sql: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffPlan {
    pub table: String,
    pub tables: Vec<String>,
    pub source_dialect: String,
    pub target_dialect: String,
    pub same_dialect: bool,
    pub statements: Vec<PlanStatement>,
    pub warnings: Vec<String>,
    pub rollback_completeness: RollbackCompleteness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RollbackCompleteness {
    pub complete: bool,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployStatus {
    Committed,
    RolledBack,
    Mixed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffDeployResult {
    pub status: DeployStatus,
    pub executed_count: usize,
    pub statement_count: usize,
    pub errors: Vec<String>,
    pub statement_results: Vec<StatementExecResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatementExecResult {
    pub index: usize,
    pub sql: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdlAtomicity {
    Transactional,
    AutoCommitPerStatement,
    Unknown,
}

pub fn ddl_atomicity(dialect: &str) -> DdlAtomicity {
    match dialect.to_ascii_lowercase().as_str() {
        "postgresql" | "postgres" | "sqlite" => DdlAtomicity::Transactional,
        "mysql" | "mariadb" | "tidb" | "oceanbase" => DdlAtomicity::AutoCommitPerStatement,
        _ => DdlAtomicity::Unknown,
    }
}

pub fn normalize_dialect(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" => "postgresql".into(),
        "mariadb" | "mysql" => "mysql".into(),
        "sqlite" => "sqlite".into(),
        other => other.to_string(),
    }
}

/// Map a source-side table pick to the identifier expected by a driver's `get_table_schema`.
///
/// UI picks from PostgreSQL are often schema-qualified (`public.users`). MySQL/SQLite
/// sessions use the active database and expect an unqualified table name.
pub fn resolve_table_for_dialect(dialect: &str, table: &str) -> String {
    let trimmed = table.trim();
    match normalize_dialect(dialect).as_str() {
        "mysql" | "sqlite" => trimmed
            .rsplit_once('.')
            .map(|(_, name)| name.to_string())
            .unwrap_or_else(|| trimmed.to_string()),
        _ => trimmed.to_string(),
    }
}

#[cfg(test)]
mod resolve_tests {
    use super::resolve_table_for_dialect;

    #[test]
    fn mysql_strips_pg_schema_prefix() {
        assert_eq!(
            resolve_table_for_dialect("mysql", "public.sd_cross_pg_mysql_abc"),
            "sd_cross_pg_mysql_abc"
        );
    }

    #[test]
    fn postgres_keeps_schema_qualified_name() {
        assert_eq!(
            resolve_table_for_dialect("postgresql", "public.users"),
            "public.users"
        );
    }

    #[test]
    fn bare_name_unchanged_for_all_dialects() {
        assert_eq!(resolve_table_for_dialect("mysql", "users"), "users");
        assert_eq!(resolve_table_for_dialect("postgresql", "users"), "users");
    }
}
