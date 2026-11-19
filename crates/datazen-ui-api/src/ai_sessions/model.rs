//! Mirror types and pure derivations for the AI chat session store.
//!
//! Ported from React `packages/frontend/src/stores/ai/sessions.ts` (tasklist
//! T040, split marker: this file owns the sessions.ts semantics only — the
//! sibling `stores/ai/streaming.ts`, `stores/ai/types.ts` and `stores/aiStore.ts`
//! are separate tasks). sessions.ts imports its vocabulary from
//! `types/ai.ts` and `lib/aiSkills.ts`. **Since T042** (`types/ai.ts` port,
//! [`crate::ai_types`]) the message/session/summary/detail/upsert vocabulary
//! is defined there in full and re-exported by this module — one source, the
//! wire profiles byte-identical to the T040 mirrors:
//!
//! - `types/ai.ts` message/session/summary/detail/upsert vocabulary →
//!   [`crate::ai_types`] (T042). Re-exported here: [`AiChatMessage`],
//!   [`AiChatSession`], [`AiSessionSummary`], [`AiSessionDetail`],
//!   [`AiSessionMessage`], [`AiSessionMessageInput`], [`AiSessionUpsertInput`],
//!   [`AiMessageRole`], [`AiMessageStatus`], [`AiLoadedSkill`].
//! - `lib/aiSkills.ts` `normalizeLoadedSkills` → T230. Mirrored here as
//!   [`normalize_loaded_skills`] (sessions.ts calls it on every restore).
//! - the live store half (`stores/aiStore.ts` + `stores/ai/types.ts`) → T128;
//!   the streaming fields on [`AiChatSession`] exist because sessions.ts
//!   *constructs* sessions with fixed defaults, but their transitions are
//!   T128's, not this file's.
//! - `commands/ai.ts` (`aiCommands.sessions*`) → the commands port task;
//!   here it is the [`super::store::AiSessionBackend`] seam.
//!
//! ## `null` / `undefined` folding
//!
//! JS distinguishes `null` from `undefined`. Within sessions.ts the
//! distinction is observable only where code branches on `!== undefined`
//! *and* assigns through — [`message_metadata`](super::mapping::message_metadata)
//! on the per-message pass-through fields (a `null` reasoning lands in the
//! AppDb metadata; an absent one does not). The typed T042 twin cannot carry
//! an explicit null on those fields (`reasoning?: string` has no null arm),
//! so a stored `null` folds to `None` and the metadata key then stays
//! absent — the fold is inert for this file's logic (nothing branches on
//! key presence of the pass-through keys) and is pinned by the tests below.
//! Everywhere else (`connectionId`, `database`,
//! `metadata`, inert streaming fields) sessions.ts only checks truthiness or
//! `??`-coalesces, so the mirrors fold `null`/`undefined` into `None`; the
//! folds are inert for this file's logic and are noted on the fields.

use serde_json::Value;

pub use crate::ai_types::{
    AiChatMessage, AiChatSession, AiLoadedSkill, AiMessageRole, AiMessageStatus,
    AiSessionDetail, AiSessionMessage, AiSessionMessageInput, AiSessionSummary,
    AiSessionUpsertInput,
};

/// React `export type SessionKey = string` — the `(identity, database)`
/// isolation key. Newtype-free alias: keys are built, compared and stored as
/// plain strings.
pub type SessionKey = String;

/// React `SessionScope`: the connection/database coordinates a chat session
/// is isolated to. `connectionId` and `dbSessionId` are different dimensions
/// (AGENTS.md ID discipline): the former is the persistent config identity,
/// the latter the runtime database-session handle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionScope {
    /// Persistent connection identity (`connections.id`).
    pub connection_id: Option<String>,
    /// Runtime database session handle; distinct from `connection_id`.
    pub db_session_id: Option<String>,
    /// Database name inside the connection.
    pub database: Option<String>,
}

impl SessionScope {
    /// Builds a scope. Mirrors the object literals sessions.ts receives from
    /// its callers (`{ connectionId, dbSessionId, database }`).
    pub fn new(
        connection_id: Option<String>,
        db_session_id: Option<String>,
        database: Option<String>,
    ) -> Self {
        Self { connection_id, db_session_id, database }
    }
}

/// React `SessionRestoreResult`.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRestoreResult {
    /// The restored session — always `Some`; React never returns a null
    /// session (the fallbacks mint a new one), the `Option` only mirrors
    /// the field's shape.
    pub session: Option<AiChatSession>,
    pub summaries: Vec<AiSessionSummary>,
    /// A migration or backend failure that must stay visible to the user.
    pub migration_error: Option<String>,
    /// Number of legacy conversations moved into AppDb on this load.
    pub migration_success: usize,
    pub backend_available: bool,
}

/// React `SessionPersistResult`.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionPersistResult {
    pub ok: bool,
    pub error: Option<String>,
    /// The backend upsert's returned detail; `None` when the write failed.
    pub detail: Option<AiSessionDetail>,
}

/// React `deleteChatSession`'s `{ ok: boolean; error?: string }` return.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionDeleteResult {
    pub ok: bool,
    pub error: Option<String>,
}

/// React `SessionExport`.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionExport {
    pub content: String,
    pub filename: String,
    pub mime: String,
}

/// React `migrateLocalSessionsToBackend`'s `{ migrated, error }` return.
#[derive(Clone, Debug, PartialEq)]
pub struct MigrationOutcome {
    pub migrated: usize,
    pub error: Option<String>,
}

/// Export formats of `exportChatSession` / `aiCommands.sessionsExport`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Markdown,
}

impl ExportFormat {
    /// The wire spelling of `format: 'json' | 'markdown'`.
    pub fn as_str(self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Markdown => "markdown",
        }
    }

    /// The file extension `exportFilename` appends.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Markdown => "md",
        }
    }

    /// The MIME type `exportChatSession` reports.
    pub fn mime(self) -> &'static str {
        match self {
            ExportFormat::Json => "application/json",
            ExportFormat::Markdown => "text/markdown",
        }
    }
}

/// React `computeSessionKey(connectionId?, dbSessionId?, database?)`.
///
/// Priority: `connectionId` > `dbSessionId` > `'default'`; the database
/// segment is empty when absent. `||` (not `??`) semantics: an empty-string
/// connectionId falls through to the next candidate, an empty-string
/// database segment yields the trailing `::` — exactly React's template.
pub fn compute_session_key(
    connection_id: Option<&str>,
    db_session_id: Option<&str>,
    database: Option<&str>,
) -> SessionKey {
    let identity = connection_id
        .filter(|id| !id.is_empty())
        .or_else(|| db_session_id.filter(|id| !id.is_empty()))
        .unwrap_or("default");
    let database = database.unwrap_or("");
    format!("{identity}::{database}")
}

/// React's private `splitLegacyKey(key)`: split at the **first** `::` and
/// map `default`/empty segments to `None`.
///
/// `key.split('::')` in JS splits at every `::`, but the two-element
/// destructure only ever reads the first two parts, so a key like
/// `a::b::c` yields connection `a` and database `b` — mirrored by splitting
/// at the first separator only.
pub fn split_legacy_key(key: &str) -> SessionScope {
    // JS `key.split('::')` splits at every separator; the two-element
    // destructure reads only the first two segments, so `a::b::c` yields
    // connection `a` and database `b` (the rest is dropped).
    let mut segments = key.split("::");
    let connection = segments.next().unwrap_or("");
    let database = segments.next();
    // React: `connection === 'default' ? undefined : connection` — an empty
    // connection segment stays `''` (only the literal `default` is lifted).
    let connection_id =
        if connection == "default" { None } else { Some(connection.to_string()) };
    // React: `database || undefined`.
    let database = database.filter(|db| !db.is_empty()).map(str::to_string);
    SessionScope {
        connection_id,
        db_session_id: None,
        database,
    }
}

/// React's private `truncateBytes(str, maxBytes)`:
/// `new TextEncoder().encode(str).length <= max` guard, then drop trailing
/// characters until under the limit.
///
/// Rust `String`s are always valid UTF-8, so the byte length is
/// `as_bytes().len()`. JS strings can additionally carry lone surrogates
/// (`TextEncoder` writes them as the 3-byte replacement char); Rust cannot
/// represent those, so the divergence is unreachable from this port.
pub fn truncate_bytes(text: &str, max_bytes: usize) -> String {
    if text.as_bytes().len() <= max_bytes {
        return text.to_string();
    }
    let mut result = text.to_string();
    while !result.is_empty() && result.as_bytes().len() > max_bytes {
        result.pop();
    }
    result
}

/// `new Date(ms).toISOString()` — the pure half of React's clock usage.
///
/// Produces the UTC `YYYY-MM-DDTHH:MM:SS.sssZ` form for epoch milliseconds:
/// fractional input is truncated toward zero first (`TimeClip`), the day
/// split is floored (negative epochs land before 1970), and out-of-range
/// inputs (`|ms| > 8.64e15`) return `None` — the moment JS produces an
/// Invalid Date and `toISOString()` throws `RangeError: Invalid time value`.
/// Years beyond ±9999 use ECMAScript's extended-year spelling (`+275760-…`,
/// `-000001-…`).
///
/// `Date.now()`-style *now* reads are not derived here: the host injects
/// `now_iso` through the [`super::store::SessionClock`] seam so tests are
/// deterministic; this function exists for the one place React converts a
/// *stored* timestamp (legacy migration's `lastAccessAt`).
pub fn iso_from_epoch_ms(ms: f64) -> Option<String> {
    if !ms.is_finite() || ms.abs() > 8.64e15 {
        return None;
    }
    let clipped = ms.trunc();
    let days = (clipped / 86_400_000.0).floor();
    let millis_of_day = clipped - days * 86_400_000.0;
    let (year, month, day) = civil_from_days(days as i64);
    let millis_of_day = millis_of_day as i64;
    let hour = millis_of_day / 3_600_000;
    let minute = (millis_of_day / 60_000) % 60;
    let second = (millis_of_day / 1_000) % 60;
    let millis = millis_of_day % 1_000;
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else if year > 9999 {
        format!("+{year}")
    } else {
        format!("-{:06}", -year)
    };
    Some(format!(
        "{year}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
    ))
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 → (y, m, d).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { y + 1 } else { y }, month, day)
}

/// React `lib/aiSkills.ts` `normalizeLoadedSkills` (T230 owns the file; the
/// function is mirrored because every restore path calls it).
///
/// Reads session metadata written by this version or an older one: a
/// non-array (or `None`) yields `[]`; entries must be objects whose `name`
/// is a non-blank string; `id`/`source`/`loadedAt` fall back when absent or
/// wrongly typed, with `loadedAt` defaulting to the injected `now_iso`
/// (React's `new Date().toISOString()`).
pub fn normalize_loaded_skills(value: Option<&Value>, now_iso: &str) -> Vec<AiLoadedSkill> {
    let Some(Value::Array(entries)) = value else {
        return Vec::new();
    };
    let mut skills = Vec::new();
    for entry in entries {
        // React: `!entry || typeof entry !== 'object'` drops nulls and
        // non-objects (arrays survive the check but fail the `name` one).
        let Some(record) = entry.as_object() else {
            continue;
        };
        let Some(name) = record.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.trim().is_empty() {
            continue;
        }
        skills.push(AiLoadedSkill {
            id: record.get("id").and_then(Value::as_str).map(str::to_string),
            name: name.to_string(),
            source: record.get("source").and_then(Value::as_str).map(str::to_string),
            loaded_at: record
                .get("loadedAt")
                .and_then(Value::as_str)
                .unwrap_or(now_iso)
                .to_string(),
        });
    }
    skills
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_session_key_prefers_the_connection_id() {
        // React: 'conn-1' wins over 'sess-1'.
        assert_eq!(
            compute_session_key(Some("conn-1"), Some("sess-1"), Some("mydb")),
            "conn-1::mydb"
        );
    }

    #[test]
    fn compute_session_key_falls_back_to_the_db_session_id() {
        assert_eq!(compute_session_key(None, Some("sess-1"), Some("mydb")), "sess-1::mydb");
    }

    #[test]
    fn compute_session_key_defaults_the_identity() {
        assert_eq!(compute_session_key(None, None, Some("mydb")), "default::mydb");
    }

    #[test]
    fn compute_session_key_handles_missing_and_empty_database() {
        assert_eq!(compute_session_key(Some("conn-1"), Some("sess-1"), None), "conn-1::");
        assert_eq!(compute_session_key(Some("conn-1"), Some("sess-1"), Some("")), "conn-1::");
    }

    #[test]
    fn compute_session_key_uses_js_or_semantics_for_empty_ids() {
        // React's `connectionId || dbSessionId || 'default'` — an empty
        // connectionId falls through (||, not ??).
        assert_eq!(compute_session_key(Some(""), Some("sess-1"), Some("db")), "sess-1::db");
        assert_eq!(compute_session_key(Some(""), None, Some("db")), "default::db");
    }

    #[test]
    fn compute_session_key_is_stable() {
        assert_eq!(
            compute_session_key(Some("a"), Some("b"), Some("c")),
            compute_session_key(Some("a"), Some("b"), Some("c"))
        );
    }

    #[test]
    fn compute_session_key_keeps_one_bucket_across_db_session_id_changes() {
        // Same connection, new runtime session: same bucket — the runtime
        // handle must not split a persistent connection's history.
        assert_eq!(
            compute_session_key(Some("conn-1"), Some("sess-a"), Some("db")),
            compute_session_key(Some("conn-1"), Some("sess-b"), Some("db"))
        );
    }

    #[test]
    fn compute_session_key_separates_connections_and_databases() {
        // Any differing dimension must not share a bucket.
        assert_ne!(
            compute_session_key(Some("conn-1"), Some("sess-1"), Some("db")),
            compute_session_key(Some("conn-2"), Some("sess-1"), Some("db"))
        );
        assert_ne!(
            compute_session_key(Some("conn-1"), Some("sess-1"), Some("db")),
            compute_session_key(Some("conn-1"), Some("sess-1"), Some("other"))
        );
        assert_ne!(
            compute_session_key(Some("conn-1"), Some("sess-1"), Some("db")),
            compute_session_key(None, Some("sess-1"), Some("db"))
        );
    }

    #[test]
    fn split_legacy_key_splits_at_the_first_separator_only() {
        // `a::b::c` destructure keeps only a/b (React takes the first two
        // parts of split('::')).
        let scope = split_legacy_key("a::b::c");
        assert_eq!(scope.connection_id.as_deref(), Some("a"));
        assert_eq!(scope.database.as_deref(), Some("b"));
    }

    #[test]
    fn split_legacy_key_maps_default_and_empty_to_none() {
        let scope = split_legacy_key("default::db");
        assert_eq!(scope.connection_id, None);
        assert_eq!(scope.database.as_deref(), Some("db"));

        let scope = split_legacy_key("conn-1::");
        assert_eq!(scope.connection_id.as_deref(), Some("conn-1"));
        assert_eq!(scope.database, None);

        let scope = split_legacy_key("default::");
        assert_eq!(scope.connection_id, None);
        assert_eq!(scope.database, None);

        let scope = split_legacy_key("noseparator");
        assert_eq!(scope.connection_id.as_deref(), Some("noseparator"));
        assert_eq!(scope.database, None);
    }

    #[test]
    fn truncate_bytes_is_identity_under_the_limit() {
        assert_eq!(truncate_bytes("hello", 4096), "hello");
        assert_eq!(truncate_bytes("", 4096), "");
    }

    #[test]
    fn truncate_bytes_drops_trailing_characters_until_under_the_limit() {
        // 6 ASCII chars, 5-byte budget → "hello".
        assert_eq!(truncate_bytes("hello!", 5), "hello");
        // Multi-byte chars drop whole chars, not bytes: "héllo" is 6 bytes;
        // with a 5-byte budget the 'o' still has to go.
        assert_eq!(truncate_bytes("héllo", 5), "héll");
        // "hé" is 3 bytes; a 1-byte budget still leaves the 1-byte "h" (the
        // loop stops once the budget is met, like React's).
        assert_eq!(truncate_bytes("hé", 1), "h");
        assert_eq!(truncate_bytes("hé", 2), "h");
    }

    #[test]
    fn iso_from_epoch_ms_formats_the_epoch_and_known_dates() {
        assert_eq!(iso_from_epoch_ms(0.0).as_deref(), Some("1970-01-01T00:00:00.000Z"));
        assert_eq!(iso_from_epoch_ms(1000.0).as_deref(), Some("1970-01-01T00:00:01.000Z"));
        // 2026-01-01T00:00:00Z.
        assert_eq!(
            iso_from_epoch_ms(1_767_225_600_000.0).as_deref(),
            Some("2026-01-01T00:00:00.000Z")
        );
    }

    #[test]
    fn iso_from_epoch_ms_floors_negative_epochs_before_1970() {
        assert_eq!(iso_from_epoch_ms(-1.0).as_deref(), Some("1969-12-31T23:59:59.999Z"));
        assert_eq!(iso_from_epoch_ms(-1.5).as_deref(), Some("1969-12-31T23:59:59.999Z"));
    }

    #[test]
    fn iso_from_epoch_ms_truncates_fractional_milliseconds() {
        // TimeClip truncates toward zero.
        assert_eq!(iso_from_epoch_ms(1000.7).as_deref(), Some("1970-01-01T00:00:01.000Z"));
        assert_eq!(iso_from_epoch_ms(-1000.7).as_deref(), Some("1969-12-31T23:59:59.000Z"));
    }

    #[test]
    fn iso_from_epoch_ms_rejects_non_finite_and_out_of_range_inputs() {
        assert_eq!(iso_from_epoch_ms(f64::NAN), None);
        assert_eq!(iso_from_epoch_ms(f64::INFINITY), None);
        // Beyond Date's ±8.64e15 ms range → Invalid Date → toISOString throws.
        assert_eq!(iso_from_epoch_ms(8.64e15 + 1.0), None);
        assert_eq!(iso_from_epoch_ms(-8.64e15 - 1.0), None);
        assert_eq!(iso_from_epoch_ms(8.64e15).as_deref(), Some("+275760-09-13T00:00:00.000Z"));
    }

    #[test]
    fn normalize_loaded_skills_reads_valid_entries_and_fills_defaults() {
        let value = serde_json::json!([
            { "id": "sk-1", "name": "sql helper", "source": "builtin", "loadedAt": "2026-01-01T00:00:00.000Z" },
            { "name": "custom" }
        ]);
        let skills = normalize_loaded_skills(Some(&value), "2026-02-02T00:00:00.000Z");
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].id.as_deref(), Some("sk-1"));
        assert_eq!(skills[0].name, "sql helper");
        assert_eq!(skills[0].source.as_deref(), Some("builtin"));
        assert_eq!(skills[0].loaded_at, "2026-01-01T00:00:00.000Z");
        // Missing loadedAt falls back to the injected now (React: new Date()).
        assert_eq!(skills[1].id, None);
        assert_eq!(skills[1].loaded_at, "2026-02-02T00:00:00.000Z");
    }

    #[test]
    fn normalize_loaded_skills_drops_invalid_entries() {
        // Non-array input (None, null, object) → [].
        assert!(normalize_loaded_skills(None, "now").is_empty());
        assert!(normalize_loaded_skills(Some(&Value::Null), "now").is_empty());
        assert!(
            normalize_loaded_skills(Some(&serde_json::json!({ "name": "x" })), "now").is_empty()
        );
        // Null entries, non-objects, blank/missing names, wrong id/source
        // types all drop; the rest survive.
        let value = serde_json::json!([
            null,
            5,
            { "name": "   " },
            { "id": 7, "name": "kept", "source": 3 },
            { "name": "also-kept", "loadedAt": 9 }
        ]);
        let skills = normalize_loaded_skills(Some(&value), "now");
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "kept");
        assert_eq!(skills[0].id, None);
        assert_eq!(skills[0].source, None);
        assert_eq!(skills[1].loaded_at, "now");
    }

    #[test]
    fn export_format_spellings_match_the_react_wire_names() {
        assert_eq!(ExportFormat::Json.as_str(), "json");
        assert_eq!(ExportFormat::Markdown.as_str(), "markdown");
        assert_eq!(ExportFormat::Json.extension(), "json");
        assert_eq!(ExportFormat::Markdown.extension(), "md");
        assert_eq!(ExportFormat::Json.mime(), "application/json");
        assert_eq!(ExportFormat::Markdown.mime(), "text/markdown");
    }
}
