//! Versioned, reusable Data Synchronization configuration.
//!
//! Profiles intentionally contain only stable configuration. Runtime database
//! sessions, credentials, comparison rows and server-owned plan ids never
//! cross this persistence boundary.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{SyncOptions, TableMapping};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncProfile {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub source_connection_id: String,
    pub target_connection_id: String,
    #[serde(default)]
    pub source_database: Option<String>,
    #[serde(default)]
    pub target_database: Option<String>,
    #[serde(default)]
    pub source_schema: Option<String>,
    #[serde(default)]
    pub target_schema: Option<String>,
    /// The selected table mappings, including disabled mappings and their
    /// structured source filters/recordsets.
    pub tables: Vec<TableMapping>,
    pub options: SyncOptions,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SyncProfile {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn validate(&self) -> Result<(), String> {
        if self.version != Self::CURRENT_VERSION {
            return Err(format!("unsupported sync profile version {}", self.version));
        }
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err("sync profile id and name are required".into());
        }
        if self.source_connection_id.trim().is_empty()
            || self.target_connection_id.trim().is_empty()
        {
            return Err(
                "sync profile sourceConnectionId and targetConnectionId are required".into(),
            );
        }
        for mapping in &self.tables {
            if mapping.source_table.trim().is_empty() || mapping.target_table.trim().is_empty() {
                return Err(
                    "sync profile table mappings require sourceTable and targetTable".into(),
                );
            }
        }
        self.options.validate().map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn profile() -> SyncProfile {
        SyncProfile {
            version: SyncProfile::CURRENT_VERSION,
            id: "sync-profile-1".into(),
            name: "Nightly users".into(),
            source_connection_id: "source".into(),
            target_connection_id: "target".into(),
            source_database: Some("app".into()),
            target_database: Some("app".into()),
            source_schema: Some("public".into()),
            target_schema: Some("public".into()),
            tables: vec![TableMapping::auto("users")],
            options: SyncOptions::default(),
            created_at: Utc.timestamp_opt(0, 0).single().expect("epoch"),
            updated_at: Utc.timestamp_opt(0, 0).single().expect("epoch"),
        }
    }

    #[test]
    fn profile_round_trip_contains_only_stable_configuration() {
        let value = serde_json::to_value(profile()).expect("serialize");
        assert!(value.get("sourceDbSessionId").is_none());
        assert!(value.get("targetDbSessionId").is_none());
        assert!(value.get("planId").is_none());
        assert!(value.get("credentials").is_none());
        assert_eq!(value["tables"][0]["enabled"], true);
        let decoded: SyncProfile = serde_json::from_value(value).expect("deserialize");
        assert_eq!(decoded, profile());
    }

    #[test]
    fn profile_round_trip_preserves_legacy_scalar_and_new_tuple_recordsets() {
        let legacy_filter = serde_json::json!({
            "filters": [],
            "recordset": {
                "orderBy": "id",
                "start": {"value": "10", "inclusive": false},
                "limit": 20
            }
        });
        let tuple_filter = serde_json::json!({
            "filters": [],
            "recordset": {
                "tupleRange": {
                    "columns": ["tenant_id", "id"],
                    "start": {"values": ["9223372036854775808", "01"], "inclusive": false},
                    "end": {"values": ["9223372036854775808", "99"], "inclusive": true}
                }
            }
        });
        let mut value = serde_json::to_value(profile()).expect("serialize base profile");
        value["tables"][0]["sourceFilter"] = legacy_filter.clone();
        let legacy: SyncProfile = serde_json::from_value(value.clone()).expect("legacy profile");
        assert_eq!(legacy.version, SyncProfile::CURRENT_VERSION);
        assert_eq!(serde_json::to_value(&legacy).unwrap(), value);

        value["tables"][0]["sourceFilter"] = tuple_filter.clone();
        let tuple: SyncProfile = serde_json::from_value(value.clone()).expect("tuple profile");
        assert_eq!(tuple.version, SyncProfile::CURRENT_VERSION);
        assert_eq!(
            tuple.tables[0].source_filter.as_ref().unwrap().0,
            tuple_filter
        );
        assert_eq!(serde_json::to_value(tuple).unwrap(), value);
    }

    #[test]
    fn profile_rejects_unknown_fields_and_versions() {
        let mut value = serde_json::to_value(profile()).expect("serialize");
        value["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<SyncProfile>(value).is_err());

        let mut old = profile();
        old.version = 99;
        assert!(old.validate().is_err());
    }
}
