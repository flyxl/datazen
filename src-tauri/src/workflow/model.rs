//! Workflow data model and execution result types.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkflowStep {
    #[serde(rename = "query")]
    Query { id: String, sql: String, connection: Option<String>, database: Option<String>, timeout_secs: Option<u64>, on_error: Option<ErrorHandlingConfig> },
    #[serde(rename = "command")]
    Command { id: String, command: String, #[serde(default)] connection: Option<String>, #[serde(default)] input: serde_json::Value, #[serde(default)] timeout_secs: Option<u64>, #[serde(default)] on_error: Option<ErrorHandlingConfig> },
    #[serde(rename = "ai")]
    Ai { id: String, prompt: String, timeout_secs: Option<u64>, on_error: Option<ErrorHandlingConfig> },
    #[serde(rename = "condition")]
    Condition { id: String, #[serde(rename = "if")] expr: String, then_steps: Vec<WorkflowStep>, else_steps: Option<Vec<WorkflowStep>> },
    #[serde(rename = "foreach")]
    ForEach { id: String, items: String, as_var: String, steps: Vec<WorkflowStep>, max_iterations: Option<usize> },
}

impl WorkflowStep {
    pub fn step_id(&self) -> &str {
        match self { Self::Query { id, .. } | Self::Command { id, .. } | Self::Ai { id, .. } | Self::Condition { id, .. } | Self::ForEach { id, .. } => id }
    }
    pub(crate) fn step_type_str(&self) -> &'static str {
        match self { Self::Query { .. } => "query", Self::Command { .. } => "command", Self::Ai { .. } => "ai", Self::Condition { .. } => "condition", Self::ForEach { .. } => "foreach" }
    }
    pub(crate) fn on_error_strategy(&self) -> Option<ErrorStrategy> {
        match self { Self::Query { on_error, .. } | Self::Command { on_error, .. } | Self::Ai { on_error, .. } => on_error.as_ref().map(|c| c.to_strategy()), _ => None }
    }
    pub(crate) fn timeout_secs(&self) -> Option<u64> {
        match self { Self::Query { timeout_secs, .. } | Self::Command { timeout_secs, .. } | Self::Ai { timeout_secs, .. } => *timeout_secs, _ => None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowOutput { pub format: String, pub template: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHandlingConfig { pub strategy: ErrorStrategyKind, #[serde(default)] pub fallback_steps: Option<Vec<WorkflowStep>> }

impl ErrorHandlingConfig {
    pub fn to_strategy(&self) -> ErrorStrategy {
        match self.strategy { ErrorStrategyKind::Abort => ErrorStrategy::Abort, ErrorStrategyKind::Skip => ErrorStrategy::Skip, ErrorStrategyKind::Fallback => ErrorStrategy::Fallback { steps: self.fallback_steps.clone().unwrap_or_default() } }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorStrategyKind { Abort, Skip, Fallback }

#[derive(Debug, Clone)]
pub enum ErrorStrategy { Abort, Skip, Fallback { steps: Vec<WorkflowStep> } }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionResult { pub success: bool, pub final_output: String, pub steps: Vec<StepExecutionResult>, pub total_time_ms: u64, pub error: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepExecutionResult { pub step_id: String, pub step_type: String, pub status: StepStatus, pub result: Option<serde_json::Value>, pub execution_time_ms: u64, pub error: Option<String>, pub connection_name: Option<String>, pub sql_executed: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus { Success, Failed, Skipped, TimedOut }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowListItem { pub id: String, pub name: String, pub description: String, pub variables: Vec<WorkflowVariable> }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_query_yaml_still_deserializes() {
        let step: WorkflowStep = serde_yaml::from_str("type: query\nid: q\nsql: SELECT 1\n").unwrap();
        assert!(matches!(step, WorkflowStep::Query { .. }));
    }
    #[test]
    fn command_yaml_deserializes() {
        let yaml = "type: command\nid: find\ncommand: find\nconnection: mongo\ninput:\n  collection: users\n";
        let step: WorkflowStep = serde_yaml::from_str(yaml).unwrap();
        match step { WorkflowStep::Command { command, connection, input, .. } => { assert_eq!(command, "find"); assert_eq!(connection.as_deref(), Some("mongo")); assert_eq!(input["collection"], "users"); }, _ => panic!("expected Command") }
    }
    #[test]
    fn workflow_connection_is_optional() {
        let workflow: WorkflowDefinition = serde_yaml::from_str("id: w\nname: W\ndescription: d\nsteps: []\n").unwrap();
        assert_eq!(workflow.connection, None);
    }
}
