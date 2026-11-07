//! Organization-scoped memory publication and audit vocabulary.
//!
//! Organization memories are intentionally **not** ordinary local memories:
//! a member can read an approved entry but only an organization administrator
//! can publish or revoke one. The service that owns authorization lives in a
//! host with a real organization role model (the team server); this module
//! keeps the wire/persistence-neutral shape and the deterministic versioning
//! rules shared by that host and its clients.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::domain::{MemoryKind, MemorySensitivity};

pub const MAX_ORG_MEMORY_SUBJECT_CHARS: usize = 200;
pub const MAX_ORG_MEMORY_CONTENT_BYTES: usize = 8 * 1024;

/// Lifecycle of one published organization-memory version. There is no
/// candidate state here: an administrator publishing an entry *is* the
/// approval, and a revoked version must disappear from retrieval immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgMemoryStatus {
    Active,
    Revoked,
}

impl OrgMemoryStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "active" => Some(Self::Active),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }
}

/// Audit decisions are deliberately separate from lifecycle status: an
/// attempted injection after revocation is still auditable as `suppressed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgMemoryDecision {
    Approved,
    Revoked,
    Injected,
    Suppressed,
}

impl OrgMemoryDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Revoked => "revoked",
            Self::Injected => "injected",
            Self::Suppressed => "suppressed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "approved" => Some(Self::Approved),
            "revoked" => Some(Self::Revoked),
            "injected" => Some(Self::Injected),
            "suppressed" => Some(Self::Suppressed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgMemoryAuditAction {
    Publish,
    Revoke,
    Inject,
}

impl OrgMemoryAuditAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Publish => "publish",
            Self::Revoke => "revoke",
            Self::Inject => "inject",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "publish" => Some(Self::Publish),
            "revoke" => Some(Self::Revoke),
            "inject" => Some(Self::Inject),
            _ => None,
        }
    }
}

/// One version of an organization memory. `id` is stable across versions;
/// `version` is monotonic per `(organization_id, id)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgMemoryEntry {
    pub id: String,
    pub organization_id: String,
    pub version: u32,
    pub kind: MemoryKind,
    pub subject: String,
    pub content: String,
    pub confidence: f64,
    pub sensitivity: MemorySensitivity,
    pub status: OrgMemoryStatus,
    pub created_by: String,
    pub approved_by: String,
    pub published_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_by: Option<String>,
}

impl OrgMemoryEntry {
    /// A revoked entry is never eligible for prompt injection. This is the
    /// single predicate hosts and clients share, so a stale cache cannot
    /// resurrect a revoked fact.
    pub fn is_injectable(&self) -> bool {
        self.status == OrgMemoryStatus::Active
    }
}

/// The fields an administrator submits when publishing. `memory_id` is
/// optional: omit it for a new memory, reuse it to publish the next version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgMemoryDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<String>,
    pub kind: MemoryKind,
    pub subject: String,
    pub content: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default = "default_sensitivity")]
    pub sensitivity: MemorySensitivity,
}

fn default_confidence() -> f64 {
    0.5
}

fn default_sensitivity() -> MemorySensitivity {
    MemorySensitivity::Normal
}

/// Audit rows never carry memory content or credentials. `detail_json` is for
/// non-sensitive context (for example the source connection id) only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgMemoryAuditEntry {
    pub id: String,
    pub organization_id: String,
    pub memory_id: String,
    pub version: u32,
    pub action: OrgMemoryAuditAction,
    pub actor_principal_id: String,
    pub decision: OrgMemoryDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OrgMemoryError {
    #[error("invalid organization memory: {0}")]
    Validation(String),
    #[error("organization memory not found")]
    NotFound,
    #[error("organization memory storage error: {0}")]
    Repository(String),
}

pub fn validate_org_memory_draft(draft: &OrgMemoryDraft) -> Result<(), OrgMemoryError> {
    let subject = draft.subject.trim();
    if subject.is_empty() {
        return Err(OrgMemoryError::Validation("subject is required".into()));
    }
    if subject.chars().count() > MAX_ORG_MEMORY_SUBJECT_CHARS {
        return Err(OrgMemoryError::Validation(format!(
            "subject exceeds {MAX_ORG_MEMORY_SUBJECT_CHARS} characters"
        )));
    }
    let content = draft.content.trim();
    if content.is_empty() {
        return Err(OrgMemoryError::Validation("content is required".into()));
    }
    if content.len() > MAX_ORG_MEMORY_CONTENT_BYTES {
        return Err(OrgMemoryError::Validation(format!(
            "content exceeds {MAX_ORG_MEMORY_CONTENT_BYTES} bytes"
        )));
    }
    if !draft.confidence.is_finite() || !(0.0..=1.0).contains(&draft.confidence) {
        return Err(OrgMemoryError::Validation(
            "confidence must be finite and between 0 and 1".into(),
        ));
    }
    if draft.sensitivity == MemorySensitivity::Secret {
        return Err(OrgMemoryError::Validation(
            "secret organization memories are rejected".into(),
        ));
    }
    if let Some(memory_id) = &draft.memory_id {
        if memory_id.trim().is_empty() {
            return Err(OrgMemoryError::Validation(
                "memoryId must not be empty when supplied".into(),
            ));
        }
    }
    Ok(())
}

/// Next version for one stable memory id. Versions start at 1 and never reuse
/// a previous number, even after a revoke.
pub fn next_org_memory_version(versions: impl IntoIterator<Item = u32>) -> u32 {
    versions.into_iter().max().unwrap_or(0).saturating_add(1)
}

/// The active, newest version of every memory id. Older active versions are
/// superseded by the newest approved version and must not be injected.
pub fn latest_active_org_memories(entries: &[OrgMemoryEntry]) -> Vec<OrgMemoryEntry> {
    let mut latest: std::collections::BTreeMap<&str, &OrgMemoryEntry> = Default::default();
    for entry in entries {
        match latest.get(entry.id.as_str()) {
            Some(current) if current.version >= entry.version => {}
            _ => {
                latest.insert(entry.id.as_str(), entry);
            }
        }
    }
    let mut values: Vec<OrgMemoryEntry> = latest
        .into_values()
        .filter(|entry| entry.is_injectable())
        .cloned()
        .collect();
    values.sort_by(|left, right| {
        right
            .published_at
            .cmp(&left.published_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn entry(id: &str, version: u32, status: OrgMemoryStatus) -> OrgMemoryEntry {
        OrgMemoryEntry {
            id: id.into(),
            organization_id: "org-a".into(),
            version,
            kind: MemoryKind::Glossary,
            subject: format!("subject-{id}-{version}"),
            content: "content".into(),
            confidence: 0.9,
            sensitivity: MemorySensitivity::Normal,
            status,
            created_by: "admin-1".into(),
            approved_by: "admin-1".into(),
            published_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            revoked_at: None,
            revoked_by: None,
        }
    }

    #[test]
    fn versions_are_monotonic_and_never_reused() {
        assert_eq!(next_org_memory_version([]), 1);
        assert_eq!(next_org_memory_version([1, 4, 2]), 5);
        assert_eq!(next_org_memory_version([u32::MAX]), u32::MAX);
    }

    #[test]
    fn revoked_and_older_versions_are_not_injectable() {
        let entries = vec![
            entry("m1", 1, OrgMemoryStatus::Active),
            entry("m1", 2, OrgMemoryStatus::Active),
            entry("m2", 1, OrgMemoryStatus::Revoked),
        ];
        let latest = latest_active_org_memories(&entries);
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].id, "m1");
        assert_eq!(latest[0].version, 2);
    }

    #[test]
    fn revoked_latest_version_suppresses_the_whole_memory() {
        let entries = vec![
            entry("m1", 1, OrgMemoryStatus::Active),
            entry("m1", 2, OrgMemoryStatus::Revoked),
        ];
        assert!(latest_active_org_memories(&entries).is_empty());
    }

    #[test]
    fn draft_validation_rejects_empty_secret_and_oversized_values() {
        let mut draft = OrgMemoryDraft {
            memory_id: None,
            kind: MemoryKind::Glossary,
            subject: "Revenue".into(),
            content: "Use paid_at".into(),
            confidence: 0.9,
            sensitivity: MemorySensitivity::Normal,
        };
        assert!(validate_org_memory_draft(&draft).is_ok());
        draft.subject.clear();
        assert!(validate_org_memory_draft(&draft).is_err());
        draft.subject = "Revenue".into();
        draft.sensitivity = MemorySensitivity::Secret;
        assert!(validate_org_memory_draft(&draft).is_err());
        draft.sensitivity = MemorySensitivity::Normal;
        draft.content = "x".repeat(MAX_ORG_MEMORY_CONTENT_BYTES + 1);
        assert!(validate_org_memory_draft(&draft).is_err());
        draft.content = "ok".into();
        draft.confidence = f64::NAN;
        assert!(validate_org_memory_draft(&draft).is_err());
    }
}
