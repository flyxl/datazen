//! Shared types for schema diff plans and deploy results.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSnapshot {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ColumnChange {
    DataType,
    Nullable,
    Default,
    Comment,
    AutoIncrement,
    PrimaryKey,
}

impl ColumnChange {
    fn as_str(&self) -> &'static str {
        match self {
            Self::DataType => "dataType",
            Self::Nullable => "nullable",
            Self::Default => "default",
            Self::Comment => "comment",
            Self::AutoIncrement => "autoIncrement",
            Self::PrimaryKey => "isPrimaryKey",
        }
    }
}
impl PartialEq<String> for ColumnChange {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}
impl PartialEq<ColumnChange> for String {
    fn eq(&self, other: &ColumnChange) -> bool {
        self == other.as_str()
    }
}
impl From<&str> for ColumnChange {
    fn from(value: &str) -> Self {
        match value {
            "dataType" => Self::DataType,
            "nullable" => Self::Nullable,
            "default" => Self::Default,
            "comment" => Self::Comment,
            "autoIncrement" => Self::AutoIncrement,
            "isPrimaryKey" => Self::PrimaryKey,
            _ => panic!("unknown column change: {value}"),
        }
    }
}
impl From<String> for ColumnChange {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanWarning {
    pub code: String,
    pub message: String,
    pub destructive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PlanRequirement {
    Backfill {
        table: String,
        column: String,
        reason: String,
    },
    Unsupported {
        operation: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChangedColumnDiff {
    pub name: String,
    pub source: ColumnSnapshot,
    pub target: ColumnSnapshot,
    pub changes: Vec<ColumnChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableColumnDiff {
    pub table: String,
    pub missing_on_target: Vec<ColumnSnapshot>,
    pub extra_on_target: Vec<ColumnSnapshot>,
    pub changed: Vec<ChangedColumnDiff>,
    pub added: Vec<ColumnSnapshot>,
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
    pub requirements: Vec<PlanRequirement>,
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
    Cancelled,
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
