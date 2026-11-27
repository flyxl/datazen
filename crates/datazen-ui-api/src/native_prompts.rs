//! Local prompt-settings data; no provider calls or credentials.
use serde::{Deserialize, Serialize};

/// React `PromptScenario`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptScenario {
    #[serde(rename = "nl2sql")]
    Nl2sql,
    #[serde(rename = "diagnose")]
    Diagnose,
    #[serde(rename = "nl_filter")]
    NlFilter,
    #[serde(rename = "schema_doc_select_tables")]
    SchemaDocSelectTables,
    #[serde(rename = "schema_doc")]
    SchemaDoc,
    #[serde(rename = "connection_diagnose")]
    ConnectionDiagnose,
    #[serde(rename = "query_summary")]
    QuerySummary,
    #[serde(rename = "explain_analysis")]
    ExplainAnalysis,
    #[serde(rename = "chat")]
    Chat,
    #[serde(rename = "workflow_generate")]
    WorkflowGenerate,
}

/// React `PromptSource`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptSource {
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "driver")]
    Driver,
    #[serde(rename = "user")]
    User,
}

/// React `PromptInfo`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PromptInfo {
    pub scenario: PromptScenario,
    pub label: String,
    pub source: PromptSource,
    pub system: String,
    #[serde(rename = "defaultSystem")]
    pub default_system: String,
}

/// React `PromptOverrideEntry`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PromptOverrideEntry {
    #[serde(rename = "driverType")]
    pub driver_type: String,
    pub scenario: PromptScenario,
    pub system: String,
}

/// An explicit request to the local prompts worker. The driver is the exact
/// selected native registry key, or `*` for the global setting.
#[derive(Clone, Debug, PartialEq)]
pub struct NativePromptRequest {
    pub driver: String,
    pub change: NativePromptChange,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativePromptChange {
    List,
    Save { scenario: PromptScenario, system: String },
    Reset { scenario: PromptScenario },
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePromptView {
    pub prompts: Vec<PromptInfo>,
    /// Some user-source previews are inherited from the global override.
    /// Only the override in the currently selected scope may be reset.
    pub resettable: Vec<PromptScenario>,
}
