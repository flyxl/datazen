//! The output half of React `relationPrediction/types.ts`.
//!
//! Real schemas frequently have no declared foreign keys — ORM-managed,
//! legacy and sharded databases all rely on naming conventions instead of
//! constraints. The engine infers those relationships from metadata alone.
//!
//! The governing rule is that **a wrong match is worse than no match**: a
//! wrong JOIN silently changes the result set, while a missing one costs the
//! user a manual click. Everything here is therefore explainable and tiered,
//! and the caller decides what to do with each tier.
//!
//! The input half of the same React file (`PredictionColumn` /
//! `DeclaredForeignKey` / `PredictionTable`, including the `schema`
//! namespace field that landed with the engine) lives with the schema
//! adapter (`from_table_schema.rs`, tasklist T033); this file carries the
//! evidence, candidate and options vocabulary the engine
//! (`predict_relations/mod.rs`, tasklist T082) emits. The evidence weights
//! and default thresholds stay beside the engine — React declares them in
//! `predictRelations.ts`, and they are its scoring contract, not types.

/// Why a candidate was proposed — surfaced so the user can judge it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PredictionEvidenceCode {
    TargetIsPrimaryKey,
    TargetIsUnique,
    NameMatchesTable,
    NameMatchesTableStem,
    /// Declared in React's union but never emitted there either: the
    /// source-side suffix fallback keeps the entry's own code and only
    /// overrides its weight. Carried for contract parity.
    NameSuffixMatchesTable,
    NameMatchesKey,
    TypeExact,
    TypeCompatible,
    SourceIndexed,
    Ambiguous,
}

impl PredictionEvidenceCode {
    /// React's kebab-case literal — the string vocabulary the UI renders
    /// and the React tests compare against, kept so the port stays
    /// auditable against the source.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TargetIsPrimaryKey => "target-is-primary-key",
            Self::TargetIsUnique => "target-is-unique",
            Self::NameMatchesTable => "name-matches-table",
            Self::NameMatchesTableStem => "name-matches-table-stem",
            Self::NameSuffixMatchesTable => "name-suffix-matches-table",
            Self::NameMatchesKey => "name-matches-key",
            Self::TypeExact => "type-exact",
            Self::TypeCompatible => "type-compatible",
            Self::SourceIndexed => "source-indexed",
            Self::Ambiguous => "ambiguous",
        }
    }
}

/// One piece of evidence behind a candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct PredictionEvidence {
    pub code: PredictionEvidenceCode,
    pub weight: f64,
    /// Human-readable reason, already localized by the caller if needed.
    pub detail: String,
}

/// How much the caller may trust a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredictionTier {
    High,
    Medium,
}

impl PredictionTier {
    /// React's literal, kept for the same reason as
    /// [`PredictionEvidenceCode::as_str`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
        }
    }
}

/// One column pairing of a relationship (React's inline `{ left, right }`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ColumnPair {
    pub left: String,
    pub right: String,
}

/// One predicted relationship.
#[derive(Clone, Debug, PartialEq)]
pub struct RelationCandidate {
    /// Deterministic id, so a user's removal survives re-prediction.
    pub id: String,
    pub from_table: String,
    pub to_table: String,
    pub column_pairs: Vec<ColumnPair>,
    pub score: f64,
    pub tier: PredictionTier,
    pub evidence: Vec<PredictionEvidence>,
    /// Set when more than one target scored comparably. The caller must not
    /// apply such a candidate automatically — the engine refuses to guess.
    pub ambiguous: bool,
}

/// Options for the engine (`predict_relations`). Every field is optional
/// and falls back to its `DEFAULT_*` constant, exactly React's `??` chain.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PredictionOptions {
    /// Include candidates whose target sits in another schema. Default
    /// false.
    pub allow_cross_schema: Option<bool>,
    /// Score at or above which a candidate is `high`. Default 0.8.
    pub high_threshold: Option<f64>,
    /// Score at or below which a candidate is dropped entirely. Default 0.5.
    pub medium_threshold: Option<f64>,
    /// How close a runner-up may be before a match counts as ambiguous.
    /// Default 0.15.
    pub ambiguity_margin: Option<f64>,
}
