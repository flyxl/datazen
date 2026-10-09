//! Persisted, reusable Schema Diff setup.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::types::ColumnTypeOverride;
use datazen_driver_api::schema_objects::DatabaseObject;

/// A reusable Schema Diff setup. Runtime sessions, generated plans and DDL are
/// intentionally absent; the current connection configuration is resolved
/// when a profile is loaded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaDiffProfile {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub source_connection_id: String,
    pub target_connection_id: String,
    pub source_database: String,
    pub target_database: String,
    #[serde(default)]
    pub source_schema: Option<String>,
    #[serde(default)]
    pub target_schema: Option<String>,
    #[serde(default)]
    pub target_only_tables: Vec<String>,
    pub tables: Vec<String>,
    #[serde(default)]
    pub source_objects: Vec<DatabaseObject>,
    #[serde(default)]
    pub target_objects: Vec<DatabaseObject>,
    pub allow_destructive: bool,
    pub include_indexes: bool,
    pub require_rollback: bool,
    #[serde(default)]
    pub type_overrides: Vec<ColumnTypeOverride>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SchemaDiffProfile {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn validate(&self) -> Result<(), String> {
        if self.version != Self::CURRENT_VERSION {
            return Err(format!(
                "unsupported schema diff profile version {}",
                self.version
            ));
        }
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err("schema diff profile id and name are required".into());
        }
        if self.source_connection_id.trim().is_empty()
            || self.target_connection_id.trim().is_empty()
        {
            return Err(
                "schema diff profile sourceConnectionId and targetConnectionId are required".into(),
            );
        }
        if self.source_database.trim().is_empty() || self.target_database.trim().is_empty() {
            return Err(
                "schema diff profile sourceDatabase and targetDatabase are required".into(),
            );
        }
        if self.tables.is_empty()
            && self.target_only_tables.is_empty()
            && self.source_objects.is_empty()
            && self.target_objects.is_empty()
        {
            return Err("schema diff profile requires at least one table or schema object".into());
        }
        let mut selected_tables = HashSet::new();
        for table in self.tables.iter().chain(self.target_only_tables.iter()) {
            if table.trim().is_empty() {
                return Err("schema diff profile table names must not be empty".into());
            }
            if !selected_tables.insert(table) {
                return Err(
                    "schema diff profile table selections must not contain duplicates".into(),
                );
            }
        }
        for (label, objects) in [
            ("source", &self.source_objects),
            ("target", &self.target_objects),
        ] {
            let mut identities = HashSet::new();
            for object in objects {
                if !matches!(
                    object.kind.as_str(),
                    "view" | "type" | "sequence" | "function" | "procedure" | "trigger"
                ) || object.name.trim().is_empty()
                {
                    return Err(format!(
                        "schema diff profile {label} schema object kind and name are required"
                    ));
                }
                if matches!(object.kind.as_str(), "function" | "procedure")
                    && object.signature.as_deref().is_none_or(|signature| {
                        !signature.is_empty() && signature.trim().is_empty()
                    })
                {
                    return Err(format!(
                        "schema diff profile {label} routine identity requires a signature"
                    ));
                }
                if object.kind == "trigger" && object.target_name.is_none() {
                    return Err(format!(
                        "schema diff profile {label} trigger identity requires a target name"
                    ));
                }
                if object
                    .schema
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty())
                    || object
                        .target_schema
                        .as_ref()
                        .is_some_and(|value| value.trim().is_empty())
                    || object
                        .target_name
                        .as_ref()
                        .is_some_and(|value| value.trim().is_empty())
                {
                    return Err(format!(
                        "schema diff profile {label} schema object identity parts must not be empty"
                    ));
                }
                let identity = (
                    object.kind.as_str(),
                    object.schema.as_deref(),
                    object.name.as_str(),
                    object.signature.as_deref(),
                    object.target_schema.as_deref(),
                    object.target_name.as_deref(),
                );
                if !identities.insert(identity) {
                    return Err(format!(
                        "schema diff profile {label} schema object selections must not contain duplicates"
                    ));
                }
            }
        }
        if self.type_overrides.iter().any(|override_| {
            override_.table.trim().is_empty()
                || override_.column.trim().is_empty()
                || override_.target_type.trim().is_empty()
        }) {
            return Err("schema diff profile type overrides must be complete".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> SchemaDiffProfile {
        let now = Utc::now();
        SchemaDiffProfile {
            version: SchemaDiffProfile::CURRENT_VERSION,
            id: "profile-1".into(),
            name: "Production schema".into(),
            source_connection_id: "source".into(),
            target_connection_id: "target".into(),
            source_database: "app".into(),
            target_database: "app".into(),
            source_schema: Some("public".into()),
            target_schema: Some("public".into()),
            target_only_tables: vec![],
            tables: vec!["public.users".into()],
            source_objects: vec![],
            target_objects: vec![],
            allow_destructive: false,
            include_indexes: true,
            require_rollback: false,
            type_overrides: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn rejects_unknown_fields_and_runtime_artifacts() {
        let mut value = serde_json::to_value(profile()).expect("profile serializes");
        value["dbSessionId"] = serde_json::json!("runtime-session");
        value["plan"] = serde_json::json!({"statements": []});
        assert!(serde_json::from_value::<SchemaDiffProfile>(value).is_err());

        let encoded = serde_json::to_string(&profile()).expect("profile serializes");
        assert!(!encoded.contains("dbSessionId"));
        assert!(!encoded.contains("plan"));
        assert!(!encoded.contains("ddl"));
        assert!(!encoded.contains("password"));
    }

    #[test]
    fn validates_version_scope_and_table_selection() {
        let mut value = profile();
        value.version = 2;
        assert!(value.validate().is_err());
        value = profile();
        value.tables.clear();
        assert!(value.validate().is_err());
        value = profile();
        value.source_database.clear();
        assert!(value.validate().is_err());
        assert!(profile().validate().is_ok());
    }

    #[test]
    fn accepts_target_only_tables_without_source_tables() {
        let mut value = profile();
        value.tables.clear();
        value.target_only_tables = vec!["public.archive".into()];
        assert!(value.validate().is_ok());
    }

    fn object(
        kind: &str,
        schema: Option<&str>,
        name: &str,
        signature: Option<&str>,
        target_schema: Option<&str>,
        target_name: Option<&str>,
    ) -> DatabaseObject {
        DatabaseObject {
            kind: kind.into(),
            schema: schema.map(str::to_owned),
            name: name.into(),
            signature: signature.map(str::to_owned),
            target_schema: target_schema.map(str::to_owned),
            target_name: target_name.map(str::to_owned),
        }
    }

    #[test]
    fn accepts_object_only_profiles_and_rejects_duplicate_or_invalid_identities() {
        let mut value = profile();
        value.tables.clear();
        value.source_objects = vec![object(
            "function",
            Some("public"),
            "calculate_total",
            Some("integer, numeric"),
            None,
            None,
        )];
        assert!(value.validate().is_ok());

        value.source_objects.push(value.source_objects[0].clone());
        assert!(value.validate().is_err());

        value.source_objects = vec![object("table", Some("public"), "users", None, None, None)];
        assert!(value.validate().is_err());
        value.source_objects = vec![object("view", Some("public"), "  ", None, None, None)];
        assert!(value.validate().is_err());

        value.source_objects = vec![object("function", Some("public"), "f", None, None, None)];
        assert!(value.validate().is_err());
        value.source_objects = vec![object(
            "function",
            Some("public"),
            "f",
            Some("   "),
            None,
            None,
        )];
        assert!(value.validate().is_err());
        value.source_objects = vec![object("trigger", Some("public"), "trg", None, None, None)];
        assert!(value.validate().is_err());
    }

    #[test]
    fn defaults_object_selections_for_older_profiles() {
        let mut value = serde_json::to_value(profile()).expect("profile serializes");
        value
            .as_object_mut()
            .expect("profile is an object")
            .remove("sourceObjects");
        value
            .as_object_mut()
            .expect("profile is an object")
            .remove("targetObjects");

        let decoded = serde_json::from_value::<SchemaDiffProfile>(value)
            .expect("legacy profile without object selections loads");
        assert!(decoded.source_objects.is_empty());
        assert!(decoded.target_objects.is_empty());
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn round_trips_routine_signatures_and_trigger_targets() {
        let mut value = profile();
        value.source_objects = vec![object(
            "function",
            Some("public"),
            "calculate_total",
            Some("integer, numeric"),
            None,
            None,
        )];
        value.target_objects = vec![object(
            "trigger",
            Some("public"),
            "audit_orders",
            None,
            Some("public"),
            Some("orders"),
        )];

        let decoded = serde_json::from_str::<SchemaDiffProfile>(
            &serde_json::to_string(&value).expect("profile serializes"),
        )
        .expect("profile deserializes");
        assert_eq!(decoded, value);
        assert!(decoded.validate().is_ok());
    }
}
