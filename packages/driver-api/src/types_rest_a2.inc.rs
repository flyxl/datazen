#[derive(Debug, Clone)]
pub struct ConnectionHandle {
    pub id: String,
    pub pool_id: String,
}

/// Opaque identity for one query execution.
///
/// Drivers may associate this token with private backend state (for example a
/// PostgreSQL backend PID or a MySQL thread id), but that state must never be
/// put in the token or exposed to the host/UI. The host creates one fresh id
/// for every streamed execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QueryExecutionId(String);

impl QueryExecutionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub struct TransactionHandle {
    pub id: String,
    pub connection_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub server_version: String,
    pub server_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableType {
    Table,
    View,
    MaterializedView,
    SystemTable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub schema: Option<String>,
    pub table_type: TableType,
    pub row_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSchema {
    pub table_name: String,
    pub columns: Vec<ColumnSchema>,
    pub primary_keys: Vec<String>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
}

impl TableSchema {
    /// Effective primary key columns. Prefers `primary_keys` field;
    /// falls back to columns marked `is_primary_key`.
    pub fn effective_primary_keys(&self) -> Vec<String> {
        if !self.primary_keys.is_empty() {
            return self.primary_keys.clone();
        }
        self.columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSchema {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub index_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub on_update: String,
    pub on_delete: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExplainPlanDetail {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExplainPlanNode {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<i64>,
    #[serde(default)]
    pub details: Vec<ExplainPlanDetail>,
    #[serde(default)]
    pub children: Vec<ExplainPlanNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainResult {
    pub plan_text: String,
    pub plan_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_tree: Option<ExplainPlanNode>,
    pub total_cost: Option<f64>,
    pub estimated_rows: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataResult {
    pub columns: Vec<ColumnSchema>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub total_rows: Option<i64>,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyEntry {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
    pub size: u64,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDetail {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
    pub value: serde_json::Value,
}

/// Identifies an AI prompt scenario.
///
/// Each scenario has a default system prompt template built into the main app.
/// Drivers can override per-scenario prompts via [`DatabaseDriver::prompt_overrides`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptScenario {
    #[serde(rename = "nl2sql")]
    Nl2Sql,
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
    Chat,
    #[serde(rename = "workflow_generate")]
    WorkflowGenerate,
}

impl PromptScenario {
    pub fn all() -> &'static [PromptScenario] {
        &[
            Self::Nl2Sql,
            Self::Diagnose,
            Self::NlFilter,
            Self::SchemaDocSelectTables,
            Self::SchemaDoc,
            Self::ConnectionDiagnose,
            Self::QuerySummary,
            Self::ExplainAnalysis,
            Self::Chat,
            Self::WorkflowGenerate,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Nl2Sql => "NL → SQL",
            Self::Diagnose => "SQL Error Diagnosis",
            Self::NlFilter => "NL Filter",
            Self::SchemaDocSelectTables => "Schema Doc (Table Selection)",
            Self::SchemaDoc => "Schema Documentation",
            Self::ConnectionDiagnose => "Connection Diagnosis",
            Self::QuerySummary => "Query Summary",
            Self::ExplainAnalysis => "EXPLAIN Analysis",
            Self::Chat => "AI Chat",
            Self::WorkflowGenerate => "Workflow Generate",
        }
    }
}

impl std::fmt::Display for PromptScenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{:?}", self));
        f.write_str(&s)
    }
}

/// A prompt template for a specific scenario (written in English).
///
/// Templates can contain `{{variable}}` placeholders that get substituted at
/// runtime. Available variables depend on the scenario.
