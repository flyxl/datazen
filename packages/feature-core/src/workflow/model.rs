//! Workflow data model and execution result types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowVisibility {
    #[default]
    User,
    DashboardHidden,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 定义格式版本（裸整数字符串）。缺省 = v1（扁平 `steps`，独立 step 语义）。
    ///
    /// 这里只负责**存**，判定在 [`super::executable::WorkflowVersion::parse`]：
    /// 非整数与高于上限的整数都拒绝执行，绝不按低版本猜测解析。
    #[serde(default)]
    pub version: Option<String>,
    pub author: Option<String>,
    #[serde(default)]
    pub variables: Vec<WorkflowVariable>,
    /// Default Connection inherited by Data-operation Steps.
    #[serde(default)]
    pub connection: Option<String>,
    /// Default Database inherited by Data-operation Steps (for multi-db connections).
    #[serde(default)]
    pub database: Option<String>,
    pub steps: Vec<WorkflowStep>,
    /// v2 资源块。v1 定义里恒为空——解析层把 `steps` 映射为独立 step 序列，
    /// 而不是把它们塞进块里。
    #[serde(default)]
    pub blocks: Vec<WorkflowBlock>,
    pub output: Option<WorkflowOutput>,
    pub timeout_secs: Option<u64>,
    pub error_handling: Option<ErrorHandlingConfig>,
    /// Optional interval schedule (jobs / timed backup via Workflow).
    #[serde(default)]
    pub schedule: Option<WorkflowSchedule>,
    /// `user` appears in Workflow UI; `dashboardHidden` is dashboard-owned.
    #[serde(default)]
    pub visibility: WorkflowVisibility,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct WorkflowSchedule {
    #[serde(default)]
    pub enabled: bool,
    /// Run every N seconds. Values below 30 are clamped at runtime.
    #[serde(default, alias = "intervalSecs")]
    pub interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowMigrationOperation {
    DataTransfer,
    DataSync,
    SchemaDiff,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnattendedDestructivePolicy {
    #[default]
    Reject,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowVariable {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: String,
    pub description: String,
    pub required: Option<bool>,
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum WorkflowStep {
    #[serde(rename = "query")]
    Query {
        id: String,
        sql: String,
        connection: Option<String>,
        database: Option<String>,
        timeout_secs: Option<u64>,
        on_error: Option<ErrorHandlingConfig>,
    },
    #[serde(rename = "command")]
    Command {
        id: String,
        command: String,
        #[serde(default)]
        connection: Option<String>,
        #[serde(default)]
        input: serde_json::Value,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        on_error: Option<ErrorHandlingConfig>,
    },
    #[serde(rename = "ai")]
    Ai {
        id: String,
        prompt: String,
        timeout_secs: Option<u64>,
        on_error: Option<ErrorHandlingConfig>,
    },
    #[serde(rename = "condition")]
    Condition {
        id: String,
        #[serde(rename = "if")]
        expr: String,
        then_steps: Vec<WorkflowStep>,
        else_steps: Option<Vec<WorkflowStep>>,
    },
    #[serde(rename = "foreach")]
    ForEach {
        id: String,
        items: String,
        as_var: String,
        steps: Vec<WorkflowStep>,
        max_iterations: Option<usize>,
    },
    #[serde(rename = "merge")]
    Merge {
        id: String,
        /// Ordered list of row groups concatenated into a single table.
        #[serde(default)]
        sources: Vec<MergeSource>,
        /// Optional global column order for the output table (unnamed source columns are appended after).
        #[serde(default)]
        columns: Option<Vec<String>>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        on_error: Option<ErrorHandlingConfig>,
    },
    #[serde(rename = "transform")]
    Transform {
        id: String,
        /// Source expression (usually `steps.<id>.rows` or a literal JSON array).
        from: String,
        /// Row-level computed columns: `name -> expression`.
        #[serde(default)]
        add_columns: Vec<TransformColumn>,
        /// Optional row filter (reuses the `conditions` comparison grammar on cell values).
        #[serde(default)]
        filter: Option<String>,
        /// Optional column to sort by; prefix with `-` for descending.
        #[serde(default)]
        sort_by: Option<String>,
        /// Number of leading rows to skip before emitting.
        #[serde(default)]
        offset: Option<usize>,
        /// Maximum number of rows to emit.
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        on_error: Option<ErrorHandlingConfig>,
    },
    /// Execute a persisted migration profile through the host-owned migration
    /// services. This is intentionally separate from Driver Commands because
    /// it owns profile revision checks, fresh sessions, immutable plans, and
    /// migration run history.
    #[serde(rename = "migration")]
    Migration {
        id: String,
        operation: WorkflowMigrationOperation,
        #[serde(alias = "profileId")]
        profile_id: String,
        #[serde(default, alias = "profileRevision")]
        profile_revision: Option<String>,
        #[serde(default, alias = "destructivePolicy")]
        destructive_policy: UnattendedDestructivePolicy,
        /// Name of the per-run variable containing a native-dialog-issued
        /// SQL-file token. The token itself is never persisted in a profile.
        #[serde(default, alias = "sqlFileTokenVariable")]
        sql_file_token_variable: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        on_error: Option<ErrorHandlingConfig>,
    },
}

/// 资源块类型（v2）。块是**资源作用域**的单位，不是 step 的语法糖：
///
/// * [`BlockKind::SessionBlock`]：整块共用一份资源，块内可见块内状态，
///   块结束无条件释放。**没有运行期原子性**。
/// * [`BlockKind::TransactionBlock`]：整块共用一份资源，并由执行层控制
///   begin / commit / rollback。是否可执行由 [`super::plan_guard`] 在**任何
///   写入发生之前**判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockKind {
    SessionBlock,
    TransactionBlock,
}

impl BlockKind {
    /// 块内是否有运行期原子性（与「共用资源」是两件事）。
    pub const fn is_transactional(self) -> bool {
        matches!(self, BlockKind::TransactionBlock)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            BlockKind::SessionBlock => "sessionBlock",
            BlockKind::TransactionBlock => "transactionBlock",
        }
    }
}

/// 块级重试策略（只适用 `transactionBlock`）。
///
/// 键名是 `block` 而不是 `times`，因为重试单位只能是整块：块内不允许从中间
/// 恢复（§3.5）。未知键直接拒绝，避免 `retry: { step: 1 }` 这类写法被静默吞掉。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BlockRetry {
    /// 整块重试次数上限。`0` = 不重试（默认）。
    #[serde(rename = "block", default)]
    pub block: u32,
}

/// 一个 v2 资源块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowBlock {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: BlockKind,
    /// 块级目标（L2）。缺省继承 Workflow 默认目标（L3）。
    #[serde(default)]
    pub connection: Option<String>,
    /// 块级目标（L2）。缺省继承 Workflow 默认目标（L3）。
    #[serde(default)]
    pub database: Option<String>,
    /// 与其他块并行。`false`（默认）时块按声明顺序串行。
    #[serde(default)]
    pub parallel: bool,
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub retry: Option<BlockRetry>,
}

/// One input group feeding a `merge` step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergeSource {
    /// Path expression resolving to a JSON array of objects.
    pub source: String,
    /// Column projection + rename: `output_name -> source_field` (dot path).
    #[serde(default)]
    pub columns: serde_json::Map<String, serde_json::Value>,
    /// Constant columns injected into every row of this group (e.g. `{ src: "PG" }`).
    #[serde(default)]
    pub add: serde_json::Map<String, serde_json::Value>,
}

/// A computed column for a `transform` step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformColumn {
    pub name: String,
    pub expr: String,
}

impl WorkflowStep {
    pub fn step_id(&self) -> &str {
        match self {
            Self::Query { id, .. }
            | Self::Command { id, .. }
            | Self::Ai { id, .. }
            | Self::Condition { id, .. }
            | Self::ForEach { id, .. }
            | Self::Merge { id, .. }
            | Self::Transform { id, .. }
            | Self::Migration { id, .. } => id,
        }
    }
    pub fn step_type_str(&self) -> &'static str {
        match self {
            Self::Query { .. } => "query",
            Self::Command { .. } => "command",
            Self::Ai { .. } => "ai",
            Self::Condition { .. } => "condition",
            Self::ForEach { .. } => "foreach",
            Self::Merge { .. } => "merge",
            Self::Transform { .. } => "transform",
            Self::Migration { .. } => "migration",
        }
    }
    pub fn on_error_strategy(&self) -> Option<ErrorStrategy> {
        match self {
            Self::Query { on_error, .. }
            | Self::Command { on_error, .. }
            | Self::Ai { on_error, .. }
            | Self::Merge { on_error, .. }
            | Self::Transform { on_error, .. }
            | Self::Migration { on_error, .. } => on_error.as_ref().map(|c| c.to_strategy()),
            _ => None,
        }
    }
    pub fn timeout_secs(&self) -> Option<u64> {
        match self {
            Self::Query { timeout_secs, .. }
            | Self::Command { timeout_secs, .. }
            | Self::Ai { timeout_secs, .. }
            | Self::Merge { timeout_secs, .. }
            | Self::Transform { timeout_secs, .. }
            | Self::Migration { timeout_secs, .. } => *timeout_secs,
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowOutput {
    pub format: String,
    pub template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorHandlingConfig {
    pub strategy: ErrorStrategyKind,
    #[serde(default)]
    pub fallback_steps: Option<Vec<WorkflowStep>>,
}

impl ErrorHandlingConfig {
    pub fn to_strategy(&self) -> ErrorStrategy {
        match self.strategy {
            ErrorStrategyKind::Abort => ErrorStrategy::Abort,
            ErrorStrategyKind::Skip => ErrorStrategy::Skip,
            ErrorStrategyKind::Fallback => ErrorStrategy::Fallback {
                steps: self.fallback_steps.clone().unwrap_or_default(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorStrategyKind {
    Abort,
    Skip,
    Fallback,
}

#[derive(Debug, Clone)]
pub enum ErrorStrategy {
    Abort,
    Skip,
    Fallback { steps: Vec<WorkflowStep> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionResult {
    pub success: bool,
    pub final_output: String,
    pub steps: Vec<StepExecutionResult>,
    pub total_time_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepExecutionResult {
    pub step_id: String,
    pub step_type: String,
    pub status: StepStatus,
    pub result: Option<serde_json::Value>,
    pub execution_time_ms: u64,
    pub error: Option<String>,
    pub connection_name: Option<String>,
    pub sql_executed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Success,
    Failed,
    Skipped,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowListItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub variables: Vec<WorkflowVariable>,
    #[serde(default)]
    pub scheduled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_query_yaml_still_deserializes() {
        let step: WorkflowStep =
            serde_yaml::from_str("type: query\nid: q\nsql: SELECT 1\n").unwrap();
        assert!(matches!(step, WorkflowStep::Query { .. }));
    }
    #[test]
    fn command_yaml_deserializes() {
        let yaml = "type: command\nid: find\ncommand: find\nconnection: mongo\ninput:\n  collection: users\n";
        let step: WorkflowStep = serde_yaml::from_str(yaml).unwrap();
        match step {
            WorkflowStep::Command {
                command,
                connection,
                input,
                ..
            } => {
                assert_eq!(command, "find");
                assert_eq!(connection.as_deref(), Some("mongo"));
                assert_eq!(input["collection"], "users");
            }
            _ => panic!("expected Command"),
        }
    }
    #[test]
    fn workflow_connection_is_optional() {
        let workflow: WorkflowDefinition =
            serde_yaml::from_str("id: w\nname: W\ndescription: d\nsteps: []\n").unwrap();
        assert_eq!(workflow.connection, None);
        assert_eq!(workflow.schedule, None);
    }

    #[test]
    fn workflow_schedule_yaml_deserializes() {
        let yaml = "id: w\nname: W\ndescription: d\nsteps: []\nschedule:\n  enabled: true\n  interval_secs: 120\n";
        let workflow: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        let schedule = workflow.schedule.unwrap();
        assert!(schedule.enabled);
        assert_eq!(schedule.interval_secs, Some(120));
    }

    #[test]
    fn workflow_schedule_accepts_camel_case_interval() {
        let json = r#"{"id":"w","name":"W","description":"d","steps":[],"schedule":{"enabled":true,"intervalSecs":45}}"#;
        let workflow: WorkflowDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(workflow.schedule.unwrap().interval_secs, Some(45));
    }

    #[test]
    fn migration_step_is_host_owned_and_defaults_to_reject() {
        let step: WorkflowStep = serde_yaml::from_str(
            "type: migration\nid: nightly-sync\noperation: dataSync\nprofile_id: profile-1\n",
        )
        .unwrap();
        match step {
            WorkflowStep::Migration {
                destructive_policy,
                sql_file_token_variable,
                ..
            } => {
                assert_eq!(destructive_policy, UnattendedDestructivePolicy::Reject);
                assert_eq!(sql_file_token_variable, None);
            }
            other => panic!("expected migration step, got {other:?}"),
        }
    }

    #[test]
    fn migration_step_serialization_does_not_contain_a_file_token() {
        let step = WorkflowStep::Migration {
            id: "export".into(),
            operation: WorkflowMigrationOperation::DataTransfer,
            profile_id: "profile-1".into(),
            profile_revision: None,
            destructive_policy: UnattendedDestructivePolicy::Allow,
            sql_file_token_variable: Some("outputToken".into()),
            timeout_secs: None,
            on_error: None,
        };
        let encoded = serde_json::to_string(&step).unwrap();
        assert!(encoded.contains("outputToken"));
        assert!(!encoded.contains("fileToken"));
    }
}
