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
    pub version: Option<String>,
    pub author: Option<String>,
    #[serde(default)]
    pub variables: Vec<WorkflowVariable>,
    /// Default Connection inherited by Data-operation Steps.
    #[serde(default)]
    pub connection: Option<String>,
    pub steps: Vec<WorkflowStep>,
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
            | Self::Transform { id, .. } => id,
        }
    }
    pub(crate) fn step_type_str(&self) -> &'static str {
        match self {
            Self::Query { .. } => "query",
            Self::Command { .. } => "command",
            Self::Ai { .. } => "ai",
            Self::Condition { .. } => "condition",
            Self::ForEach { .. } => "foreach",
            Self::Merge { .. } => "merge",
            Self::Transform { .. } => "transform",
        }
    }
    pub(crate) fn on_error_strategy(&self) -> Option<ErrorStrategy> {
        match self {
            Self::Query { on_error, .. }
            | Self::Command { on_error, .. }
            | Self::Ai { on_error, .. }
            | Self::Merge { on_error, .. }
            | Self::Transform { on_error, .. } => on_error.as_ref().map(|c| c.to_strategy()),
            _ => None,
        }
    }
    pub(crate) fn timeout_secs(&self) -> Option<u64> {
        match self {
            Self::Query { timeout_secs, .. }
            | Self::Command { timeout_secs, .. }
            | Self::Ai { timeout_secs, .. }
            | Self::Merge { timeout_secs, .. }
            | Self::Transform { timeout_secs, .. } => *timeout_secs,
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
}
