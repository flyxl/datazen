//! Memory domain vocabulary and deterministic scoring.
//!
//! The scoring formula is fixed here (not provider-specific):
//! `0.30*confidence + 0.25*usage + 0.20*recency + 0.15*scope + 0.10*pinned - 0.40*conflict`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CONFIDENCE_WEIGHT: f64 = 0.30;
pub const USAGE_WEIGHT: f64 = 0.25;
pub const RECENCY_WEIGHT: f64 = 0.20;
pub const SCOPE_WEIGHT: f64 = 0.15;
pub const PINNED_WEIGHT: f64 = 0.10;
pub const CONFLICT_WEIGHT: f64 = 0.40;

/// Scope levels ordered from broadest to narrowest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScopeKind {
    Organization,
    User,
    Connection,
    Database,
    Schema,
    Table,
    Column,
}

impl MemoryScopeKind {
    pub fn is_context_layer(self) -> bool {
        matches!(
            self,
            Self::Connection | Self::Database | Self::Schema | Self::Table | Self::Column
        )
    }
}

/// A memory's storage scope. `id` is the host's stable identifier for that
/// scope; database/schema/table/column ids should be fully qualified by the
/// host so equal names in different parents never collide.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryScope {
    pub kind: MemoryScopeKind,
    pub id: String,
}

impl MemoryScope {
    pub fn new(kind: MemoryScopeKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }

    pub fn organization(id: impl Into<String>) -> Self {
        Self::new(MemoryScopeKind::Organization, id)
    }

    pub fn user(id: impl Into<String>) -> Self {
        Self::new(MemoryScopeKind::User, id)
    }

    pub fn connection(id: impl Into<String>) -> Self {
        Self::new(MemoryScopeKind::Connection, id)
    }

    pub fn database(id: impl Into<String>) -> Self {
        Self::new(MemoryScopeKind::Database, id)
    }

    pub fn schema(id: impl Into<String>) -> Self {
        Self::new(MemoryScopeKind::Schema, id)
    }

    pub fn table(id: impl Into<String>) -> Self {
        Self::new(MemoryScopeKind::Table, id)
    }

    pub fn column(id: impl Into<String>) -> Self {
        Self::new(MemoryScopeKind::Column, id)
    }

    /// Compact label used in the prompt; callers escape it before rendering.
    /// It never includes credentials.
    pub fn label(&self) -> String {
        let prefix = match self.kind {
            MemoryScopeKind::User => return "user".to_string(),
            MemoryScopeKind::Organization => return "org".to_string(),
            MemoryScopeKind::Connection => "connection",
            MemoryScopeKind::Database => "db",
            MemoryScopeKind::Schema => "schema",
            MemoryScopeKind::Table => "table",
            MemoryScopeKind::Column => "column",
        };
        format!("{prefix}:{}", self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Preference,
    Glossary,
    Relationship,
    Pattern,
    Correction,
    Environment,
    Negative,
    SchemaNote,
}

impl MemoryKind {
    pub const fn as_str(self) -> &'static str {
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

impl std::fmt::Display for MemoryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Candidate,
    Active,
    Rejected,
    Archived,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySensitivity {
    Normal,
    Internal,
    Secret,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    #[default]
    Explicit,
    Correction,
    Usage,
    Feedback,
    SchemaNote,
    Import,
    Org,
}

/// A persisted memory entry. Hosts map their row into this value; retrieval
/// never mutates persistence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntry {
    pub id: String,
    pub organization_id: Option<String>,
    pub owner_principal_id: Option<String>,
    pub scope: MemoryScope,
    pub kind: MemoryKind,
    pub subject: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_json: Option<serde_json::Value>,
    pub source: MemorySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    pub confidence: f64,
    pub sensitivity: MemorySensitivity,
    pub status: MemoryStatus,
    pub pinned: bool,
    /// Set by the host or by [`mark_conflicts`] when another active memory
    /// occupies the same `scope + kind + subject` slot.
    pub conflict: bool,
    pub use_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MemoryEntry {
    pub fn new(
        id: impl Into<String>,
        scope: MemoryScope,
        kind: MemoryKind,
        subject: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            organization_id: None,
            owner_principal_id: None,
            scope,
            kind,
            subject: subject.into(),
            content: content.into(),
            structured_json: None,
            source: MemorySource::Explicit,
            source_ref: None,
            confidence: 0.5,
            sensitivity: MemorySensitivity::Normal,
            status: MemoryStatus::Candidate,
            pinned: false,
            conflict: false,
            use_count: 0,
            last_used_at: None,
            expires_at: None,
            content_hash: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_status(mut self, status: MemoryStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn with_sensitivity(mut self, sensitivity: MemorySensitivity) -> Self {
        self.sensitivity = sensitivity;
        self
    }

    pub fn with_source(mut self, source: MemorySource) -> Self {
        self.source = source;
        self
    }

    pub fn with_pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }

    pub fn with_conflict(mut self, conflict: bool) -> Self {
        self.conflict = conflict;
        self
    }

    pub fn with_use_count(mut self, use_count: u32) -> Self {
        self.use_count = use_count;
        self
    }

    pub fn with_last_used_at(mut self, last_used_at: DateTime<Utc>) -> Self {
        self.last_used_at = Some(last_used_at);
        self
    }

    pub fn with_created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = created_at;
        self
    }

    pub fn with_updated_at(mut self, updated_at: DateTime<Utc>) -> Self {
        self.updated_at = updated_at;
        self
    }

    pub fn is_active(&self) -> bool {
        self.status == MemoryStatus::Active
    }

    /// Identity of the slot that must have only one active memory.
    pub fn conflict_key(&self) -> MemoryConflictKey {
        MemoryConflictKey {
            scope_kind: self.scope.kind,
            scope_id: self.scope.id.clone(),
            kind: self.kind,
            subject: normalize_memory_subject(&self.subject),
        }
    }
}

/// Current request context used to resolve scope match and retrieval scopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryContext {
    pub user: MemoryScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization: Option<MemoryScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<MemoryScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<MemoryScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<MemoryScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<MemoryScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<MemoryScope>,
}

impl MemoryContext {
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user: MemoryScope::user(user_id),
            organization: None,
            connection: None,
            database: None,
            schema: None,
            table: None,
            column: None,
        }
    }

    pub fn with_organization(mut self, id: impl Into<String>) -> Self {
        self.organization = Some(MemoryScope::organization(id));
        self
    }

    pub fn with_connection(mut self, id: impl Into<String>) -> Self {
        self.connection = Some(MemoryScope::connection(id));
        self
    }

    pub fn with_database(mut self, id: impl Into<String>) -> Self {
        self.database = Some(MemoryScope::database(id));
        self
    }

    pub fn with_schema(mut self, id: impl Into<String>) -> Self {
        self.schema = Some(MemoryScope::schema(id));
        self
    }

    pub fn with_table(mut self, id: impl Into<String>) -> Self {
        self.table = Some(MemoryScope::table(id));
        self
    }

    pub fn with_column(mut self, id: impl Into<String>) -> Self {
        self.column = Some(MemoryScope::column(id));
        self
    }

    /// Most specific scope available for this request.
    pub fn target_scope(&self) -> &MemoryScope {
        self.column
            .as_ref()
            .or(self.table.as_ref())
            .or(self.schema.as_ref())
            .or(self.database.as_ref())
            .or(self.connection.as_ref())
            .unwrap_or(&self.user)
    }

    /// Context layers from narrowest to broadest, excluding user/org.
    pub fn ancestors(&self) -> Vec<&MemoryScope> {
        let mut out = Vec::new();
        for scope in [
            self.column.as_ref(),
            self.table.as_ref(),
            self.schema.as_ref(),
            self.database.as_ref(),
            self.connection.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if scope != self.target_scope() {
                out.push(scope);
            }
        }
        out
    }

    /// Scopes a repository should recall: exact target, its ancestors, user,
    /// then organization. Deduplicated while preserving specificity order.
    pub fn retrieval_scopes(&self) -> Vec<MemoryScope> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for scope in std::iter::once(self.target_scope())
            .chain(self.ancestors())
            .chain(std::iter::once(&self.user))
            .chain(self.organization.iter())
        {
            if seen.insert((scope.kind, scope.id.clone())) {
                out.push(scope.clone());
            }
        }
        out
    }

    /// PRD scope weights: exact 1.0, ancestor 0.8, user 0.6, org 0.5.
    /// Returns `None` when the scope is outside this request's context.
    pub fn scope_match(&self, scope: &MemoryScope) -> Option<f64> {
        if scope.kind == MemoryScopeKind::User {
            return (scope == &self.user).then_some(0.6);
        }
        if scope.kind == MemoryScopeKind::Organization {
            return self
                .organization
                .as_ref()
                .filter(|organization| *organization == scope)
                .map(|_| 0.5);
        }
        if scope == self.target_scope() {
            return Some(1.0);
        }
        self.ancestors()
            .into_iter()
            .any(|ancestor| ancestor == scope)
            .then_some(0.8)
    }
}

/// Component scores plus the weighted total. All components are bounded:
/// confidence/usage/recency/scope are `0..=1`, pinned is 0 or 1, and
/// `conflict_penalty` is 0 or `CONFLICT_WEIGHT` (already weighted).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryScore {
    pub confidence: f64,
    pub usage: f64,
    pub recency: f64,
    pub scope_match: f64,
    pub pinned: f64,
    pub conflict_penalty: f64,
    pub total: f64,
}

impl MemoryScore {
    pub fn compute(entry: &MemoryEntry, scope_match: f64, now: DateTime<Utc>) -> Self {
        let confidence = finite_clamp(entry.confidence, 0.0, 1.0);
        let usage = usage_norm(entry.use_count);
        let recency = recency_score(entry.last_used_at.unwrap_or(entry.updated_at), now);
        let scope_match = finite_clamp(scope_match, 0.0, 1.0);
        let pinned = if entry.pinned { 1.0 } else { 0.0 };
        let conflict_penalty = if entry.conflict { CONFLICT_WEIGHT } else { 0.0 };
        let total = CONFIDENCE_WEIGHT * confidence
            + USAGE_WEIGHT * usage
            + RECENCY_WEIGHT * recency
            + SCOPE_WEIGHT * scope_match
            + PINNED_WEIGHT * pinned
            - conflict_penalty;
        Self {
            confidence,
            usage,
            recency,
            scope_match,
            pinned,
            conflict_penalty,
            total,
        }
    }
}

fn finite_clamp(value: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}

fn usage_norm(use_count: u32) -> f64 {
    let raw = (1.0 + use_count as f64).ln() / 50.0_f64.ln();
    raw.min(1.0)
}

fn recency_score(last_used: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    let days = now.signed_duration_since(last_used).num_seconds() as f64 / 86_400.0;
    (-days.max(0.0) / 90.0).exp()
}

/// Normalized slot subject used for uniqueness/conflict grouping.
pub fn normalize_memory_subject(subject: &str) -> String {
    subject
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryConflictKey {
    pub scope_kind: MemoryScopeKind,
    pub scope_id: String,
    pub kind: MemoryKind,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryConflictGroup {
    pub key: MemoryConflictKey,
    pub memory_ids: Vec<String>,
    pub highest_confidence: f64,
    pub newest_updated_at: DateTime<Utc>,
}

/// Active memories that share a `scope + kind + subject` slot. More than one id
/// means the host should surface a conflict prompt.
pub fn find_active_conflict_groups(entries: &[MemoryEntry]) -> Vec<MemoryConflictGroup> {
    let mut groups: BTreeMap<MemoryConflictKey, Vec<&MemoryEntry>> = BTreeMap::new();
    for entry in entries.iter().filter(|entry| entry.is_active()) {
        groups.entry(entry.conflict_key()).or_default().push(entry);
    }
    groups
        .into_iter()
        .filter(|(_, entries)| entries.len() > 1)
        .map(|(key, entries)| {
            let highest = entries
                .iter()
                .map(|entry| entry.confidence)
                .fold(f64::NEG_INFINITY, f64::max);
            let newest = entries
                .iter()
                .map(|entry| entry.updated_at)
                .max()
                .unwrap_or(DateTime::<Utc>::MIN_UTC);
            MemoryConflictGroup {
                key,
                memory_ids: entries.iter().map(|entry| entry.id.clone()).collect(),
                highest_confidence: highest,
                newest_updated_at: newest,
            }
        })
        .collect()
}

pub fn conflict_memory_ids(entries: &[MemoryEntry]) -> BTreeSet<String> {
    find_active_conflict_groups(entries)
        .into_iter()
        .flat_map(|group| group.memory_ids)
        .collect()
}

/// Mark every active member of a conflicting slot. Returns the number of
/// entries whose `conflict` flag changed.
pub fn mark_conflicts(entries: &mut [MemoryEntry]) -> usize {
    let conflicting = conflict_memory_ids(entries);
    entries
        .iter_mut()
        .filter(|entry| conflicting.contains(&entry.id) && !entry.conflict)
        .map(|entry| {
            entry.conflict = true;
            1
        })
        .sum()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryConflictResolution {
    /// Keep both rows visible and ask the user; both receive the -0.40 score
    /// penalty while unresolved.
    KeepBoth { memory_ids: Vec<String> },
    /// One row supersedes the other; T14 archives the loser.
    Prefer { winner_id: String, loser_id: String },
    /// Merge into the winner, retaining both source ids in audit metadata.
    Merge {
        winner_id: String,
        loser_id: String,
        merged_content: String,
    },
}

/// Pure adjudication for T14. Explicit user memories that cannot be separated
/// stay in conflict; otherwise a clear confidence gap or newer update wins.
/// It never deletes data and never silently rewrites a memory.
pub fn resolve_memory_conflict(
    existing: &MemoryEntry,
    incoming: &MemoryEntry,
) -> MemoryConflictResolution {
    if existing.conflict_key() != incoming.conflict_key() {
        return MemoryConflictResolution::KeepBoth {
            memory_ids: vec![existing.id.clone(), incoming.id.clone()],
        };
    }
    if existing.source == MemorySource::Explicit && incoming.source == MemorySource::Explicit {
        return MemoryConflictResolution::KeepBoth {
            memory_ids: vec![existing.id.clone(), incoming.id.clone()],
        };
    }
    if (existing.confidence - incoming.confidence).abs() >= 0.10 {
        let (winner, loser) = if existing.confidence > incoming.confidence {
            (existing, incoming)
        } else {
            (incoming, existing)
        };
        return MemoryConflictResolution::Prefer {
            winner_id: winner.id.clone(),
            loser_id: loser.id.clone(),
        };
    }
    if existing.updated_at != incoming.updated_at {
        let (winner, loser) = if incoming.updated_at > existing.updated_at {
            (incoming, existing)
        } else {
            (existing, incoming)
        };
        return MemoryConflictResolution::Prefer {
            winner_id: winner.id.clone(),
            loser_id: loser.id.clone(),
        };
    }
    MemoryConflictResolution::KeepBoth {
        memory_ids: vec![existing.id.clone(), incoming.id.clone()],
    }
}

/// Pure merge helper for T14: preserve primary identity, combine content, take
/// the max confidence, sum usage, and keep the conflict marker until reviewed.
pub fn merge_memory_entries(primary: &MemoryEntry, secondary: &MemoryEntry) -> MemoryEntry {
    let mut merged = primary.clone();
    if !secondary.content.trim().is_empty() && !merged.content.contains(&secondary.content) {
        if !merged.content.trim().is_empty() {
            merged.content.push('\n');
        }
        merged.content.push_str(secondary.content.trim());
    }
    merged.confidence = primary.confidence.max(secondary.confidence);
    merged.use_count = primary.use_count.saturating_add(secondary.use_count);
    merged.pinned = primary.pinned || secondary.pinned;
    merged.conflict = true;
    merged.updated_at = primary.updated_at.max(secondary.updated_at);
    merged.content_hash = None;
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, day, 0, 0, 0).unwrap()
    }

    fn entry(id: &str, scope: MemoryScope) -> MemoryEntry {
        MemoryEntry::new(
            id,
            scope,
            MemoryKind::Preference,
            "default limit",
            "LIMIT 100",
        )
        .with_status(MemoryStatus::Active)
        .with_confidence(0.8)
        .with_updated_at(at(1))
    }

    #[test]
    fn score_components_match_the_documented_formula() {
        let table = MemoryScope::table("public.orders");
        let mut value = entry("m1", table.clone())
            .with_use_count(49)
            .with_last_used_at(at(10))
            .with_pinned(true);
        value.updated_at = at(10);
        let score = MemoryScore::compute(&value, 1.0, at(10));
        assert!((score.confidence - 0.8).abs() < 1e-9);
        assert!((score.usage - 1.0).abs() < 1e-9);
        assert!((score.recency - 1.0).abs() < 1e-9);
        assert!((score.scope_match - 1.0).abs() < 1e-9);
        assert!((score.pinned - 1.0).abs() < 1e-9);
        assert!((score.conflict_penalty - 0.0).abs() < 1e-9);
        assert!((score.total - 0.94).abs() < 1e-9, "{score:?}");
    }

    #[test]
    fn recency_and_usage_are_bounded() {
        let now = at(10);
        let old = entry("old", MemoryScope::user("u"))
            .with_use_count(10_000)
            .with_last_used_at(at(1));
        let recency = recency_score(at(1), now);
        assert!(recency > 0.0 && recency < 1.0);
        assert_eq!(usage_norm(u32::MAX), 1.0);
        let future = recency_score(at(20), now);
        assert!((future - 1.0).abs() < 1e-9);
        assert!(old.updated_at <= now);
    }

    #[test]
    fn conflict_penalty_is_exactly_forty_percent() {
        let scope = MemoryScope::table("t");
        let base = entry("base", scope.clone());
        let conflicted = entry("conflicted", scope).with_conflict(true);
        let base_score = MemoryScore::compute(&base, 1.0, at(10));
        let conflict_score = MemoryScore::compute(&conflicted, 1.0, at(10));
        assert!((base_score.total - conflict_score.total - 0.40).abs() < 1e-9);
    }

    #[test]
    fn context_scope_match_follows_exact_parent_user_org() {
        let context = MemoryContext::new("u1")
            .with_organization("org1")
            .with_connection("c1")
            .with_database("db1")
            .with_table("t1");
        assert_eq!(context.scope_match(&MemoryScope::table("t1")), Some(1.0));
        assert_eq!(
            context.scope_match(&MemoryScope::database("db1")),
            Some(0.8)
        );
        assert_eq!(
            context.scope_match(&MemoryScope::connection("c1")),
            Some(0.8)
        );
        assert_eq!(context.scope_match(&MemoryScope::user("u1")), Some(0.6));
        assert_eq!(
            context.scope_match(&MemoryScope::organization("org1")),
            Some(0.5)
        );
        assert_eq!(context.scope_match(&MemoryScope::table("other")), None);
        assert_eq!(context.scope_match(&MemoryScope::user("other")), None);
    }

    #[test]
    fn retrieval_scopes_are_deduplicated_and_specific_first() {
        let context = MemoryContext::new("u1")
            .with_connection("c1")
            .with_table("t1");
        let scopes = context.retrieval_scopes();
        let labels: Vec<_> = scopes.iter().map(MemoryScope::label).collect();
        assert_eq!(labels, vec!["table:t1", "connection:c1", "user"]);
    }

    #[test]
    fn active_conflicts_are_grouped_and_marked() {
        let scope = MemoryScope::table("t1");
        let first = entry("m1", scope.clone()).with_confidence(0.9);
        let second = entry("m2", scope.clone()).with_confidence(0.4);
        let third = entry("m3", scope).with_status(MemoryStatus::Archived);
        let mut entries = vec![first, second, third];
        let groups = find_active_conflict_groups(&entries);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].memory_ids, vec!["m1", "m2"]);
        assert_eq!(mark_conflicts(&mut entries), 2);
        assert!(entries[0].conflict && entries[1].conflict);
        assert!(!entries[2].conflict);
    }

    #[test]
    fn explicit_conflict_resolution_keeps_both() {
        let scope = MemoryScope::table("t1");
        let first = entry("m1", scope.clone()).with_source(MemorySource::Explicit);
        let second = entry("m2", scope).with_source(MemorySource::Explicit);
        assert!(matches!(
            resolve_memory_conflict(&first, &second),
            MemoryConflictResolution::KeepBoth { .. }
        ));
    }

    #[test]
    fn conflict_resolution_prefers_a_clear_confidence_gap() {
        let scope = MemoryScope::table("t1");
        let first = entry("m1", scope.clone())
            .with_source(MemorySource::Import)
            .with_confidence(0.9);
        let second = entry("m2", scope)
            .with_source(MemorySource::Import)
            .with_confidence(0.5);
        match resolve_memory_conflict(&first, &second) {
            MemoryConflictResolution::Prefer {
                winner_id,
                loser_id,
            } => {
                assert_eq!(winner_id, "m1");
                assert_eq!(loser_id, "m2");
            }
            other => panic!("expected prefer, got {other:?}"),
        }
    }

    #[test]
    fn merge_preserves_primary_identity_and_marks_conflict() {
        let scope = MemoryScope::table("t1");
        let first = entry("m1", scope.clone())
            .with_use_count(2)
            .with_confidence(0.6);
        let second = entry("m2", scope).with_use_count(3).with_confidence(0.9);
        let merged = merge_memory_entries(&first, &second);
        assert_eq!(merged.id, "m1");
        assert!(merged.content.contains("LIMIT 100"));
        assert_eq!(merged.use_count, 5);
        assert_eq!(merged.confidence, 0.9);
        assert!(merged.conflict);
    }
}
