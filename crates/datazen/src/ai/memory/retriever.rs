//! Scope-aware memory recall, deterministic scoring, and conflict marking.

#[cfg(test)]
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::domain::{conflict_memory_ids, MemoryContext, MemoryEntry, MemoryScore, MemoryStatus};
use super::repository::{MemoryError, MemoryQuery, MemoryRepository};

/// Default candidate cap returned by retrieval before prompt-budget trimming.
pub const DEFAULT_MEMORY_CANDIDATES: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRetrievalRequest {
    pub text: String,
    /// Candidate memories are recalled only when the UI enables preview.
    pub include_candidates: bool,
    pub max_candidates: usize,
    /// Injected explicitly for deterministic recency tests; hosts pass
    /// `Utc::now()`.
    pub now: DateTime<Utc>,
}

impl Default for MemoryRetrievalRequest {
    fn default() -> Self {
        Self {
            text: String::new(),
            include_candidates: false,
            max_candidates: DEFAULT_MEMORY_CANDIDATES,
            now: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredMemory {
    pub entry: MemoryEntry,
    pub score: MemoryScore,
}

/// Retrieval is intentionally storage-neutral: the host supplies candidate
/// rows (FTS/keyword recall), and this layer applies the shared scope rules,
/// formula, conflict marking, and stable ordering.
pub struct MemoryRetriever<'a> {
    repository: &'a dyn MemoryRepository,
}

impl<'a> MemoryRetriever<'a> {
    pub fn new(repository: &'a dyn MemoryRepository) -> Self {
        Self { repository }
    }

    pub async fn retrieve(
        &self,
        context: &MemoryContext,
        request: &MemoryRetrievalRequest,
    ) -> Result<Vec<ScoredMemory>, MemoryError> {
        let mut statuses = vec![MemoryStatus::Active];
        if request.include_candidates {
            statuses.push(MemoryStatus::Candidate);
        }
        let max_candidates = request.max_candidates.min(100);
        if max_candidates == 0 {
            return Ok(Vec::new());
        }
        let query = MemoryQuery {
            text: request.text.clone(),
            scopes: context.retrieval_scopes(),
            statuses: statuses.clone(),
            // Ask the host for a superset so scope/status/expiry filtering
            // cannot starve the final candidate list.
            limit: max_candidates.saturating_mul(4).max(32),
        };
        let recalled = self.repository.candidates(&query).await?;

        let mut seen = std::collections::BTreeSet::new();
        let mut eligible: Vec<MemoryEntry> = recalled
            .into_iter()
            .filter(|entry| statuses.contains(&entry.status))
            .filter(|entry| !seen.contains(&entry.id) && seen.insert(entry.id.clone()))
            .filter(|entry| {
                entry
                    .expires_at
                    .map_or(true, |expires| expires > request.now)
            })
            .filter(|entry| context.scope_match(&entry.scope).is_some())
            .collect();
        let conflicting = conflict_memory_ids(&eligible);
        for entry in &mut eligible {
            if conflicting.contains(&entry.id) {
                entry.conflict = true;
            }
        }

        let mut scored: Vec<ScoredMemory> = eligible
            .into_iter()
            .filter_map(|entry| {
                let scope_match = context.scope_match(&entry.scope)?;
                let score = MemoryScore::compute(&entry, scope_match, request.now);
                Some(ScoredMemory { entry, score })
            })
            .collect();

        // Deterministic tie-break: score desc, updated_at desc, id asc.
        scored.sort_by(|left, right| {
            right
                .score
                .total
                .partial_cmp(&left.score.total)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.entry.updated_at.cmp(&left.entry.updated_at))
                .then_with(|| left.entry.id.cmp(&right.entry.id))
        });
        scored.truncate(max_candidates);
        Ok(scored)
    }
}

/// Test-only adapter used by the shared unit tests. Hosts implement the trait
/// against real persistence; this never ships in a production path.
#[cfg(test)]
pub(crate) struct StaticMemoryRepository {
    pub entries: Vec<MemoryEntry>,
}

#[cfg(test)]
#[async_trait]
impl MemoryRepository for StaticMemoryRepository {
    async fn candidates(&self, _query: &MemoryQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        Ok(self.entries.clone())
    }

    async fn list_by_scope(
        &self,
        scope: &super::domain::MemoryScope,
        status: Option<MemoryStatus>,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        Ok(self
            .entries
            .iter()
            .filter(|entry| &entry.scope == scope)
            .filter(|entry| status.map_or(true, |status| entry.status == status))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::memory::domain::{MemoryKind, MemoryScope};
    use async_trait::async_trait;
    use chrono::TimeZone;

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, day, 0, 0, 0).unwrap()
    }

    fn active(id: &str, scope: MemoryScope) -> MemoryEntry {
        MemoryEntry::new(
            id,
            scope,
            MemoryKind::Preference,
            format!("subject-{id}"),
            format!("content-{id}"),
        )
        .with_status(MemoryStatus::Active)
        .with_confidence(0.5)
        .with_created_at(at(1))
        .with_updated_at(at(1))
    }

    fn context() -> MemoryContext {
        MemoryContext::new("u1")
            .with_connection("c1")
            .with_database("db1")
            .with_table("t1")
    }

    fn request() -> MemoryRetrievalRequest {
        MemoryRetrievalRequest {
            text: "limit".into(),
            include_candidates: false,
            max_candidates: 64,
            now: at(10),
        }
    }

    #[tokio::test]
    async fn exact_scope_outranks_ancestor_with_equal_confidence() {
        let repository = StaticMemoryRepository {
            entries: vec![
                active("parent", MemoryScope::connection("c1")),
                active("exact", MemoryScope::table("t1")),
            ],
        };
        let scored = MemoryRetriever::new(&repository)
            .retrieve(&context(), &request())
            .await
            .unwrap();
        assert_eq!(scored[0].entry.id, "exact");
        assert!(scored[0].score.scope_match > scored[1].score.scope_match);
    }

    #[tokio::test]
    async fn unrelated_scope_is_filtered_not_scored() {
        let repository = StaticMemoryRepository {
            entries: vec![active("other", MemoryScope::table("other"))],
        };
        let scored = MemoryRetriever::new(&repository)
            .retrieve(&context(), &request())
            .await
            .unwrap();
        assert!(scored.is_empty());
    }

    #[tokio::test]
    async fn weighted_fixture_orders_confidence_usage_recency_and_pinned() {
        let scope = MemoryScope::table("t1");
        let repository = StaticMemoryRepository {
            entries: vec![
                active("high", scope.clone())
                    .with_confidence(1.0)
                    .with_updated_at(at(1)),
                active("used", scope.clone())
                    .with_use_count(49)
                    .with_updated_at(at(1)),
                active("recent", scope.clone()).with_updated_at(at(10)),
                active("pinned", scope)
                    .with_pinned(true)
                    .with_updated_at(at(1)),
            ],
        };
        let scored = MemoryRetriever::new(&repository)
            .retrieve(&context(), &request())
            .await
            .unwrap();
        let ids: Vec<_> = scored.iter().map(|item| item.entry.id.as_str()).collect();
        assert_eq!(ids, vec!["used", "high", "pinned", "recent"]);
    }

    #[tokio::test]
    async fn tie_break_is_updated_at_then_id() {
        let repository = StaticMemoryRepository {
            entries: vec![
                active("b", MemoryScope::table("t1")).with_updated_at(at(2)),
                active("a", MemoryScope::table("t1")).with_updated_at(at(2)),
                active("older", MemoryScope::table("t1")).with_updated_at(at(1)),
            ],
        };
        let scored = MemoryRetriever::new(&repository)
            .retrieve(&context(), &request())
            .await
            .unwrap();
        let ids: Vec<_> = scored.iter().map(|item| item.entry.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "older"]);
    }

    #[tokio::test]
    async fn candidates_are_excluded_unless_preview_is_enabled() {
        let repository = StaticMemoryRepository {
            entries: vec![
                active("candidate", MemoryScope::table("t1")).with_status(MemoryStatus::Candidate)
            ],
        };
        let retriever = MemoryRetriever::new(&repository);
        assert!(retriever
            .retrieve(&context(), &request())
            .await
            .unwrap()
            .is_empty());
        let mut preview = request();
        preview.include_candidates = true;
        assert_eq!(
            retriever
                .retrieve(&context(), &preview)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn expired_and_archived_entries_are_not_returned() {
        let mut expired = active("expired", MemoryScope::table("t1"));
        expired.expires_at = Some(at(5));
        let repository = StaticMemoryRepository {
            entries: vec![
                expired,
                active("archived", MemoryScope::table("t1")).with_status(MemoryStatus::Archived),
            ],
        };
        assert!(MemoryRetriever::new(&repository)
            .retrieve(&context(), &request())
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn duplicate_active_slot_is_marked_conflicted_and_penalized() {
        let mut first = active("m1", MemoryScope::table("t1")).with_confidence(0.8);
        let mut second = active("m2", MemoryScope::table("t1")).with_confidence(0.8);
        first.subject = "same subject".into();
        second.subject = "same subject".into();
        let repository = StaticMemoryRepository {
            entries: vec![first, second],
        };
        let scored = MemoryRetriever::new(&repository)
            .retrieve(&context(), &request())
            .await
            .unwrap();
        assert_eq!(scored.len(), 2);
        assert!(scored.iter().all(|item| item.entry.conflict));
        assert!(scored
            .iter()
            .all(|item| (item.score.conflict_penalty - 0.40).abs() < 1e-9));
    }

    #[tokio::test]
    async fn max_candidates_truncates_after_sorting() {
        let mut entries = Vec::new();
        for index in 0..10 {
            entries.push(active(&format!("m{index}"), MemoryScope::table("t1")));
        }
        let repository = StaticMemoryRepository { entries };
        let mut request = request();
        request.max_candidates = 3;
        let scored = MemoryRetriever::new(&repository)
            .retrieve(&context(), &request)
            .await
            .unwrap();
        assert_eq!(scored.len(), 3);
    }
}
