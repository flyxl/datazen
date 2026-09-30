//! Structured catalog identities shared by metadata transports.
use crate::{ColumnSchema, TableSchema, TableType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelationRef {
    pub database: String,
    pub schema: Option<String>,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationSummary {
    #[serde(rename = "ref")]
    pub relation: RelationRef,
    pub kind: TableType,
    pub row_count: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationColumns {
    #[serde(rename = "ref")]
    pub relation: RelationRef,
    pub columns: Vec<ColumnSchema>,
    pub primary_keys: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationSchema {
    #[serde(rename = "ref")]
    pub relation: RelationRef,
    pub definition: TableSchema,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCatalogOutput {
    pub database: String,
    pub schemas: Vec<String>,
    pub relations: Vec<RelationSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetadataReadErrorCode {
    Unsupported,
    ReadFailed,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetadataReadError {
    pub code: MetadataReadErrorCode,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ColumnsReadResult {
    Ok {
        value: RelationColumns,
    },
    Error {
        #[serde(rename = "ref")]
        relation: RelationRef,
        error: MetadataReadError,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadColumnsOutput {
    pub results: Vec<ColumnsReadResult>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum MetadataRefreshScope {
    Session,
    Database { database: String },
    Relation { relation: RelationRef },
}
