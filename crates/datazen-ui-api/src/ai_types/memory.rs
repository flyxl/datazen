//! The AI memory vocabulary — React `types/ai.ts` lines 67–76 (settings)
//! and 285–444 (references, entries, pages, stats, and the create/update/
//! list inputs).
//!
//! The memory backend owns these rows; the UI consumes them through the
//! `commands/ai.ts` memory surface (its port task). Every enum here is a
//! closed string union; `AiMemoryEntry`'s scope/kind/status/sensitivity/
//! source fields are the enum columns, while `AiMemoryReference.kind` /
//! `.scope` stay plain strings — React declares them `string` there, so
//! the citation metadata does not narrow them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// React `AiMemoryCloudPolicy = 'normal_only' | 'allow_internal' |
/// 'disable_cloud'` — which memories may leave the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMemoryCloudPolicy {
    NormalOnly,
    AllowInternal,
    DisableCloud,
}

impl AiMemoryCloudPolicy {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NormalOnly => "normal_only",
            Self::AllowInternal => "allow_internal",
            Self::DisableCloud => "disable_cloud",
        }
    }
}

impl Default for AiMemoryCloudPolicy {
    /// React's `defaultAiMemorySettings.cloudPolicy` is `normal_only`.
    fn default() -> Self {
        Self::NormalOnly
    }
}

/// React `AiMemorySettings` — the opt-in switch panel. All four fields are
/// required (a settings host always materializes the whole object).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemorySettings {
    /// Opt-in switch. Older configs omit this and therefore stay disabled.
    pub enabled: bool,
    pub auto_learn: bool,
    pub cloud_policy: AiMemoryCloudPolicy,
    /// True after the first-use privacy explanation was acknowledged.
    pub privacy_acknowledged: bool,
}

/// React `defaultAiMemorySettings` — the exact literal object.
pub const DEFAULT_AI_MEMORY_SETTINGS: AiMemorySettings = AiMemorySettings {
    enabled: false,
    auto_learn: false,
    cloud_policy: AiMemoryCloudPolicy::NormalOnly,
    privacy_acknowledged: false,
};

/// React `AiMemoryReference` — non-secret citation metadata for one memory
/// injected into a chat run. `kind`/`scope` are plain strings on the React
/// interface (the backend's vocabulary, not narrowed by the UI contract).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryReference {
    /// Stable answer reference (`M1`, `M2`, …).
    pub reference: String,
    pub id: String,
    pub subject: String,
    pub kind: String,
    pub scope: String,
    pub score: f64,
}

/// React `AiMemoryScopeKind` — the scope column an entry is keyed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMemoryScopeKind {
    Org,
    Organization,
    User,
    Connection,
    Database,
    Schema,
    Table,
    Column,
}

impl AiMemoryScopeKind {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Org => "org",
            Self::Organization => "organization",
            Self::User => "user",
            Self::Connection => "connection",
            Self::Database => "database",
            Self::Schema => "schema",
            Self::Table => "table",
            Self::Column => "column",
        }
    }
}

/// React `AiMemoryKind` — what the entry remembers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMemoryKind {
    Preference,
    Glossary,
    Relationship,
    Pattern,
    Correction,
    Environment,
    Negative,
    SchemaNote,
}

impl AiMemoryKind {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::Glossary => "glossary",
            Self::Relationship => "relationship",
            Self::Pattern => "pattern",
            Self::Correction => "correction",
            Self::Environment => "environment",
            Self::Negative => "negative",
            Self::SchemaNote => "schema_note",
        }
    }
}

/// React `AiMemoryStatus` — the entry lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMemoryStatus {
    Candidate,
    Active,
    Rejected,
    Archived,
    Stale,
}

impl AiMemoryStatus {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Active => "active",
            Self::Rejected => "rejected",
            Self::Archived => "archived",
            Self::Stale => "stale",
        }
    }
}

/// React `AiMemorySensitivity` — who may see the entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMemorySensitivity {
    Normal,
    Internal,
    Secret,
}

impl AiMemorySensitivity {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Internal => "internal",
            Self::Secret => "secret",
        }
    }
}

/// React `AiMemorySource` — how the entry came to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMemorySource {
    Explicit,
    Correction,
    Usage,
    Feedback,
    SchemaNote,
    Import,
    Org,
}

impl AiMemorySource {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Correction => "correction",
            Self::Usage => "usage",
            Self::Feedback => "feedback",
            Self::SchemaNote => "schema_note",
            Self::Import => "import",
            Self::Org => "org",
        }
    }
}

/// React `AiMemoryEntry` — one memory row, as the memory panel and the
/// backend exchange it.
///
/// The scope address fields (`connectionId` … `columnName`) and the
/// timestamp fields are **required-but-nullable** (`string | null` without
/// `?`): they serialize as an explicit `null` when `None` — no
/// `skip_serializing_if`, so the keys stay present on the wire. On the
/// read side `None` covers both spellings (serde's implicit Option default
/// also accepts a missing key; the declared contract says the key is
/// always there, so the tolerance only widens what loads). `conflict` is
/// the one optional field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryEntry {
    pub id: String,
    pub scope_type: AiMemoryScopeKind,
    pub scope_id: String,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub schema_name: Option<String>,
    pub table_name: Option<String>,
    pub column_name: Option<String>,
    pub kind: AiMemoryKind,
    pub subject: String,
    pub content: String,
    /// `Record<string, unknown> | null` — opaque structured payload.
    pub structured_json: Option<Value>,
    pub source: AiMemorySource,
    pub source_ref: Option<String>,
    pub confidence: f64,
    pub sensitivity: AiMemorySensitivity,
    pub status: AiMemoryStatus,
    pub pinned: bool,
    /// True when a learned candidate collides with an active slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<bool>,
    pub use_count: u64,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// React `AiMemoryPage` — one listing page of [`AiMemoryEntry`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryPage {
    pub entries: Vec<AiMemoryEntry>,
    pub total: u64,
    pub limit: u64,
    pub offset: u64,
}

/// React `AiMemoryStats` — aggregate counts for the memory panel.
/// The `Record<string, number>` maps become `BTreeMap`: JS object key
/// order is insertion order, but these are count lookups keyed by status/
/// kind/scope name, so sorted iteration is inert on the wire semantic.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryStats {
    pub total: u64,
    pub by_status: BTreeMap<String, u64>,
    pub by_kind: BTreeMap<String, u64>,
    pub by_scope: BTreeMap<String, u64>,
}

/// React `AiMemoryCreateInput` — the create form's payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryCreateInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub scope_type: AiMemoryScopeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_name: Option<String>,
    pub kind: AiMemoryKind,
    pub subject: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<AiMemorySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<AiMemorySensitivity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AiMemoryStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// React `AiMemoryCorrectionInput` — one SQL correction to learn from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryCorrectionInput {
    pub original_sql: String,
    pub new_sql: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

/// React `AiMemorySqlSample` — one sampled statement for extraction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemorySqlSample {
    pub sql: String,
    pub observed_at: String,
}

/// React `AiMemoryExtractionInput` — the batch extraction request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryExtractionInput {
    pub sql: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<AiMemorySqlSample>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_days: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_occurrences: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_output_json: Option<String>,
}

/// React `AiMemoryStaleInput` — which scope subtree to mark stale.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryStaleInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
}

/// React `AiMemoryUpdateInput` — a partial update; every field optional.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryUpdateInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<AiMemoryKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<AiMemorySensitivity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AiMemoryStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// React `AiMemoryListParams` — the listing filter; every field optional.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_type: Option<AiMemoryScopeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AiMemoryStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<AiMemoryKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    #[test]
    fn memory_scope_kinds_keep_the_react_literals() {
        let table = [
            (AiMemoryScopeKind::Org, "org"),
            (AiMemoryScopeKind::Organization, "organization"),
            (AiMemoryScopeKind::User, "user"),
            (AiMemoryScopeKind::Connection, "connection"),
            (AiMemoryScopeKind::Database, "database"),
            (AiMemoryScopeKind::Schema, "schema"),
            (AiMemoryScopeKind::Table, "table"),
            (AiMemoryScopeKind::Column, "column"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AiMemoryScopeKind = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
    }

    #[test]
    fn memory_kinds_keep_the_react_literals() {
        let table = [
            (AiMemoryKind::Preference, "preference"),
            (AiMemoryKind::Glossary, "glossary"),
            (AiMemoryKind::Relationship, "relationship"),
            (AiMemoryKind::Pattern, "pattern"),
            (AiMemoryKind::Correction, "correction"),
            (AiMemoryKind::Environment, "environment"),
            (AiMemoryKind::Negative, "negative"),
            (AiMemoryKind::SchemaNote, "schema_note"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AiMemoryKind = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
    }

    #[test]
    fn memory_statuses_keep_the_react_literals() {
        let table = [
            (AiMemoryStatus::Candidate, "candidate"),
            (AiMemoryStatus::Active, "active"),
            (AiMemoryStatus::Rejected, "rejected"),
            (AiMemoryStatus::Archived, "archived"),
            (AiMemoryStatus::Stale, "stale"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AiMemoryStatus = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
    }

    #[test]
    fn memory_sensitivities_and_sources_keep_the_react_literals() {
        let sensitivities = [
            (AiMemorySensitivity::Normal, "normal"),
            (AiMemorySensitivity::Internal, "internal"),
            (AiMemorySensitivity::Secret, "secret"),
        ];
        for (value, literal) in sensitivities {
            assert_eq!(value.as_str(), literal);
            let parsed: AiMemorySensitivity = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
        }
        let sources = [
            (AiMemorySource::Explicit, "explicit"),
            (AiMemorySource::Correction, "correction"),
            (AiMemorySource::Usage, "usage"),
            (AiMemorySource::Feedback, "feedback"),
            (AiMemorySource::SchemaNote, "schema_note"),
            (AiMemorySource::Import, "import"),
            (AiMemorySource::Org, "org"),
        ];
        for (value, literal) in sources {
            assert_eq!(value.as_str(), literal);
            let parsed: AiMemorySource = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
        }
    }

    #[test]
    fn cloud_policies_keep_the_react_literals_and_default() {
        let table = [
            (AiMemoryCloudPolicy::NormalOnly, "normal_only"),
            (AiMemoryCloudPolicy::AllowInternal, "allow_internal"),
            (AiMemoryCloudPolicy::DisableCloud, "disable_cloud"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AiMemoryCloudPolicy = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
        }
        // defaultAiMemorySettings pins cloudPolicy: 'normal_only'.
        assert_eq!(AiMemoryCloudPolicy::default(), AiMemoryCloudPolicy::NormalOnly);
        assert_eq!(DEFAULT_AI_MEMORY_SETTINGS.enabled, false);
        assert_eq!(DEFAULT_AI_MEMORY_SETTINGS.auto_learn, false);
        assert_eq!(DEFAULT_AI_MEMORY_SETTINGS.privacy_acknowledged, false);
        assert_eq!(
            serde_json::to_value(DEFAULT_AI_MEMORY_SETTINGS.clone()).ok(),
            Some(serde_json::json!({
                "enabled": false,
                "autoLearn": false,
                "cloudPolicy": "normal_only",
                "privacyAcknowledged": false
            }))
        );
        assert_eq!(AiMemorySettings::default(), DEFAULT_AI_MEMORY_SETTINGS);
    }

    /// The entry round-trips with required-but-nullable keys kept present
    /// (explicit `null` in, explicit `null` out) and the one optional key
    /// (`conflict`) omitted.
    #[test]
    fn a_memory_entry_round_trips_its_null_shape_exactly() {
        let wire = serde_json::json!({
            "id": "mem-1",
            "scopeType": "user",
            "scopeId": "u-1",
            "connectionId": null,
            "databaseName": "shop",
            "schemaName": null,
            "tableName": "orders",
            "columnName": null,
            "kind": "preference",
            "subject": "prefer lowercase identifiers",
            "content": "Always writes lowercase.",
            "structuredJson": null,
            "source": "explicit",
            "sourceRef": "note-7",
            "confidence": 0.9,
            "sensitivity": "normal",
            "status": "active",
            "pinned": true,
            "useCount": 3,
            "lastUsedAt": null,
            "expiresAt": "2027-01-01T00:00:00Z",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-02T00:00:00Z"
        });
        let entry: AiMemoryEntry = decode(wire.clone());
        assert_eq!(entry.scope_type, AiMemoryScopeKind::User);
        assert_eq!(entry.kind, AiMemoryKind::Preference);
        assert_eq!(entry.source, AiMemorySource::Explicit);
        assert_eq!(entry.status, AiMemoryStatus::Active);
        assert_eq!(entry.sensitivity, AiMemorySensitivity::Normal);
        assert_eq!(entry.connection_id, None);
        assert_eq!(entry.database_name.as_deref(), Some("shop"));
        assert_eq!(entry.structured_json, None);
        assert_eq!(entry.conflict, None);
        assert_eq!(entry.use_count, 3);
        assert_eq!(serde_json::to_value(&entry).ok(), Some(wire));

        // Read-side tolerance: serde's implicit Option default also accepts
        // an entry that omits a required-but-nullable key (reads as `None`);
        // the write side is what keeps the keys present as nulls.
        let incomplete = serde_json::json!({
            "id": "mem-1",
            "scopeType": "user",
            "scopeId": "u-1",
            "kind": "preference",
            "subject": "s",
            "content": "c",
            "source": "explicit",
            "confidence": 0.5,
            "sensitivity": "normal",
            "status": "active",
            "pinned": false,
            "useCount": 0,
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z"
        });
        let entry: AiMemoryEntry = decode(incomplete);
        assert_eq!(entry.connection_id, None);
        let back = serde_json::to_value(&entry).ok();
        assert_eq!(
            back.as_ref().and_then(|wire| wire.get("connectionId").cloned()),
            Some(serde_json::Value::Null)
        );
    }

    #[test]
    fn a_page_and_stats_round_trip() {
        let wire = serde_json::json!({
            "entries": [],
            "total": 12,
            "limit": 20,
            "offset": 0
        });
        let page: AiMemoryPage = decode(wire.clone());
        assert_eq!(page.total, 12);
        assert_eq!(serde_json::to_value(&page).ok(), Some(wire));

        let wire = serde_json::json!({
            "total": 5,
            "byStatus": { "active": 3, "candidate": 2 },
            "byKind": { "preference": 5 },
            "byScope": { "user": 5 }
        });
        let stats: AiMemoryStats = decode(wire.clone());
        assert_eq!(stats.total, 5);
        assert_eq!(stats.by_status.get("active"), Some(&3));
        assert_eq!(stats.by_kind.get("preference"), Some(&5));
        assert_eq!(stats.by_scope.get("user"), Some(&5));
        assert_eq!(serde_json::to_value(&stats).ok(), Some(wire));
    }

    /// The create/update/list inputs omit absent keys rather than writing
    /// nulls — React's `?:` optionality.
    #[test]
    fn memory_inputs_round_trip_their_optionality() {
        let wire = serde_json::json!({
            "scopeType": "table",
            "kind": "schema_note",
            "subject": "orders",
            "content": "partitioned by month"
        });
        let create: AiMemoryCreateInput = decode(wire.clone());
        assert_eq!(create.scope_type, AiMemoryScopeKind::Table);
        assert_eq!(create.kind, AiMemoryKind::SchemaNote);
        assert_eq!(create.status, None);
        assert_eq!(create.confidence, None);
        assert_eq!(serde_json::to_value(&create).ok(), Some(wire));

        let update: AiMemoryUpdateInput = decode(serde_json::json!({
            "status": "archived",
            "pinned": false
        }));
        assert_eq!(update.status, Some(AiMemoryStatus::Archived));
        assert_eq!(update.subject, None);
        let update_wire = serde_json::to_value(&update).ok();
        assert_eq!(
            update_wire.and_then(|wire| wire.get("subject").cloned()),
            None
        );

        let params: AiMemoryListParams = decode(serde_json::json!({
            "status": "active",
            "limit": 50
        }));
        assert_eq!(params.status, Some(AiMemoryStatus::Active));
        assert_eq!(params.limit, Some(50));
        assert_eq!(params.offset, None);

        let stale: AiMemoryStaleInput =
            decode(serde_json::json!({ "connectionId": "c1", "database": "shop" }));
        assert_eq!(stale.connection_id.as_deref(), Some("c1"));
        assert_eq!(stale.schema, None);

        let extraction: AiMemoryExtractionInput = decode(serde_json::json!({
            "sql": "select 1",
            "history": [{ "sql": "select 1", "observedAt": "2026-01-01T00:00:00Z" }],
            "windowDays": 30,
            "minOccurrences": 2
        }));
        assert_eq!(extraction.history.as_ref().map(Vec::len), Some(1));
        assert_eq!(extraction.window_days, Some(30));

        let correction: AiMemoryCorrectionInput =
            decode(serde_json::json!({ "originalSql": "a", "newSql": "b" }));
        assert_eq!(correction.database, None);
        assert_eq!(correction.original_sql, "a");
    }

    /// The chat-facing citation reference round-trips (its `kind`/`scope`
    /// stay strings, like the React interface).
    #[test]
    fn a_memory_reference_round_trips_as_strings() {
        let wire = serde_json::json!({
            "reference": "M1",
            "id": "mem-1",
            "subject": "prefer lowercase",
            "kind": "preference",
            "scope": "user",
            "score": 0.42
        });
        let reference: AiMemoryReference = decode(wire.clone());
        assert_eq!(reference.reference, "M1");
        assert_eq!(reference.kind, "preference");
        assert_eq!(reference.score, 0.42);
        assert_eq!(serde_json::to_value(&reference).ok(), Some(wire));
    }
}
