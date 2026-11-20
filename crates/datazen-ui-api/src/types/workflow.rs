//! The workflow re-exports the aggregate still owed — ported from React
//! `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! React's `types/index.ts` re-exports twenty-four workflow names from
//! `@datazen/backend-client` (the W8 刀3c web 收口 lifted them there from
//! frontend-local definitions, keeping the aggregate's import paths
//! stable). Five of them already have Rust homes — `WorkflowVariable`,
//! `WorkflowListItem`, `StepStatus`, `StepExecutionResult` and
//! `WorkflowExecutionResult` are `ai_store_types`' field-complete lean
//! mirrors (T126, whose docs name this port as their owner) — so the
//! aggregate re-exports those five from there and this module ports the
//! other nineteen:
//!
//! `ErrorHandlingConfig`, `MergeSource`, `TransformColumn`,
//! `WorkflowBlock`, `WorkflowBlockRetry`, `WorkflowBlockType`,
//! `WorkflowDefinition`, `WorkflowExecuteRequest`, `WorkflowIdRequest`,
//! `WorkflowOutput`, `WorkflowRunGetRequest`, `WorkflowRunListItem`,
//! `WorkflowRunRecord`, `WorkflowRunsListRequest`, `WorkflowSaveRequest`,
//! `WorkflowSchedule`, `WorkflowStep`, `WorkflowStepType`,
//! `WorkflowYamlSaveRequest` — plus, at the aggregate, the two pre-lift
//! compatibility aliases (`HistoryListItem` = `WorkflowRunListItem`,
//! `HistoryEntry` = `WorkflowRunRecord`).
//!
//! Source of truth: `packages/backend-client/src/types/workflow.ts` —
//! itself the mirror of the shared Rust vocabulary
//! (`datazen::workflow::domain`) serialized camelCase; both the desktop IPC
//! rail and the server HTTP surface answer with this same shape. The
//! kernel's step enum serializes with a `type` tag that already matches the
//! flat TS shape, so **no tagged-enum folding is needed** — the frontend
//! view this module mirrors is the flat struct, exactly what the TS
//! interface declares.
//!
//! Pure contracts — no IO, no state — so the module compiles for wasm32
//! exactly like its `chart_types` sibling.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ai_store_types::{WorkflowExecutionResult, WorkflowVariable};
use crate::connection_types::Value;

/// React `WorkflowStepType = 'query' | 'command' | 'ai' | 'condition' |
/// 'foreach' | 'merge' | 'transform'` — the kinds of step a definition can
/// hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowStepType {
    Query,
    Command,
    Ai,
    Condition,
    Foreach,
    Merge,
    Transform,
}

impl WorkflowStepType {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Command => "command",
            Self::Ai => "ai",
            Self::Condition => "condition",
            Self::Foreach => "foreach",
            Self::Merge => "merge",
            Self::Transform => "transform",
        }
    }
}

/// React `ErrorHandlingConfig.strategy`'s inline union `'abort' | 'skip' |
/// 'fallback'` — what a step's failure does. React declares it inline with
/// no exported name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorHandlingStrategy {
    Abort,
    Skip,
    Fallback,
}

impl ErrorHandlingStrategy {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Abort => "abort",
            Self::Skip => "skip",
            Self::Fallback => "fallback",
        }
    }
}

/// React `ErrorHandlingConfig` — what a step's (or a definition's) failure
/// does; `fallback` steps run in place of the failed one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorHandlingConfig {
    pub strategy: ErrorHandlingStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_steps: Option<Vec<WorkflowStep>>,
}

/// React `WorkflowStep` — one step, in the flat shape the TS interface
/// declares (the kernel's tagged enum serializes into exactly this).
///
/// `type` is required; every other leg is plain `?:` (absent stays
/// absent). `if` is a TS/SQL keyword — the field is `r#if` with serde
/// keeping the wire key exactly `if`. The `thenSteps` / `elseSteps` /
/// `steps` legs recurse (a step holds sub-steps), which `Vec` breaks.
/// `input` / `add` are `Record<string, unknown>` → opaque [`Value`]
/// maps (`BTreeMap`, the `ai_types` name-keyed rule).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub r#type: WorkflowStepType,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<BTreeMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Step wall-time budget (seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<ErrorHandlingConfig>,
    /// The condition step's predicate (TS/SQL keyword `if`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#if: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then_steps: Option<Vec<WorkflowStep>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub else_steps: Option<Vec<WorkflowStep>>,
    /// The foreach step's list expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<String>,
    /// The foreach step's binding name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_var: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<WorkflowStep>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    /// The merge step's sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<MergeSource>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    /// The transform step's input variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_columns: Option<Vec<TransformColumn>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

/// React `MergeSource` — one input of a merge step.
///
/// `columns` is `Record<string, string>` (output → input column pairs) and
/// `add` is `Record<string, unknown>` (opaque [`Value`] literals) — both
/// name-keyed `BTreeMap`s, never order-observed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeSource {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add: Option<BTreeMap<String, Value>>,
}

/// React `TransformColumn` — one computed column a transform step adds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformColumn {
    pub name: String,
    pub expr: String,
}

/// React `WorkflowOutput` — how a definition's final output is rendered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutput {
    pub format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

/// React `WorkflowBlockType = 'sessionBlock' | 'transactionBlock'` — the
/// v2 resource-block kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowBlockType {
    SessionBlock,
    TransactionBlock,
}

impl WorkflowBlockType {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionBlock => "sessionBlock",
            Self::TransactionBlock => "transactionBlock",
        }
    }
}

/// React `WorkflowBlockRetry` — block-level retry. The unit is always the
/// whole block — a block never resumes from the middle — so there is
/// deliberately no per-step count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowBlockRetry {
    /// Which block (0-based index) the retry targets.
    pub block: u32,
}

/// React `WorkflowBlock` — a v2 resource block: its own steps and target,
/// level 2 of the step > block > workflow > profile chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowBlock {
    pub id: String,
    pub r#type: WorkflowBlockType,
    /// Block target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    /// Block target (multi-db connections).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Run this block concurrently with *other* blocks, never with itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel: Option<bool>,
    pub steps: Vec<WorkflowStep>,
    /// `transactionBlock` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<WorkflowBlockRetry>,
}

/// React `WorkflowSchedule.interval_secs`'s dual-key wire: the TS interface
/// declares both spellings (`interval_secs?` and `intervalSecs?`) because
/// the kernel serializes the snake key and legacy payloads carried the
/// camel one. The mirror is the kernel's own approach — one field, the
/// snake key on the wire, the camel key accepted on read.
///
/// (The family's camelCase `rename_all` does not apply here: the explicit
/// `rename` on this field wins, exactly the `websocket` precedent
/// `connection_types::TunnelKind` set.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSchedule {
    pub enabled: bool,
    /// Run every N seconds; values below 30 are clamped at runtime.
    #[serde(default, alias = "intervalSecs", rename = "interval_secs")]
    pub interval_secs: Option<u64>,
}

/// React `WorkflowDefinition.visibility`'s inline union `'user' |
/// 'dashboardHidden'` — hidden workflows are dashboard-owned SQL bindings.
/// React declares it inline with no exported name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowVisibility {
    User,
    DashboardHidden,
}

impl WorkflowVisibility {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::DashboardHidden => "dashboardHidden",
        }
    }
}

/// React `WorkflowDefinition` — a persisted workflow: its variables, its
/// default connection/database, its steps and v2 blocks, its output,
/// schedule and visibility.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub variables: Vec<WorkflowVariable>,
    /// Default connection inherited by data-operation steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    /// Default database inherited by data-operation steps (multi-db
    /// connections).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub steps: Vec<WorkflowStep>,
    /// v2 resource blocks; each block holds its own steps and target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<WorkflowBlock>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<WorkflowOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_handling: Option<ErrorHandlingConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<WorkflowSchedule>,
    /// `user` | `dashboardHidden`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<WorkflowVisibility>,
}

// ── Requests ────────────────────────────────────────────────────────────────

/// React `WorkflowIdRequest` — `getWorkflow` / `getWorkflowYaml` /
/// `deleteWorkflow`: which definition. The id field name is the desktop
/// IPC parameter bag's, kept verbatim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowIdRequest {
    pub workflow_id: String,
}

/// React `WorkflowYamlSaveRequest` — `saveWorkflowYaml`: the YAML text to
/// parse-and-persist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowYamlSaveRequest {
    pub yaml: String,
}

/// React `WorkflowSaveRequest` — `saveWorkflow`: the definition rides
/// inside the `{ workflow }` bag (the desktop IPC parameter shape, the
/// same `{ dashboard }` / `{ profile }` domain precedent).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSaveRequest {
    pub workflow: WorkflowDefinition,
}

/// React `WorkflowExecuteRequest` — `executeWorkflow`: one synchronous run
/// of a persisted definition (a long-running workflow past the transport
/// timeout is the caller's retry policy, not a job — §6.14).
///
/// `variables` is `Record<string, unknown>` → opaque [`Value`] map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecuteRequest {
    pub workflow_id: String,
    pub variables: BTreeMap<String, Value>,
    /// Overrides the definition's default connection for this run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
}

/// React `WorkflowRunsListRequest` — `listWorkflowRuns` /
/// `clearWorkflowRuns`: present = one workflow's runs, absent = the whole
/// recorded history. `null` and omission mean the same thing (the desktop
/// rail historically sent an explicit `null`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunsListRequest {
    /// `?: string | null`: null on the wire when set to `None`; absent
    /// also accepted.
    pub workflow_id: Option<String>,
}

/// React `WorkflowRunGetRequest` — `getWorkflowRun`: which recorded run.
/// The field name is the desktop IPC parameter bag's `historyId`, kept
/// verbatim — the contract renamed the noun (run, not history) but not the
/// parameter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunGetRequest {
    pub history_id: String,
}

// ── Recorded run history ────────────────────────────────────────────────────

/// React `WorkflowRunListItem` — one recorded workflow run, without the
/// payload. The frontend's pre-lift name was `HistoryListItem`; the
/// aggregate keeps that alias (see `types/mod.rs`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunListItem {
    pub id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub success: bool,
    pub total_time_ms: u64,
    /// When the run was recorded (the TS history domain's ISO string).
    pub created_at: String,
}

/// React `WorkflowRunRecord` — one recorded workflow run, with the
/// variables and the full result. The frontend's pre-lift name was
/// `HistoryEntry`; the aggregate keeps that alias (see `types/mod.rs`).
///
/// `variables` is `Record<string, unknown>` → opaque [`Value`] map; the
/// result is [`WorkflowExecutionResult`] — `ai_store_types`' mirror, the
/// single definition (this module re-exports it, never redefines).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunRecord {
    pub id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub variables: BTreeMap<String, Value>,
    pub result: WorkflowExecutionResult,
    /// When the run was recorded (the TS history domain's ISO string).
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn the_workflow_unions_keep_their_literals() {
        let step_types = [
            (WorkflowStepType::Query, "query"),
            (WorkflowStepType::Command, "command"),
            (WorkflowStepType::Ai, "ai"),
            (WorkflowStepType::Condition, "condition"),
            (WorkflowStepType::Foreach, "foreach"),
            (WorkflowStepType::Merge, "merge"),
            (WorkflowStepType::Transform, "transform"),
        ];
        for (value, literal) in step_types {
            assert_eq!(value.as_str(), literal);
            let parsed: WorkflowStepType = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let strategies = [
            (ErrorHandlingStrategy::Abort, "abort"),
            (ErrorHandlingStrategy::Skip, "skip"),
            (ErrorHandlingStrategy::Fallback, "fallback"),
        ];
        for (value, literal) in strategies {
            assert_eq!(value.as_str(), literal);
            let parsed: ErrorHandlingStrategy = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let block_types = [
            (WorkflowBlockType::SessionBlock, "sessionBlock"),
            (WorkflowBlockType::TransactionBlock, "transactionBlock"),
        ];
        for (value, literal) in block_types {
            assert_eq!(value.as_str(), literal);
            let parsed: WorkflowBlockType = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let visibilities = [
            (WorkflowVisibility::User, "user"),
            (WorkflowVisibility::DashboardHidden, "dashboardHidden"),
        ];
        for (value, literal) in visibilities {
            assert_eq!(value.as_str(), literal);
            let parsed: WorkflowVisibility = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn a_step_keeps_its_flat_keyword_wire() {
        let step = WorkflowStep {
            r#type: WorkflowStepType::Condition,
            id: "s1".into(),
            sql: None,
            connection: None,
            database: None,
            command: None,
            input: None,
            prompt: None,
            timeout_secs: Some(60),
            on_error: Some(ErrorHandlingConfig {
                strategy: ErrorHandlingStrategy::Skip,
                fallback_steps: None,
            }),
            r#if: Some("rows > 0".into()),
            then_steps: Some(vec![WorkflowStep {
                r#type: WorkflowStepType::Query,
                id: "s2".into(),
                sql: Some("SELECT 1".into()),
                connection: None,
                database: None,
                command: None,
                input: None,
                prompt: None,
                timeout_secs: None,
                on_error: None,
                r#if: None,
                then_steps: None,
                else_steps: None,
                items: None,
                as_var: None,
                steps: None,
                max_iterations: None,
                sources: None,
                columns: None,
                from: None,
                add_columns: None,
                filter: None,
                sort_by: None,
                offset: None,
                limit: Some(10),
            }]),
            else_steps: None,
            items: None,
            as_var: None,
            steps: None,
            max_iterations: None,
            sources: None,
            columns: None,
            from: None,
            add_columns: None,
            filter: None,
            sort_by: None,
            offset: None,
            limit: None,
        };
        let wire = encode(&step);
        // The TS/SQL keywords keep their exact wire keys.
        assert_eq!(wire["type"], "condition");
        assert_eq!(wire["if"], "rows > 0");
        assert_eq!(wire["thenSteps"][0]["limit"], 10);
        // Plain `?:` legs stay absent.
        assert!(wire.get("sql").is_none());
        assert!(wire.get("asVar").is_none());
        let back: WorkflowStep = decode(wire);
        assert_eq!(back, step);
    }

    #[test]
    fn the_schedule_reads_both_wire_spellings_and_reemits_the_snake_key() {
        let schedule = WorkflowSchedule { enabled: true, interval_secs: Some(3600) };
        let wire = encode(&schedule);
        assert_eq!(wire["interval_secs"], 3600);
        assert!(wire.get("intervalSecs").is_none());
        let back: WorkflowSchedule = decode(wire);
        assert_eq!(back, schedule);
        // The legacy camel key decodes into the same field.
        let legacy: WorkflowSchedule =
            decode(serde_json::json!({ "enabled": false, "intervalSecs": 30 }));
        assert_eq!(legacy.interval_secs, Some(30));
    }

    #[test]
    fn a_definition_round_trips_its_blocks_and_output() {
        let definition = WorkflowDefinition {
            id: "wf-1".into(),
            name: "Nightly rollup".into(),
            description: "Aggregates yesterday".into(),
            version: Some("2".into()),
            author: None,
            variables: vec![WorkflowVariable {
                name: "date".into(),
                r#type: "string".into(),
                description: "The run date".into(),
                required: Some(true),
                default: None,
            }],
            connection: Some("c1".into()),
            database: Some("app".into()),
            steps: vec![WorkflowStep {
                r#type: WorkflowStepType::Query,
                id: "q1".into(),
                sql: Some("SELECT now()".into()),
                connection: None,
                database: None,
                command: None,
                input: None,
                prompt: None,
                timeout_secs: None,
                on_error: None,
                r#if: None,
                then_steps: None,
                else_steps: None,
                items: None,
                as_var: None,
                steps: None,
                max_iterations: None,
                sources: None,
                columns: None,
                from: None,
                add_columns: None,
                filter: None,
                sort_by: None,
                offset: None,
                limit: None,
            }],
            blocks: Some(vec![WorkflowBlock {
                id: "b1".into(),
                r#type: WorkflowBlockType::TransactionBlock,
                connection: None,
                database: None,
                parallel: Some(true),
                steps: Vec::new(),
                retry: Some(WorkflowBlockRetry { block: 0 }),
            }]),
            output: Some(WorkflowOutput {
                format: "csv".into(),
                template: None,
            }),
            timeout_secs: Some(300),
            error_handling: None,
            schedule: Some(WorkflowSchedule { enabled: false, interval_secs: None }),
            visibility: Some(WorkflowVisibility::DashboardHidden),
        };
        let wire = encode(&definition);
        assert_eq!(wire["blocks"][0]["type"], "transactionBlock");
        assert_eq!(wire["blocks"][0]["retry"]["block"], 0);
        assert_eq!(wire["visibility"], "dashboardHidden");
        let back: WorkflowDefinition = decode(wire);
        assert_eq!(back, definition);
    }

    #[test]
    fn the_request_bags_keep_their_verbatim_field_names() {
        // `WorkflowSaveRequest` nests the definition under `workflow`.
        let save = WorkflowSaveRequest {
            workflow: WorkflowDefinition {
                id: "wf-2".into(),
                name: "Empty".into(),
                description: String::new(),
                version: None,
                author: None,
                variables: Vec::new(),
                connection: None,
                database: None,
                steps: Vec::new(),
                blocks: None,
                output: None,
                timeout_secs: None,
                error_handling: None,
                schedule: None,
                visibility: None,
            },
        };
        assert_eq!(encode(&save)["workflow"]["name"], "Empty");

        // `WorkflowRunGetRequest` keeps the desktop bag's `historyId`.
        let get = WorkflowRunGetRequest { history_id: "h-1".into() };
        assert_eq!(encode(&get)["historyId"], "h-1");

        // `WorkflowRunsListRequest`'s null/omission equivalence.
        let null_filter: WorkflowRunsListRequest = decode(serde_json::json!({ "workflowId": null }));
        assert_eq!(null_filter.workflow_id, None);
        let absent: WorkflowRunsListRequest = decode(serde_json::json!({}));
        assert_eq!(absent.workflow_id, None);
        let present = WorkflowRunsListRequest { workflow_id: Some("wf-1".into()) };
        assert_eq!(encode(&present)["workflowId"], "wf-1");
    }

    #[test]
    fn a_run_record_carries_the_store_result_shape() {
        let record = WorkflowRunRecord {
            id: "r-1".into(),
            workflow_id: "wf-1".into(),
            workflow_name: "Nightly rollup".into(),
            variables: {
                let mut map = BTreeMap::new();
                map.insert("date".into(), serde_json::json!("2026-11-20"));
                map
            },
            result: WorkflowExecutionResult {
                success: true,
                final_output: "ok".into(),
                steps: Vec::new(),
                total_time_ms: 42,
                error: None,
            },
            created_at: "2026-11-20T04:00:00Z".into(),
        };
        let wire = encode(&record);
        assert_eq!(wire["workflowName"], "Nightly rollup");
        assert_eq!(wire["result"]["finalOutput"], "ok");
        assert_eq!(wire["variables"]["date"], "2026-11-20");
        let back: WorkflowRunRecord = decode(wire);
        assert_eq!(back, record);

        // The list projection stays payload-free.
        let item = WorkflowRunListItem {
            id: "r-1".into(),
            workflow_id: "wf-1".into(),
            workflow_name: "Nightly rollup".into(),
            success: true,
            total_time_ms: 42,
            created_at: "2026-11-20T04:00:00Z".into(),
        };
        assert_eq!(encode(&item).as_object().map(|o| o.len()), Some(6));
    }
}
