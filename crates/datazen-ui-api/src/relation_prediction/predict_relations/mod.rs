//! Foreign-key prediction from metadata alone.
//!
//! Ported from React
//! `packages/frontend/src/lib/relationPrediction/predictRelations.ts`
//! (migration tasklist T082; this segment's tracker assigned the port as
//! T081 — the tasklist's own T081 is the `relationMetadata` family). It
//! answers "which tables are related?" for schemas that never declared a
//! constraint. It is pure: metadata in, ranked candidates out, no IPC and no
//! data access — so it is testable without a database and safe to run on a
//! whole database, which is what the ER diagram asks of it.
//!
//! Design constraints, each one a defence against a wrong JOIN:
//!
//! - **A relationship can only target a key.** A source column is only
//!   considered against a target's primary key or a unique index. Pointing
//!   at an arbitrary column is not a foreign key, and allowing it is how
//!   `status_id` ends up related to `order_status` merely because the names
//!   line up.
//! - **Type family is a gate, not a score.** A `varchar` column is never
//!   related to an `integer` key, however well the names match. Some dialects
//!   would compare them happily through an implicit cast, so this cannot be
//!   left to the data.
//! - **Ambiguity abstains.** When two targets score comparably the candidate
//!   is marked and the caller must not apply it automatically.
//! - **Declared foreign keys win.** Columns already covered by a real
//!   constraint are skipped rather than predicted.
//!
//! The data-overlap probe is deliberately absent: it needs to query the
//! database, and it is the part that can hurt a production server. It
//! belongs behind an explicit user action, layered on top of these
//! structural candidates.
//!
//! ## Why names are indexed rather than scanned
//!
//! Every naming rule here is "does this column name *spell out* some target
//! key?". Asking that question by comparing each source column against
//! every target key is quadratic, and measured at ~15s for a 500-table
//! schema — unusable for an ER diagram that runs it over a whole database.
//! Instead the question is inverted: each target key registers the column
//! names that would refer to it, and a source column is a single hash
//! lookup. The rules are unchanged; only the direction of the search is.
//!
//! ## Layout
//!
//! One Rust file per React file, engine and suite split for the
//! single-file size discipline:
//!
//! - `mod.rs` (this file) — the search and the scoring, plus the evidence
//!   weights and default thresholds React declares here;
//! - `types.rs` — the output half of `types.ts` (evidence/candidates/
//!   options); the input half lives with the schema adapter
//!   (`from_table_schema.rs`, T033);
//! - `tests.rs` — the mirrored React suites.
//!
//! ## Port decisions
//!
//! - Weights and scores are `f64` (JS numbers are doubles), summed
//!   left-to-right in the same order as React's `reduce`, so the pinned
//!   score tests transfer without an epsilon.
//! - React threads table object references through the match structs and
//!   re-finds them via id-keyed `Map`s; this port threads positions in the
//!   input slice. Positions are unique by construction, so React's
//!   duplicate-id edge (last table of an id silently wins the `Map` slot)
//!   cannot arise — and the ER diagram keys nodes by table name anyway.
//! - JS `Map`s iterate in insertion order and the engine leans on it: the
//!   best-per-key iteration feeds the match order, which feeds the pair
//!   order inside a group. The two order-sensitive maps (`bestPerKey`,
//!   `grouped`) are `Vec`s here, so iteration order is structural and the
//!   output deterministic.
//! - `Array#sort` has been stable since ES2019 and so is [`Vec::sort_by`];
//!   the comparators are 1:1 (`b.score - a.score` becomes `total_cmp` —
//!   scores are finite sums of finite weights, never NaN).
//! - `Math.round(score * 1000) / 1000` becomes
//!   `(score * 1000.0).round() / 1000.0`: scores are sums of non-negative
//!   weights, where Rust's round-half-away-from-zero agrees with JS's
//!   round-half-toward-+Infinity.
//! - String comparison (`<`/`>` on ids and pair lefts) is UTF-16 code-unit
//!   order in JS and UTF-8 byte order here — identical over the ASCII
//!   identifiers and rendered ids schemas produce.
//! - React's `\u0000`-joined composite map keys become the [`GroupId`]
//!   enum; its `startsWith("composite\u{0}")` test becomes a variant match.
//! - No new dependencies: a `std` [`HashMap`] is the whole reverse index.

use std::collections::{HashMap, HashSet};

use super::from_table_schema::PredictionTable;
use super::normalize::{
    TypeFamily, is_generic_key_name, normalize_data_type, normalize_identifier, singularize,
    type_family,
};

mod types;
#[cfg(test)]
mod tests;

pub use types::{
    ColumnPair, PredictionEvidence, PredictionEvidenceCode, PredictionOptions, PredictionTier,
    RelationCandidate,
};

/// Evidence weights. Tunable in one place so scoring stays inspectable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EvidenceWeights {
    pub target_is_primary_key: f64,
    pub target_is_unique: f64,
    pub name_matches_table: f64,
    pub name_matches_table_stem: f64,
    /// Table names are commonly prefixed (`app_user`), the column is not.
    pub name_matches_prefixed_table: f64,
    pub name_suffix_matches_table: f64,
    pub name_matches_key: f64,
    pub type_exact: f64,
    pub source_indexed: f64,
}

/// React's `EVIDENCE_WEIGHTS` (`as const` object) as a struct constant.
pub const EVIDENCE_WEIGHTS: EvidenceWeights = EvidenceWeights {
    target_is_primary_key: 0.4,
    target_is_unique: 0.3,
    name_matches_table: 0.4,
    name_matches_table_stem: 0.4,
    name_matches_prefixed_table: 0.25,
    name_suffix_matches_table: 0.15,
    name_matches_key: 0.3,
    type_exact: 0.1,
    source_indexed: 0.1,
};

pub const DEFAULT_HIGH_THRESHOLD: f64 = 0.8;
pub const DEFAULT_MEDIUM_THRESHOLD: f64 = 0.5;
pub const DEFAULT_AMBIGUITY_MARGIN: f64 = 0.15;

// ── Internal search state ──

/// A key a relationship may point at: the primary key or a unique index.
struct TargetKey {
    columns: Vec<String>,
    is_primary_key: bool,
}

/// One column of one target key, with everything needed to score against it.
/// React identifies the table by id; the port uses the table's position in
/// the input slice (see the module docs).
#[derive(Clone)]
struct TargetColumnRef {
    table_pos: usize,
    table_name: String,
    key_index: usize,
    column_name: String,
    type_family: TypeFamily,
    normalized_data_type: String,
}

/// A registered way of naming a target column, and what that naming is
/// worth.
struct IndexEntry {
    target: TargetColumnRef,
    weight: f64,
    code: PredictionEvidenceCode,
}

/// One scored column-pair match, before grouping.
///
/// `target_key_columns` is the target key's own column list (React keeps the
/// key object itself); the composite fold and the completeness check read
/// it, so it rides along instead of a second lookup.
struct PairMatch {
    source_pos: usize,
    source_column: String,
    target_pos: usize,
    target_key_columns: Vec<String>,
    target_column: String,
    score: f64,
    evidence: Vec<PredictionEvidence>,
}

/// Grouping key for the composite fold (React's `\u0000`-joined map keys).
///
/// A composite group folds every pair match against the *same* composite
/// key into one relationship; a pair group is one (source column, target),
/// so two references to one table stay two relationships. React
/// distinguishes the two by a `composite\u{0}` string prefix, embedding the
/// key's raw column names in one id and the source column's *normalized*
/// name in the other — the variants mirror those spellings exactly.
#[derive(Clone, PartialEq, Eq, Hash)]
enum GroupId {
    Composite {
        source_pos: usize,
        target_pos: usize,
        key_columns: Vec<String>,
    },
    Pair {
        source_pos: usize,
        target_pos: usize,
        source_column: String,
    },
}

fn evidence(code: PredictionEvidenceCode, weight: f64, detail: String) -> PredictionEvidence {
    PredictionEvidence {
        code,
        weight,
        detail,
    }
}

/// Every key a relationship may target, per table: the primary key first,
/// then each unique index that is not the primary key wearing a different
/// hat.
fn collect_target_keys(table: &PredictionTable) -> Vec<TargetKey> {
    let mut keys = Vec::new();
    if !table.primary_key.is_empty() {
        keys.push(TargetKey {
            columns: table.primary_key.clone(),
            is_primary_key: true,
        });
    }
    for set in &table.unique_column_sets {
        if set.is_empty() {
            continue;
        }
        // A unique index over the primary key adds no new target. React
        // compares the spellings verbatim (`includes`), so no normalization
        // is applied here either.
        let is_same_as_pk = set.len() == table.primary_key.len()
            && set.iter().all(|column| table.primary_key.contains(column));
        if is_same_as_pk {
            continue;
        }
        keys.push(TargetKey {
            columns: set.clone(),
            is_primary_key: false,
        });
    }
    keys
}

/// Word-boundary suffixes of a table's stem: `a_b_c` → `b_c`, `c`. A
/// prefixed table (`app_user`) is reachable from the unprefixed word it
/// ends with.
fn stem_suffixes(stem: &str) -> Vec<String> {
    // The stem is already normalized (words joined on `_`), so the split
    // yields exactly its words — like React's `stem.split('_')`.
    let words: Vec<&str> = stem.split('_').collect();
    (1..words.len())
        .map(|start| words[start..].join("_"))
        .collect()
}

/// Register every column name that would refer to `target`.
///
/// The rules, in descending strength:
///
/// - `{table}_{key}` — the convention stated in full.
/// - `{stem}_{key}` — the same, singularized (`users` → `user_id`).
/// - `{key}` alone — a domain-named key referenced by its own name
///   (`users.user_id` ← `user_id`). Generic names are excluded: every table
///   has an `id`, so matching on it would relate everything to everything.
/// - `{stemSuffix}_{key}` — a prefixed table (`app_user`) referenced by the
///   unprefixed word (`user_id`). Weaker, because two prefixed tables can
///   share that word — which is what the ambiguity check exists for.
fn register_target_column(
    index: &mut HashMap<String, Vec<IndexEntry>>,
    target: &TargetColumnRef,
) {
    let table = normalize_identifier(&target.table_name);
    let stem = normalize_identifier(&singularize(&target.table_name));
    let key = normalize_identifier(&target.column_name);

    let mut add = |name: String, weight: f64, code: PredictionEvidenceCode| {
        // React skips falsy names; only the empty string can occur here —
        // a name that normalized away entirely.
        if name.is_empty() {
            return;
        }
        index.entry(name).or_default().push(IndexEntry {
            target: target.clone(),
            weight,
            code,
        });
    };

    add(
        format!("{table}_{key}"),
        EVIDENCE_WEIGHTS.name_matches_table,
        PredictionEvidenceCode::NameMatchesTable,
    );
    if stem != table {
        add(
            format!("{stem}_{key}"),
            EVIDENCE_WEIGHTS.name_matches_table_stem,
            PredictionEvidenceCode::NameMatchesTableStem,
        );
    }
    if !is_generic_key_name(&target.column_name) {
        add(
            key.clone(),
            EVIDENCE_WEIGHTS.name_matches_key,
            PredictionEvidenceCode::NameMatchesKey,
        );
    }
    for suffix in stem_suffixes(&stem) {
        add(
            format!("{suffix}_{key}"),
            EVIDENCE_WEIGHTS.name_matches_prefixed_table,
            PredictionEvidenceCode::NameMatchesTableStem,
        );
    }
}

/// Columns already covered by a declared foreign key on this table.
fn declared_foreign_key_columns(table: &PredictionTable) -> HashSet<String> {
    let mut covered = HashSet::new();
    for fk in &table.declared_foreign_keys {
        for column in &fk.columns {
            covered.insert(normalize_identifier(column));
        }
    }
    covered
}

/// Deterministic candidate id, stable across re-prediction.
fn candidate_id(from_table: &str, to_table: &str, pairs: &[ColumnPair]) -> String {
    let rendered = pairs
        .iter()
        .map(|pair| format!("{}={}", pair.left, pair.right))
        .collect::<Vec<_>>()
        .join(",");
    format!("predicted-{from_table}-{to_table}-{rendered}")
}

/// Infer relationships among `tables`.
///
/// Results are sorted by descending score, then by id, so the output is
/// stable.
pub fn predict_relations(
    tables: &[PredictionTable],
    options: &PredictionOptions,
) -> Vec<RelationCandidate> {
    if tables.len() < 2 {
        return Vec::new();
    }

    let high_threshold = options.high_threshold.unwrap_or(DEFAULT_HIGH_THRESHOLD);
    let medium_threshold = options.medium_threshold.unwrap_or(DEFAULT_MEDIUM_THRESHOLD);
    let ambiguity_margin = options.ambiguity_margin.unwrap_or(DEFAULT_AMBIGUITY_MARGIN);
    let allow_cross_schema = options.allow_cross_schema.unwrap_or(false);

    let keys_by_table: Vec<Vec<TargetKey>> = tables.iter().map(collect_target_keys).collect();

    // ── Build the reverse index: "column name" → "what it would refer to" ──
    let mut index: HashMap<String, Vec<IndexEntry>> = HashMap::new();
    for (table_pos, table) in tables.iter().enumerate() {
        for (key_index, target_key) in keys_by_table[table_pos].iter().enumerate() {
            for key_column in &target_key.columns {
                let normalized_key_column = normalize_identifier(key_column);
                // A key naming a column the table does not report cannot be
                // targeted.
                let Some(column) = table
                    .columns
                    .iter()
                    .find(|candidate| normalize_identifier(&candidate.name) == normalized_key_column)
                else {
                    continue;
                };
                register_target_column(
                    &mut index,
                    &TargetColumnRef {
                        table_pos,
                        table_name: table.name.clone(),
                        key_index,
                        column_name: key_column.clone(),
                        type_family: type_family(&column.data_type),
                        normalized_data_type: normalize_data_type(&column.data_type),
                    },
                );
            }
        }
    }

    // ── Match each source column against the index ──
    let mut matches: Vec<PairMatch> = Vec::new();

    for (source_pos, source) in tables.iter().enumerate() {
        let covered = declared_foreign_key_columns(source);

        for column in &source.columns {
            // Ground truth first: a declared constraint is not something to
            // predict.
            if covered.contains(&normalize_identifier(&column.name)) {
                continue;
            }

            let source_name = normalize_identifier(&column.name);
            if source_name.is_empty() {
                continue;
            }

            // An exact hit uses the weight the naming rule earned. Failing
            // that, a prefix the column carries but the table does not
            // (`t_user_id`) still names a target, at the weakest weight.
            let mut entries: Option<&[IndexEntry]> = index
                .get(&source_name)
                .map(Vec::as_slice)
                .filter(|bucket| !bucket.is_empty());
            let mut weight_override: Option<f64> = None;
            if entries.is_none() {
                for suffix in stem_suffixes(&source_name) {
                    if let Some(found) = index
                        .get(&suffix)
                        .map(Vec::as_slice)
                        .filter(|bucket| !bucket.is_empty())
                    {
                        entries = Some(found);
                        weight_override = Some(EVIDENCE_WEIGHTS.name_suffix_matches_table);
                        break;
                    }
                }
            }
            let Some(entries) = entries else {
                continue;
            };

            let source_family = type_family(&column.data_type);
            let source_normalized_type = normalize_data_type(&column.data_type);
            // The same target key can be reachable by more than one
            // registered name; only the strongest naming for each key is
            // worth keeping. React keeps them in a Map (insertion-ordered);
            // a Vec with a linear find keeps that order — buckets hold a
            // handful of entries, so this is not the hot path.
            let mut best_per_key: Vec<((usize, usize, String), usize, f64)> = Vec::new();
            for (entry_pos, entry) in entries.iter().enumerate() {
                let target = &entry.target;
                if target.table_pos == source_pos {
                    continue;
                }
                if !allow_cross_schema && tables[target.table_pos].schema != source.schema {
                    continue;
                }
                // ── Type gate: families must agree, or the relationship is
                // impossible ──
                if target.type_family != source_family {
                    continue;
                }

                let weight = match weight_override {
                    None => entry.weight,
                    Some(override_weight) => entry.weight.min(override_weight),
                };
                let dedupe_key = (
                    target.table_pos,
                    target.key_index,
                    normalize_identifier(&target.column_name),
                );
                let existing = best_per_key
                    .iter()
                    .position(|(existing, _, _)| *existing == dedupe_key);
                if let Some(position) = existing {
                    // Replace only on a strictly stronger naming — exactly
                    // React's `!existing || weight > existing.weight`.
                    if weight > best_per_key[position].2 {
                        best_per_key[position] = (dedupe_key, entry_pos, weight);
                    }
                } else {
                    best_per_key.push((dedupe_key, entry_pos, weight));
                }
            }

            for (_, entry_pos, weight) in &best_per_key {
                let entry = &entries[*entry_pos];
                let target = &entry.target;
                let Some(target_key) = keys_by_table[target.table_pos].get(target.key_index)
                else {
                    // Unreachable — the ref was minted from this very key —
                    // but React guards the same lookup (`?.` + skip), so the
                    // shape is kept.
                    continue;
                };

                let mut found: Vec<PredictionEvidence> = vec![
                    evidence(
                        entry.code,
                        *weight,
                        match entry.code {
                            PredictionEvidenceCode::NameMatchesKey => format!(
                                "{} shares the key name {}",
                                column.name, target.column_name
                            ),
                            PredictionEvidenceCode::NameMatchesTable => format!(
                                "{} names {}.{}",
                                column.name, target.table_name, target.column_name
                            ),
                            _ => format!(
                                "{} names the distinguishing word of {}.{}",
                                column.name, target.table_name, target.column_name
                            ),
                        },
                    ),
                    evidence(
                        if target_key.is_primary_key {
                            PredictionEvidenceCode::TargetIsPrimaryKey
                        } else {
                            PredictionEvidenceCode::TargetIsUnique
                        },
                        if target_key.is_primary_key {
                            EVIDENCE_WEIGHTS.target_is_primary_key
                        } else {
                            EVIDENCE_WEIGHTS.target_is_unique
                        },
                        format!(
                            "{}.{} is {}",
                            target.table_name,
                            target.column_name,
                            if target_key.is_primary_key {
                                "the primary key"
                            } else {
                                "unique"
                            }
                        ),
                    ),
                ];

                if source_normalized_type == target.normalized_data_type {
                    found.push(evidence(
                        PredictionEvidenceCode::TypeExact,
                        EVIDENCE_WEIGHTS.type_exact,
                        format!("type {}", column.data_type),
                    ));
                } else {
                    found.push(evidence(
                        PredictionEvidenceCode::TypeCompatible,
                        0.0,
                        format!("{} is compatible with the target key", column.data_type),
                    ));
                }

                if column.indexed {
                    found.push(evidence(
                        PredictionEvidenceCode::SourceIndexed,
                        EVIDENCE_WEIGHTS.source_indexed,
                        "column is indexed".to_string(),
                    ));
                }

                // Recompute the total from the evidence itself so the score
                // and the explanation can never disagree.
                let score: f64 = found.iter().map(|item| item.weight).sum();
                matches.push(PairMatch {
                    source_pos,
                    source_column: column.name.clone(),
                    target_pos: target.table_pos,
                    target_key_columns: target_key.columns.clone(),
                    target_column: target.column_name.clone(),
                    score,
                    evidence: found,
                });
            }
        }
    }

    // ── Ambiguity: one source column matching several targets ──
    let mut by_source_column: HashMap<(usize, String), Vec<&PairMatch>> = HashMap::new();
    for matched in &matches {
        by_source_column
            .entry((matched.source_pos, normalize_identifier(&matched.source_column)))
            .or_default()
            .push(matched);
    }

    let mut ambiguous_columns: HashSet<(usize, String)> = HashSet::new();
    for (key, bucket) in &by_source_column {
        if bucket.len() < 2 {
            continue;
        }
        let mut sorted: Vec<&PairMatch> = bucket.clone();
        sorted.sort_by(|a, b| b.score.total_cmp(&a.score));
        // Distinct targets only: two keys of the same table are not a
        // conflict.
        if let (Some(top), Some(runner_up)) = (sorted.first(), sorted.get(1)) {
            if tables[top.target_pos].id != tables[runner_up.target_pos].id
                && top.score - runner_up.score <= ambiguity_margin
            {
                ambiguous_columns.insert(key.clone());
            }
        }
    }

    // ── Composite grouping ──
    // Two source columns that both reference the *same composite* key form
    // one relationship. Two columns referencing the same single-column key
    // do not: `orders.billing_address_id` and `orders.shipping_address_id`
    // are two relationships to one table, and merging them would invent an
    // AND that matches nothing.
    let mut grouped: HashMap<GroupId, Vec<&PairMatch>> = HashMap::new();
    let mut group_order: Vec<GroupId> = Vec::new();
    for matched in &matches {
        let group_id = if matched.target_key_columns.len() > 1 {
            GroupId::Composite {
                source_pos: matched.source_pos,
                target_pos: matched.target_pos,
                key_columns: matched.target_key_columns.clone(),
            }
        } else {
            GroupId::Pair {
                source_pos: matched.source_pos,
                target_pos: matched.target_pos,
                source_column: normalize_identifier(&matched.source_column),
            }
        };
        let is_new = !grouped.contains_key(&group_id);
        if is_new {
            group_order.push(group_id.clone());
        }
        grouped.entry(group_id).or_default().push(matched);
    }

    let mut candidates: Vec<RelationCandidate> = Vec::new();
    for group_id in &group_order {
        // Unreachable-miss guard: React asserts the bucket with `!`; the
        // same shape without the panic.
        let Some(group) = grouped.get(group_id) else {
            continue;
        };

        // A composite relationship is only complete when every column of
        // the key is covered; a partial match would generate an ON clause
        // missing a predicate.
        if let GroupId::Composite { key_columns, .. } = group_id {
            let matched: HashSet<String> = group
                .iter()
                .map(|matched| normalize_identifier(&matched.target_column))
                .collect();
            let complete = key_columns
                .iter()
                .all(|column| matched.contains(&normalize_identifier(column)));
            if !complete {
                continue;
            }
        }

        // React's `reduce` keeps the *first* of equal-scoring matches; a scan
        // with a strict `>` preserves that tie-breaking.
        let Some(mut best) = group.first().copied() else {
            continue;
        };
        for matched in group.iter().copied().skip(1) {
            if matched.score > best.score {
                best = matched;
            }
        }

        let mut pairs: Vec<ColumnPair> = group
            .iter()
            .map(|matched| ColumnPair {
                left: matched.source_column.clone(),
                right: matched.target_column.clone(),
            })
            .collect();
        // React sorts by `left` only, with a stable sort: equal lefts keep
        // the match order, which the insertion-ordered structures above
        // keep deterministic.
        pairs.sort_by(|a, b| a.left.cmp(&b.left));

        if best.score < medium_threshold {
            continue;
        }

        let ambiguous = group.iter().any(|matched| {
            ambiguous_columns.contains(&(
                matched.source_pos,
                normalize_identifier(&matched.source_column),
            ))
        });

        let mut evidence_list = best.evidence.clone();
        if ambiguous {
            evidence_list.push(evidence(
                PredictionEvidenceCode::Ambiguous,
                0.0,
                "more than one table matches this column equally well".to_string(),
            ));
        }

        candidates.push(RelationCandidate {
            id: candidate_id(
                &tables[best.source_pos].id,
                &tables[best.target_pos].id,
                &pairs,
            ),
            from_table: tables[best.source_pos].id.clone(),
            to_table: tables[best.target_pos].id.clone(),
            column_pairs: pairs,
            score: (best.score * 1000.0).round() / 1000.0,
            // An ambiguous candidate is never high, whatever it scored.
            tier: if !ambiguous && best.score >= high_threshold {
                PredictionTier::High
            } else {
                PredictionTier::Medium
            },
            evidence: evidence_list,
            ambiguous,
        });
    }

    // Descending score, then ascending id — stable, so an equal (score, id)
    // pair keeps the deterministic group order above.
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    candidates
}
