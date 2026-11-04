//! Structure editor types (drafts, plans, risk).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StructureChangeMode {
    Create,
    Alter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StructureColumnDraft {
    pub id: String,
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default)]
    pub is_primary_key: bool,
    #[serde(default)]
    pub is_auto_increment: bool,
    #[serde(default)]
    pub is_unique: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StructureIndexDraft {
    pub id: String,
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    #[serde(default)]
    pub is_primary: bool,
    #[serde(default)]
    pub index_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StructureChangeRequest {
    pub mode: StructureChangeMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub table: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub original_columns: Vec<StructureColumnDraft>,
    pub current_columns: Vec<StructureColumnDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub original_indexes: Vec<StructureIndexDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub current_indexes: Vec<StructureIndexDraft>,
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
    pub summary: String,
    pub risk: StatementRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct StructureChangePlan {
    pub statements: Vec<PlanStatement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}
