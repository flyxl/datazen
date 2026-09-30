//! Shared types for schema diff plans and deploy results.

use crate::db::TableOptions;
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
pub struct CheckConstraintSnapshot {
    pub name: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TableOptionChange {
    Comment,
    Engine,
    Charset,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableOptionsDiff {
    pub source: TableOptions,
    pub target: TableOptions,
    pub changes: Vec<TableOptionChange>,
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
    #[serde(default)]
    pub missing_check_constraints: Vec<CheckConstraintSnapshot>,
    #[serde(default)]
    pub extra_check_constraints: Vec<CheckConstraintSnapshot>,
    #[serde(default)]
    pub table_options: Option<TableOptionsDiff>,
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
    /// This statement is one step in a multi-statement change whose rollback
    /// guarantee comes from the host transaction. Deploy must refuse to run
    /// it when transactions are unavailable or disabled.
    #[serde(default)]
    pub requires_transaction: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TypeSuggestion {
    pub table: String,
    pub column: String,
    pub source_type: String,
    pub suggested_type: String,
    pub current_type: String,
    pub reason: String,
    pub is_key_or_indexed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColumnTypeOverride {
    pub table: String,
    pub column: String,
    pub target_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffPlan {
    #[serde(default)]
    pub plan_id: Option<String>,
    pub table: String,
    pub tables: Vec<String>,
    pub source_dialect: String,
    pub target_dialect: String,
    pub same_dialect: bool,
    pub statements: Vec<PlanStatement>,
    pub warnings: Vec<String>,
    pub requirements: Vec<PlanRequirement>,
    pub rollback_completeness: RollbackCompleteness,
    #[serde(default)]
    pub type_suggestions: Vec<TypeSuggestion>,
    /// Target snapshots used by transactional table rebuilds; deploy checks for stale review state before writing.
    #[serde(default)]
    pub expected_target_schemas: Vec<crate::db::TableSchema>,
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
    Unknown,
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

pub use crate::db::DdlAtomicity;

pub fn normalize_dialect(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" => "postgresql".into(),
        "mariadb" | "mysql" => "mysql".into(),
        "sqlite" => "sqlite".into(),
        other => other.to_string(),
    }
}

/// Whether a dialect uses schema as the relation scope inside the configured
/// database. SQL Server and PostgreSQL both keep schema identity separate from
/// database/catalog identity.
pub fn uses_schema_scope(dialect: &str) -> bool {
    matches!(
        normalize_dialect(dialect).as_str(),
        "postgresql" | "sqlserver"
    )
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
    use super::{resolve_table_for_dialect, uses_schema_scope};
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
    fn sqlserver_uses_schema_scope_and_keeps_qualified_relation() {
        assert!(uses_schema_scope("sqlserver"));
        assert!(uses_schema_scope("postgres"));
        assert!(!uses_schema_scope("mysql"));
        assert_eq!(
            resolve_table_for_dialect("sqlserver", "sales.orders"),
            "sales.orders"
        );
    }
    #[test]
    fn bare_name_unchanged_for_all_dialects() {
        assert_eq!(resolve_table_for_dialect("mysql", "users"), "users");
        assert_eq!(resolve_table_for_dialect("postgresql", "users"), "users");
    }
}
