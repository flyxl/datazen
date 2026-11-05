//! Shared AI memory vocabulary, retrieval, and prompt injection.
//!
//! This module is storage-neutral: hosts implement [`MemoryRepository`] over
//! their own persistence (AppDb/FTS in the desktop host), while retrieval,
//! scoring, conflict detection, and `<memory>` formatting stay identical in
//! every host. Nothing here reaches into chat request assembly; T12 wires the
//! retriever/injector into Chat/NL2SQL.

pub mod domain;
pub mod extraction;
pub mod injector;
pub mod repository;
pub mod retriever;

pub use domain::{
    conflict_memory_ids, find_active_conflict_groups, mark_conflicts, merge_memory_entries,
    normalize_memory_subject, resolve_memory_conflict, MemoryConflictGroup, MemoryConflictKey,
    MemoryConflictResolution, MemoryContext, MemoryEntry, MemoryKind, MemoryMode, MemoryScope,
    MemoryScopeKind, MemoryScore, MemorySensitivity, MemorySource, MemoryStatus, CONFIDENCE_WEIGHT,
    CONFLICT_WEIGHT, PINNED_WEIGHT, RECENCY_WEIGHT, SCOPE_WEIGHT, USAGE_WEIGHT,
};
pub use extraction::{
    correction_candidate, extract_candidates, normalize_sql_literals, parse_extraction_json,
    sql_fingerprint, summarize_sql_diff, ExtractionContext, ExtractionLimits, MemoryCandidate,
    SqlHistorySample,
};
pub use injector::{
    escape_memory_text, sanitize_memory_text, InjectedMemory, MemoryInjection,
    MemoryInjectionPolicy, MemoryInjector, MemoryReference, DEFAULT_MEMORY_TOKEN_BUDGET,
    HARD_MEMORY_TOKEN_BUDGET, MAX_INJECTED_MEMORIES, MAX_MEMORY_CONTENT_BYTES,
};
pub use repository::{MemoryError, MemoryQuery, MemoryRepository};
pub use retriever::{
    MemoryRetrievalRequest, MemoryRetriever, ScoredMemory, DEFAULT_MEMORY_CANDIDATES,
};
