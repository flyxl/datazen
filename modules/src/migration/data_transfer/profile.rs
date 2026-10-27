use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::model::{
    SqlFileCompression, SqlFileEncoding, TableMapping, TransferMode, TransferOptions, WriteMode,
};

/// Persisted, reusable Data Transfer configuration.
///
/// Runtime dbSessionIds and native SQL-file path tokens are deliberately not
/// part of this model. A profile is resolved against current connection
/// configurations when it is loaded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferProfile {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub source_connection_id: String,
    #[serde(default)]
    pub target_connection_id: Option<String>,
    #[serde(default)]
    pub source_database: Option<String>,
    #[serde(default)]
    pub target_database: Option<String>,
    #[serde(default)]
    pub source_schema: Option<String>,
    #[serde(default)]
    pub target_schema: Option<String>,
    pub destination_mode: String,
    #[serde(default)]
    pub sql_file_dialect: Option<String>,
    #[serde(default)]
    pub sql_file_encoding: Option<String>,
    #[serde(default)]
    pub sql_file_compression: Option<String>,
    #[serde(default)]
    pub sql_file_database: Option<String>,
    #[serde(default)]
    pub sql_file_schema: Option<String>,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub tables: Vec<TableMapping>,
    pub options: TransferOptions,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TransferProfile {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn validate(&self) -> Result<(), String> {
        if self.version != Self::CURRENT_VERSION {
            return Err(format!(
                "unsupported transfer profile version {}",
                self.version
            ));
        }
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err("transfer profile id and name are required".into());
        }
        if self.source_connection_id.trim().is_empty() {
            return Err("transfer profile sourceConnectionId is required".into());
        }
        match self.destination_mode.as_str() {
            "database" => {
                if self
                    .target_connection_id
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                {
                    return Err("database transfer profiles require targetConnectionId".into());
                }
            }
            "sqlFile" => {}
            other => {
                return Err(format!(
                    "unknown transfer profile destinationMode '{other}'"
                ))
            }
        }
        self.options.validate().map_err(|error| error.to_string())?;
        if let Some(value) = self.sql_file_encoding.as_deref() {
            SqlFileEncoding::parse_profile(value).map_err(|error| error.to_string())?;
        }
        if let Some(value) = self.sql_file_compression.as_deref() {
            SqlFileCompression::parse_profile(value).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
