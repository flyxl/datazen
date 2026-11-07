//! T14 candidate extraction: deterministic SQL normalization and learning.
//!
//! Everything here is pure: callers supply SQL, scope, history, a clock, and
//! thresholds. The host owns persistence and must still create candidates in
//! `candidate` state — extraction never activates a memory.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::domain::{MemoryKind, MemoryScope, MemorySource, MemoryStatus};

static IN_LIST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bIN\s*\(\s*\?(?:\s*,\s*\?)+\s*\)").expect("valid IN-list regex")
});
static JOIN_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bJOIN\b").expect("valid JOIN-start regex"));
static JOIN_SEGMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r##"(?is)^JOIN\s+([A-Za-z0-9_.`"\[\]]+)(?:\s+(?:AS\s+)?([A-Za-z0-9_]+))?\s+ON\s+(.+)$"##,
    )
    .expect("valid JOIN-segment regex")
});
static CLAUSE_STOP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+(?:JOIN|WHERE|GROUP\s+BY|ORDER\s+BY|LIMIT|HAVING|UNION)\b")
        .expect("valid clause-stop regex")
});
static WHERE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)\b(?:WHERE|HAVING)\s+(.+?)(?:;|$)").expect("valid WHERE regex")
});
static PREDICATE_SPLIT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s+(?:AND|OR)\s+").expect("valid predicate-split regex"));
static NEGATIVE_PREDICATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r##"(?i)^\s*([A-Za-z0-9_."`\[\]]+)\s*(!=|<>|NOT\s+IN|NOT\s+LIKE|NOT\s+ILIKE)\s*(.+?)\s*$"##,
    )
    .expect("valid negative-predicate regex")
});
static FILTER_PREDICATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r##"(?is)^\s*([A-Za-z0-9_."`\[\]]+\s*(?:=|!=|<>|<=|>=|<|>|LIKE|ILIKE|IN|IS|BETWEEN)\s*.+)$"##,
    )
    .expect("valid filter-predicate regex")
});
static EXPRESSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?is)\b(DATE_TRUNC|SUM|COUNT|AVG|MIN|MAX|COALESCE|EXTRACT|SUBSTRING|CAST)\s*\(([^()]{0,160})\)",
    )
    .expect("valid expression regex")
});
static EQUALITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r##"(?i)([A-Za-z0-9_."`\[\]]+)\s*=\s*([A-Za-z0-9_."`\[\]]+)"##)
        .expect("valid equality regex")
});

/// The rolling-window policy for repeated-pattern proposals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionLimits {
    pub window_days: u32,
    pub min_occurrences: u32,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            window_days: 14,
            min_occurrences: 3,
        }
    }
}

/// One historical execution supplied by the host. `observed_at` is when the
/// SQL ran, not when extraction ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlHistorySample {
    pub sql: String,
    pub observed_at: DateTime<Utc>,
}

/// Scope and privacy context attached to every extracted candidate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtractionContext {
    pub connection_id: Option<String>,
    pub database: Option<String>,
}

impl ExtractionContext {
    pub fn scope(&self) -> MemoryScope {
        if let Some(connection_id) = self
            .connection_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return MemoryScope::connection(connection_id);
        }
        if let Some(database) = self
            .database
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return MemoryScope::database(database);
        }
        MemoryScope::user("local")
    }
}

/// A proposal from the shared layer. The host assigns an id/time and persists
/// it as [`MemoryStatus::Candidate`]; status is explicit so callers cannot
/// accidentally activate learned material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCandidate {
    pub kind: MemoryKind,
    pub subject: String,
    pub content: String,
    pub confidence: f64,
    pub source: MemorySource,
    pub status: MemoryStatus,
    pub scope: MemoryScope,
    pub structured_json: serde_json::Value,
}

impl MemoryCandidate {
    fn new(
        kind: MemoryKind,
        subject: impl Into<String>,
        content: impl Into<String>,
        confidence: f64,
        context: &ExtractionContext,
        structured_json: serde_json::Value,
    ) -> Self {
        Self {
            kind,
            subject: subject.into(),
            content: content.into(),
            confidence: confidence.clamp(0.0, 1.0),
            source: MemorySource::Usage,
            status: MemoryStatus::Candidate,
            scope: context.scope(),
            structured_json,
        }
    }
}

/// Replace literal values and bind parameters with a single `?` placeholder.
///
/// Numbers, quoted strings, `IN (...)` lists, `$1`, `:name`, and `?` all
/// collapse to the same shape. Quoted identifiers are preserved because they
/// name schema objects, not data.
pub fn normalize_sql_literals(sql: &str) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        match ch {
            '\'' => {
                index += 1;
                while index < chars.len() {
                    if chars[index] == '\'' {
                        if chars.get(index + 1) == Some(&'\'') {
                            index += 2;
                            continue;
                        }
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                out.push('?');
            }
            '"' | '`' => {
                let quote = ch;
                out.push(ch);
                index += 1;
                while index < chars.len() {
                    out.push(chars[index]);
                    if chars[index] == quote {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
            }
            '[' => {
                out.push(ch);
                index += 1;
                while index < chars.len() {
                    out.push(chars[index]);
                    if chars[index] == ']' {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
            }
            ':' if chars.get(index + 1) != Some(&':') => {
                let mut end = index + 1;
                while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                    end += 1;
                }
                if end > index + 1 {
                    out.push('?');
                    index = end;
                } else {
                    out.push(ch);
                    index += 1;
                }
            }
            '$' if chars.get(index + 1).is_some_and(char::is_ascii_digit) => {
                let mut end = index + 2;
                while end < chars.len() && chars[end].is_ascii_digit() {
                    end += 1;
                }
                out.push('?');
                index = end;
            }
            '?' => {
                out.push('?');
                index += 1;
            }
            '0'..='9' => {
                let previous_is_identifier =
                    index > 0 && (chars[index - 1].is_alphanumeric() || chars[index - 1] == '_');
                if previous_is_identifier {
                    out.push(ch);
                    index += 1;
                } else {
                    let mut end = index + 1;
                    while end < chars.len() && chars[end].is_ascii_digit() {
                        end += 1;
                    }
                    if chars.get(end) == Some(&'.')
                        && chars.get(end + 1).is_some_and(char::is_ascii_digit)
                    {
                        end += 1;
                        while end < chars.len() && chars[end].is_ascii_digit() {
                            end += 1;
                        }
                    }
                    out.push('?');
                    index = end;
                }
            }
            _ => {
                out.push(ch);
                index += 1;
            }
        }
    }
    IN_LIST_RE.replace_all(&out, "IN (?)").into_owned()
}

/// Whitespace/case-insensitive shape used for dedupe and frequency counting.
pub fn sql_fingerprint(sql: &str) -> String {
    normalize_sql_literals(sql)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// Human-readable summary of the first structural difference. Literal-only
/// edits normalize away, so they deliberately produce no correction.
pub fn summarize_sql_diff(original_sql: &str, new_sql: &str) -> String {
    let original = normalize_sql_literals(original_sql);
    let new = normalize_sql_literals(new_sql);
    if original.trim() == new.trim() {
        return "literal-only change (normalized SQL unchanged)".into();
    }
    let old_words: Vec<&str> = original.split_whitespace().collect();
    let new_words: Vec<&str> = new.split_whitespace().collect();
    let mismatch = old_words
        .iter()
        .zip(new_words.iter())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| old_words.len().min(new_words.len()));
    let start = mismatch.saturating_sub(3);
    let old_slice = old_words
        .get(start..(mismatch + 4).min(old_words.len()))
        .unwrap_or(&[])
        .join(" ");
    let new_slice = new_words
        .get(start..(mismatch + 4).min(new_words.len()))
        .unwrap_or(&[])
        .join(" ");
    format!(
        "{} → {}",
        truncate(&old_slice, 160),
        truncate(&new_slice, 160)
    )
}

/// Build a correction candidate. Returns `None` when the normalized SQL is
/// unchanged (literal-only edits are not learnable under PRD §6.3).
pub fn correction_candidate(
    original_sql: &str,
    new_sql: &str,
    context: &ExtractionContext,
) -> Option<MemoryCandidate> {
    let original = normalize_sql_literals(original_sql);
    let new = normalize_sql_literals(new_sql);
    if original.trim() == new.trim() {
        return None;
    }
    let diff = summarize_sql_diff(original_sql, new_sql);
    let subject = format!("Corrected SQL: {}", truncate(&diff, 90));
    let content = format!(
        "Prefer the corrected SQL shape.\nDiff: {diff}\nBefore: {}\nAfter: {}",
        truncate(&original, 700),
        truncate(&new, 700)
    );
    let mut candidate = MemoryCandidate::new(
        MemoryKind::Correction,
        subject,
        content,
        0.7,
        context,
        json!({
            "originalSql": original,
            "newSql": new,
            "diffSummary": diff,
            "connectionId": context.connection_id,
            "database": context.database,
        }),
    );
    candidate.source = MemorySource::Correction;
    Some(candidate)
}

fn add_candidate(
    out: &mut Vec<MemoryCandidate>,
    seen: &mut BTreeSet<String>,
    candidate: MemoryCandidate,
    dedupe_key: String,
) {
    if !seen.insert(dedupe_key) {
        return;
    }
    if candidate.subject.trim().is_empty() || candidate.content.trim().is_empty() {
        return;
    }
    out.push(candidate);
}

fn first_clause_end(segment: &str) -> usize {
    CLAUSE_STOP_RE
        .find(segment)
        .map(|found| found.start())
        .or_else(|| segment.find(';'))
        .unwrap_or(segment.len())
}

fn where_predicates(sql: &str) -> Vec<String> {
    let Some(captures) = WHERE_RE.captures(sql) else {
        return Vec::new();
    };
    let clause = captures.get(1).map(|m| m.as_str()).unwrap_or_default();
    let clause = &clause[..first_clause_end(clause)];
    PREDICATE_SPLIT_RE
        .split(clause)
        .map(str::trim)
        .filter(|predicate| !predicate.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn relationship_candidates(sql: &str, context: &ExtractionContext) -> Vec<MemoryCandidate> {
    let normalized = normalize_sql_literals(sql);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for join in JOIN_START_RE.find_iter(&normalized) {
        let rest = &normalized[join.end()..];
        let end = first_clause_end(rest);
        let segment = format!("JOIN {}", rest[..end].trim());
        let Some(captures) = JOIN_SEGMENT_RE.captures(&segment) else {
            continue;
        };
        let table = captures.get(1).map(|m| m.as_str()).unwrap_or_default();
        let condition = captures
            .get(3)
            .map(|m| m.as_str().trim())
            .unwrap_or_default();
        if table.is_empty() || condition.is_empty() {
            continue;
        }
        let relation = EQUALITY_RE
            .captures(condition)
            .map(|caps| {
                format!(
                    "{} -> {}",
                    caps.get(1).map(|m| m.as_str()).unwrap_or_default(),
                    caps.get(2).map(|m| m.as_str()).unwrap_or_default()
                )
            })
            .unwrap_or_else(|| format!("JOIN {table}"));
        let subject = format!("Relationship: {relation}");
        let content = format!("JOIN {table} ON {condition}");
        let candidate = MemoryCandidate::new(
            MemoryKind::Relationship,
            &subject,
            &content,
            0.65,
            context,
            json!({
                "feature": "join",
                "table": table,
                "condition": condition,
                "connectionId": context.connection_id,
                "database": context.database,
            }),
        );
        add_candidate(&mut out, &mut seen, candidate, subject);
    }
    out
}

fn negative_candidates(sql: &str, context: &ExtractionContext) -> Vec<MemoryCandidate> {
    let normalized = normalize_sql_literals(sql);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for predicate in where_predicates(&normalized) {
        let Some(captures) = NEGATIVE_PREDICATE_RE.captures(&predicate) else {
            continue;
        };
        let column = captures.get(1).map(|m| m.as_str()).unwrap_or_default();
        let operator = captures.get(2).map(|m| m.as_str()).unwrap_or_default();
        let value = captures
            .get(3)
            .map(|m| m.as_str().trim())
            .unwrap_or_default();
        if column.is_empty() || value.is_empty() {
            continue;
        }
        let predicate = format!("{column} {operator} {value}");
        let subject = format!("Avoid: {}", truncate(&predicate, 100));
        let candidate = MemoryCandidate::new(
            MemoryKind::Negative,
            &subject,
            format!("Negative filter observed: {predicate}"),
            0.6,
            context,
            json!({
                "feature": "negative-filter",
                "predicate": predicate,
                "connectionId": context.connection_id,
                "database": context.database,
            }),
        );
        add_candidate(&mut out, &mut seen, candidate, subject);
    }
    out
}

fn features(sql: &str) -> Vec<(String, String)> {
    let normalized = normalize_sql_literals(sql);
    let mut found = BTreeMap::<String, String>::new();
    for join in JOIN_START_RE.find_iter(&normalized) {
        let rest = &normalized[join.end()..];
        let end = first_clause_end(rest);
        let segment = format!("JOIN {}", rest[..end].trim());
        let Some(captures) = JOIN_SEGMENT_RE.captures(&segment) else {
            continue;
        };
        let condition = captures
            .get(3)
            .map(|m| m.as_str().trim())
            .unwrap_or_default();
        if condition.is_empty() {
            continue;
        }
        let key = format!("join:{}", sql_fingerprint(condition));
        found
            .entry(key)
            .or_insert_with(|| format!("JOIN ON {condition}"));
    }
    for predicate in where_predicates(&normalized) {
        if FILTER_PREDICATE_RE.captures(&predicate).is_none() {
            continue;
        }
        let key = format!("filter:{}", sql_fingerprint(&predicate));
        found
            .entry(key)
            .or_insert_with(|| format!("filter {predicate}"));
    }
    for captures in EXPRESSION_RE.captures_iter(&normalized) {
        let function = captures.get(1).map(|m| m.as_str()).unwrap_or_default();
        let arguments = captures
            .get(2)
            .map(|m| m.as_str().trim())
            .unwrap_or_default();
        let expression = format!("{function}({arguments})");
        let key = format!("expr:{}", sql_fingerprint(&expression));
        found
            .entry(key)
            .or_insert_with(|| format!("expression {expression}"));
    }
    found.into_iter().collect()
}

fn pattern_candidates(
    current_sql: &str,
    history: &[SqlHistorySample],
    limits: ExtractionLimits,
    context: &ExtractionContext,
    now: DateTime<Utc>,
) -> Vec<MemoryCandidate> {
    let window_days = limits.window_days.clamp(1, 365);
    let min_occurrences = limits.min_occurrences.clamp(1, 100);
    let cutoff = now - Duration::days(i64::from(window_days));
    let mut samples: Vec<SqlHistorySample> = history
        .iter()
        .filter(|sample| sample.observed_at >= cutoff && sample.observed_at <= now)
        .cloned()
        .collect();
    let current_fingerprint = sql_fingerprint(current_sql);
    let current_observed = history.iter().any(|sample| {
        sql_fingerprint(&sample.sql) == current_fingerprint && sample.observed_at == now
    });
    if !current_observed {
        samples.push(SqlHistorySample {
            sql: current_sql.to_string(),
            observed_at: now,
        });
    }

    let mut counts = BTreeMap::<String, (String, BTreeSet<usize>)>::new();
    for (sample_index, sample) in samples.iter().enumerate() {
        for (key, label) in features(&sample.sql) {
            counts
                .entry(key)
                .or_insert_with(|| (label, BTreeSet::new()))
                .1
                .insert(sample_index);
        }
    }

    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (key, (label, occurrences)) in counts {
        if occurrences.len() < min_occurrences as usize {
            continue;
        }
        let count = occurrences.len();
        let subject = format!("Repeated pattern: {}", truncate(&label, 100));
        let candidate = MemoryCandidate::new(
            MemoryKind::Pattern,
            &subject,
            format!("{label} appeared {count} times in the last {window_days} days."),
            (0.55 + 0.05 * count as f64).min(0.9),
            context,
            json!({
                "feature": "repeated-pattern",
                "fingerprint": key,
                "occurrences": count,
                "windowDays": window_days,
                "connectionId": context.connection_id,
                "database": context.database,
            }),
        );
        add_candidate(&mut out, &mut seen, candidate, key);
    }
    out
}

/// Extract relationship, negative, and repeated-pattern candidates from the
/// current SQL plus host-supplied execution history.
pub fn extract_candidates(
    sql: &str,
    context: &ExtractionContext,
    history: &[SqlHistorySample],
    limits: ExtractionLimits,
    now: DateTime<Utc>,
) -> Vec<MemoryCandidate> {
    let mut out = relationship_candidates(sql, context);
    out.extend(negative_candidates(sql, context));
    out.extend(pattern_candidates(sql, history, limits, context, now));
    let mut seen = BTreeSet::new();
    out.retain(|candidate| {
        let key = format!(
            "{:?}:{}:{}",
            candidate.kind,
            candidate.scope.kind.as_str(),
            candidate.subject.to_lowercase()
        );
        seen.insert(key)
    });
    out
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExtractionEnvelope {
    candidates: Vec<ExtractionJsonCandidate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExtractionJsonCandidate {
    kind: String,
    subject: String,
    content: String,
    confidence: Option<f64>,
}

/// Strict model-output parser. Any malformed JSON, unknown field, unsupported
/// kind, or empty subject/content drops the whole response rather than guessing.
pub fn parse_extraction_json(raw: &str, context: &ExtractionContext) -> Vec<MemoryCandidate> {
    let envelope: ExtractionEnvelope = match serde_json::from_str(raw) {
        Ok(envelope) => envelope,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for item in envelope.candidates {
        let kind = match item.kind.as_str() {
            "relationship" => MemoryKind::Relationship,
            "pattern" => MemoryKind::Pattern,
            "negative" => MemoryKind::Negative,
            _ => return Vec::new(),
        };
        if item.subject.trim().is_empty() || item.content.trim().is_empty() {
            return Vec::new();
        }
        let confidence = item.confidence.unwrap_or(0.55);
        if !(0.0..=1.0).contains(&confidence) {
            return Vec::new();
        }
        out.push(MemoryCandidate::new(
            kind,
            item.subject.trim(),
            item.content.trim(),
            confidence,
            context,
            json!({
                "feature": "model-json",
                "connectionId": context.connection_id,
                "database": context.database,
            }),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, day, 0, 0, 0).unwrap()
    }

    fn context() -> ExtractionContext {
        ExtractionContext {
            connection_id: Some("conn-1".into()),
            database: Some("sales".into()),
        }
    }

    #[test]
    fn normalization_collapses_numbers_strings_in_lists_and_parameters() {
        let sql = "SELECT * FROM orders WHERE id IN (1, 2, 3) AND name = 'O''Brien' AND amount = 12.50 AND owner = $1 AND active = :active AND code = ?";
        let normalized = normalize_sql_literals(sql);
        assert!(normalized.contains("IN (?)"));
        assert!(normalized.contains("name = ?"));
        assert!(normalized.contains("amount = ?"));
        assert!(normalized.contains("owner = ?"));
        assert!(normalized.contains("active = ?"));
        assert!(normalized.contains("code = ?"));
        assert!(!normalized.contains("O''Brien"));
        assert!(!normalized.contains("12.50"));
        assert_eq!(normalized, normalize_sql_literals(&normalized));
    }

    #[test]
    fn literal_only_changes_do_not_create_correction_candidates() {
        assert!(correction_candidate(
            "SELECT * FROM orders WHERE id = 1",
            "SELECT * FROM orders WHERE id = 2",
            &context()
        )
        .is_none());
        let candidate = correction_candidate(
            "SELECT * FROM orders o JOIN users u ON o.user_id = u.id",
            "SELECT * FROM orders o JOIN users u ON o.customer_id = u.id",
            &context(),
        )
        .expect("structural correction");
        assert_eq!(candidate.kind, MemoryKind::Correction);
        assert_eq!(candidate.status, MemoryStatus::Candidate);
        assert_eq!(candidate.source, MemorySource::Correction);
        assert!(candidate.content.contains("Diff:"));
        assert_eq!(candidate.structured_json["connectionId"], "conn-1");
        assert_eq!(candidate.structured_json["database"], "sales");
    }

    #[test]
    fn repeated_join_filter_and_expression_need_three_occurrences_in_window() {
        let sql = "SELECT DATE_TRUNC('week', paid_at), SUM(amount) FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status != 'legacy' GROUP BY 1";
        let sample = |day| SqlHistorySample {
            sql: sql.into(),
            observed_at: at(day),
        };
        let history = [sample(1), sample(2), sample(3), sample(20)];
        let candidates = extract_candidates(
            sql,
            &context(),
            &history,
            ExtractionLimits::default(),
            at(5),
        );
        assert!(candidates
            .iter()
            .any(|candidate| candidate.kind == MemoryKind::Pattern));
        let pattern = candidates
            .iter()
            .find(|candidate| candidate.kind == MemoryKind::Pattern)
            .unwrap();
        assert_eq!(pattern.status, MemoryStatus::Candidate);
        assert!(pattern.structured_json["occurrences"].as_u64().unwrap() >= 3);

        // Only two in-window samples (day 20 is outside the 14-day window).
        let few = extract_candidates(
            sql,
            &context(),
            &[sample(1)],
            ExtractionLimits::default(),
            at(5),
        );
        assert!(!few
            .iter()
            .any(|candidate| candidate.kind == MemoryKind::Pattern));
    }

    #[test]
    fn extracts_relationships_and_negative_filters_and_deduplicates() {
        let sql = "SELECT * FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status != 'legacy' AND o.deleted_at IS NULL";
        let candidates =
            extract_candidates(sql, &context(), &[], ExtractionLimits::default(), at(1));
        assert_eq!(
            candidates
                .iter()
                .filter(|candidate| candidate.kind == MemoryKind::Relationship)
                .count(),
            1
        );
        let negative = candidates
            .iter()
            .find(|candidate| candidate.kind == MemoryKind::Negative)
            .expect("negative filter");
        assert!(negative.content.contains("status !="));
        let relationships: Vec<_> = candidates
            .iter()
            .filter(|candidate| candidate.kind == MemoryKind::Relationship)
            .collect();
        assert_eq!(relationships.len(), 1, "dedupe keeps one JOIN candidate");
    }

    #[test]
    fn strict_json_parser_discards_malformed_unknown_and_invalid_payloads() {
        let valid = r#"{"candidates":[{"kind":"relationship","subject":"orders.user_id -> users.id","content":"Join orders to users","confidence":0.8}]}"#;
        assert_eq!(parse_extraction_json(valid, &context()).len(), 1);
        assert!(parse_extraction_json("not json", &context()).is_empty());
        let unknown_field =
            r#"{"candidates":[{"kind":"pattern","subject":"x","content":"y","extra":1}]}"#;
        assert!(parse_extraction_json(unknown_field, &context()).is_empty());
        let bad_kind = r#"{"candidates":[{"kind":"preference","subject":"x","content":"y"}]}"#;
        assert!(parse_extraction_json(bad_kind, &context()).is_empty());
        let empty = r#"{"candidates":[{"kind":"pattern","subject":" ","content":"y"}]}"#;
        assert!(parse_extraction_json(empty, &context()).is_empty());
    }

    #[test]
    fn diff_summary_shows_first_structural_change() {
        let diff = summarize_sql_diff(
            "SELECT * FROM orders JOIN users ON orders.user_id = users.id",
            "SELECT * FROM orders JOIN users ON orders.customer_id = users.id",
        );
        assert!(diff.contains('→'));
        assert!(diff.contains("user") || diff.contains("customer"));
    }
}
