//! Post-execution AI-memory hooks — ported from React
//! `packages/frontend/src/lib/aiMemoryCorrection.ts` (tasklist T179,
//! 63 lines).
//!
//! The query editor fires both hooks from its execution-complete seam
//! (React `QueryPanel`): after a successful run it asks the host to learn
//! the difference between the AI's original SQL and the SQL that actually
//! ran ([`submit_memory_correction_if_needed`]), and to mark the memories
//! attached to objects the executed DDL dropped stale
//! ([`mark_stale_for_schema_drops`]). The host owns normalization, the
//! auto-learn switch, conflict handling and candidate status; the UI only
//! supplies the two SQL strings — and learning must never make a
//! successful query look failed, so both paths swallow transport errors
//! (the React doc comments' own contract).
//!
//! ## The seams (each names its owning migration task)
//!
//! | React import | Port seam | Owner |
//! | --- | --- | --- |
//! | `commands/ai` (`aiCommands.memoryCorrectionCandidate` / `aiCommands.memoryMarkStale`) | [`AiMemoryCommands`] — `Result` per the T040 await→`Result` precedent | commands/ai port task |
//! | `./sqlSchemaDropTargets` (`parseSchemaDropTargets`) | already ported (T619): [`parse_schema_drop_targets`](crate::sql_schema_drop_targets::parse_schema_drop_targets) | done |
//! | `../types` (`AiMemoryEntry`) | already ported (T042): [`AiMemoryEntry`](crate::ai_types::AiMemoryEntry) | done |
//!
//! ## Mirror rules applied
//!
//! - **Async → sync fold (the T040 precedent)**: each `await`ed invoke is
//!   one seam call returning `Result<T, String>` (`Err` is the rejection's
//!   display message — the `AiSessionBackend` convention), and the module's
//!   two `try`/`catch` blocks become folds to the swallowed outcome.
//!   `Promise.all`'s concurrency is unobservable here — every per-target
//!   outcome is discarded and the calls share no state — so the port
//!   issues them sequentially, in the React array's order.
//! - `undefined`-optional fields (`connectionId?: string`, `database?:
//!   string`, and the drop target's `schema?`/`table?`/`column?`) →
//!   `Option<String>` forwarded **verbatim**: never trimmed, never
//!   defaulted, `None` landing exactly where React writes
//!   `column: undefined`.
//! - JS `.trim()` → [`str::trim`] (the two whitespace vocabularies differ
//!   only on exotic code points); the trimmed values feed BOTH the
//!   emptiness/sameness guard and the wire input — React's two locals,
//!   computed once up front.
//! - The command's own `null` return (`AiMemoryEntry | null`) is a third
//!   fold, distinct from both the guard (no seam call at all) and a
//!   rejection (also `None`): `Ok(None)` forwards as `None`.
//! - No serde on this module's own shapes: `MemoryCorrectionSubmission`
//!   and `SchemaStaleSubmission` are in-process call parameters the
//!   editor builds locally — nothing that crosses a storage/IPC boundary.
//!   The wire shapes (`AiMemoryCorrectionInput`, `AiMemoryStaleInput`,
//!   `AiMemoryEntry`) already live in `ai_types` as camelCase serde types,
//!   and this module reuses them rather than forking them.

use crate::ai_types::{AiMemoryCorrectionInput, AiMemoryEntry, AiMemoryStaleInput};
use crate::sql_schema_drop_targets::parse_schema_drop_targets;

/// The `aiCommands.memory*` seam — the two calls
/// `lib/aiMemoryCorrection.ts` makes on the `commands/ai.ts` singleton
/// (commands segment, not ported).
///
/// React's promises map to `Result` exactly the way the T040 sessions port
/// mapped `aiCommands.sessions*`: `Ok` is the invoke's resolved value,
/// `Err(String)` is the rejection's display message.
/// `memory_correction_candidate` resolves with `AiMemoryEntry | null` —
/// the host may decline to learn (`Ok(None)`), so the correction fold has
/// a third arm besides the rejection swallow. `memory_mark_stale` resolves
/// with the entries it marked; this module discards that list (React
/// awaits it and drops it too). **Ownership**: the commands/ai port task
/// owns the real adapter (the Tauri invoke / HTTP route over
/// `ai_memory_correction_candidate` and `ai_memory_mark_stale`); when it
/// lands it should implement this seam rather than fork it.
pub trait AiMemoryCommands {
    /// `aiCommands.memoryCorrectionCandidate(input)` — await → `Result`.
    fn memory_correction_candidate(
        &self,
        input: &AiMemoryCorrectionInput,
    ) -> Result<Option<AiMemoryEntry>, String>;
    /// `aiCommands.memoryMarkStale(input)` — await → `Result`.
    fn memory_mark_stale(&self, input: &AiMemoryStaleInput) -> Result<Vec<AiMemoryEntry>, String>;
}

/// React's `MemoryCorrectionSubmission` — what the editor hands the
/// correction hook: the AI's original SQL, the SQL that actually ran, and
/// the optional scope the run happened under.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryCorrectionSubmission {
    pub original_sql: String,
    pub current_sql: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
}

/// React's `SchemaStaleSubmission` — the executed SQL plus the optional
/// scope, for the host to mark attached memories stale.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaStaleSubmission {
    pub sql: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
}

/// React's `submitMemoryCorrectionIfNeeded`: submit a correction candidate
/// after a successful execution. The host owns normalization, the
/// auto-learn switch, conflict handling and candidate status; the UI only
/// supplies the original AI SQL and the SQL that actually ran.
///
/// Learning must never make a successful query look failed, so transport
/// errors are intentionally swallowed here — `Err` and the host's own
/// `null` both fold to `None`, as does the blank/unchanged guard (which
/// never calls the seam at all).
pub fn submit_memory_correction_if_needed<C: AiMemoryCommands>(
    commands: &C,
    submission: &MemoryCorrectionSubmission,
) -> Option<AiMemoryEntry> {
    // React: `const originalSql = submission.originalSql.trim();` (and the
    // current twin) — the trimmed locals feed both the guard and the input.
    let original_sql = submission.original_sql.trim().to_owned();
    let current_sql = submission.current_sql.trim().to_owned();
    // React: `if (!originalSql || !currentSql || originalSql ===
    // currentSql) return null;` — the empty-string falsy arms plus strict
    // sameness on the trimmed values.
    if original_sql.is_empty() || current_sql.is_empty() || original_sql == current_sql {
        return None;
    }
    // React: `aiCommands.memoryCorrectionCandidate({ originalSql, newSql:
    // currentSql, connectionId: submission.connectionId, database:
    // submission.database })` — the scope fields verbatim, never trimmed.
    let input = AiMemoryCorrectionInput {
        original_sql,
        new_sql: current_sql,
        connection_id: submission.connection_id.clone(),
        database: submission.database.clone(),
    };
    // React: `try { return await … } catch { return null }` — `.ok()`
    // swallows the rejection, `.flatten()` forwards the host's own `null`.
    commands.memory_correction_candidate(&input).ok().flatten()
}

/// React's `markStaleForSchemaDrops`: ask the host to mark memories
/// attached to dropped tables/columns stale. Staleness is a maintenance
/// hint; it never fails the user's DDL — each target's call is attempted
/// and its outcome (value and error alike) discarded, and one target's
/// transport failure never blocks the rest.
pub fn mark_stale_for_schema_drops<C: AiMemoryCommands>(
    commands: &C,
    submission: &SchemaStaleSubmission,
) {
    // React: `const targets = parseSchemaDropTargets(submission.sql); if
    // (targets.length === 0) return;` — no drop targets, no calls.
    let targets = parse_schema_drop_targets(&submission.sql);
    // React: `await Promise.all(targets.map(async (target) => { try {
    // await aiCommands.memoryMarkStale({ connectionId, database, schema:
    // target.schema, table: target.table, column: target.column }) }
    // catch {} }))` — every call is issued, every per-call outcome
    // swallowed; the concurrency is unobservable (nothing is shared
    // between the calls), so the port walks the array's order.
    for target in targets {
        let input = AiMemoryStaleInput {
            connection_id: submission.connection_id.clone(),
            database: submission.database.clone(),
            schema: target.schema,
            table: target.table,
            column: target.column,
        };
        // The resolved list is dropped exactly like the rejection: the
        // whole statement below is React's `try`/`catch` pair.
        let _ = commands.memory_mark_stale(&input);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_types::{
        AiMemoryKind, AiMemoryScopeKind, AiMemorySensitivity, AiMemorySource, AiMemoryStatus,
    };
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// The commands double the vitest `vi.mock('../../commands/ai')` plays:
    /// every call is recorded, and each call replays the scripted outcome
    /// in order. An exhausted script answers the seam's inert default
    /// (`Ok(None)` / `Ok(vec![])`), so a forgotten script never invents a
    /// failure.
    #[derive(Default)]
    struct RecordingCommands {
        correction_inputs: RefCell<Vec<AiMemoryCorrectionInput>>,
        stale_inputs: RefCell<Vec<AiMemoryStaleInput>>,
        correction_script: RefCell<VecDeque<Result<Option<AiMemoryEntry>, String>>>,
        stale_script: RefCell<VecDeque<Result<Vec<AiMemoryEntry>, String>>>,
    }

    impl RecordingCommands {
        fn script_correction(&self, outcome: Result<Option<AiMemoryEntry>, String>) {
            self.correction_script.borrow_mut().push_back(outcome);
        }
        fn script_stale(&self, outcome: Result<Vec<AiMemoryEntry>, String>) {
            self.stale_script.borrow_mut().push_back(outcome);
        }
        fn corrections(&self) -> Vec<AiMemoryCorrectionInput> {
            self.correction_inputs.borrow().clone()
        }
        fn stales(&self) -> Vec<AiMemoryStaleInput> {
            self.stale_inputs.borrow().clone()
        }
    }

    impl AiMemoryCommands for RecordingCommands {
        fn memory_correction_candidate(
            &self,
            input: &AiMemoryCorrectionInput,
        ) -> Result<Option<AiMemoryEntry>, String> {
            self.correction_inputs.borrow_mut().push(input.clone());
            self.correction_script
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(None))
        }
        fn memory_mark_stale(
            &self,
            input: &AiMemoryStaleInput,
        ) -> Result<Vec<AiMemoryEntry>, String> {
            self.stale_inputs.borrow_mut().push(input.clone());
            self.stale_script
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(Vec::new()))
        }
    }

    /// The React suite's `mockResolvedValue({ id: 'm1', status:
    /// 'candidate' })` — the Rust row's field-complete equivalent (the TS
    /// mock is loose; the wire shape is not).
    fn entry(id: &str) -> AiMemoryEntry {
        AiMemoryEntry {
            id: id.to_owned(),
            scope_type: AiMemoryScopeKind::Connection,
            scope_id: "conn-1".to_owned(),
            connection_id: Some("conn-1".to_owned()),
            database_name: Some("sales".to_owned()),
            schema_name: None,
            table_name: None,
            column_name: None,
            kind: AiMemoryKind::Correction,
            subject: "SQL correction".to_owned(),
            content: "SELECT * FROM orders WHERE status = ?".to_owned(),
            structured_json: None,
            source: AiMemorySource::Correction,
            source_ref: None,
            confidence: 0.8,
            sensitivity: AiMemorySensitivity::Normal,
            status: AiMemoryStatus::Candidate,
            pinned: false,
            conflict: None,
            use_count: 0,
            last_used_at: None,
            expires_at: None,
            created_at: "2026-11-19T00:00:00.000Z".to_owned(),
            updated_at: "2026-11-19T00:00:00.000Z".to_owned(),
        }
    }

    fn correction_submission(original: &str, current: &str) -> MemoryCorrectionSubmission {
        MemoryCorrectionSubmission {
            original_sql: original.to_owned(),
            current_sql: current.to_owned(),
            connection_id: None,
            database: None,
        }
    }

    // The React suite (`lib/__tests__/aiMemoryCorrection.test.ts`) mirrored
    // it() by it(): the four under `describe('AI memory correction
    // submission')`.

    #[test]
    fn submits_the_original_and_modified_sql_after_a_successful_modified_ai_query() {
        let commands = RecordingCommands::default();
        commands.script_correction(Ok(Some(entry("m1"))));
        let outcome = submit_memory_correction_if_needed(
            &commands,
            &MemoryCorrectionSubmission {
                original_sql: "SELECT * FROM orders".to_owned(),
                current_sql: "SELECT * FROM orders WHERE status = ?".to_owned(),
                connection_id: Some("conn-1".to_owned()),
                database: Some("sales".to_owned()),
            },
        );
        // `return await` — the host's entry forwards untouched.
        assert_eq!(outcome, Some(entry("m1")));
        assert_eq!(
            commands.corrections(),
            vec![AiMemoryCorrectionInput {
                original_sql: "SELECT * FROM orders".to_owned(),
                new_sql: "SELECT * FROM orders WHERE status = ?".to_owned(),
                connection_id: Some("conn-1".to_owned()),
                database: Some("sales".to_owned()),
            }]
        );
    }

    #[test]
    fn does_not_submit_an_unchanged_ai_query() {
        let commands = RecordingCommands::default();
        // A resolving script proves the guard — not the script — keeps the
        // call away.
        commands.script_correction(Ok(Some(entry("m1"))));
        let outcome = submit_memory_correction_if_needed(
            &commands,
            &correction_submission("SELECT 1", "  SELECT 1  "),
        );
        assert_eq!(outcome, None);
        // `expect(...).not.toHaveBeenCalled()`.
        assert!(commands.corrections().is_empty());
    }

    #[test]
    fn does_not_break_the_query_when_candidate_submission_fails() {
        let commands = RecordingCommands::default();
        commands.script_correction(Err("host offline".to_owned()));
        let outcome = submit_memory_correction_if_needed(
            &commands,
            &correction_submission("SELECT 1", "SELECT 2"),
        );
        assert_eq!(outcome, None);
        // The call was attempted — the swallow happens after, not instead.
        assert_eq!(commands.corrections().len(), 1);
    }

    #[test]
    fn marks_each_dropped_schema_object_stale() {
        let commands = RecordingCommands::default();
        // `mockResolvedValue([])`.
        commands.script_stale(Ok(Vec::new()));
        mark_stale_for_schema_drops(
            &commands,
            &SchemaStaleSubmission {
                sql: "DROP TABLE sales.orders;".to_owned(),
                connection_id: Some("conn-1".to_owned()),
                database: Some("sales".to_owned()),
            },
        );
        assert_eq!(
            commands.stales(),
            vec![AiMemoryStaleInput {
                connection_id: Some("conn-1".to_owned()),
                database: Some("sales".to_owned()),
                schema: Some("sales".to_owned()),
                table: Some("orders".to_owned()),
                // React's `column: undefined`.
                column: None,
            }]
        );
    }

    // [tester] The guard's blank arms — JS's empty-string falsiness on the
    // trimmed locals — never reach the host.

    #[test]
    fn a_blank_half_never_reaches_the_host() {
        let commands = RecordingCommands::default();
        commands.script_correction(Ok(Some(entry("m1"))));
        for (original, current) in [("   ", "SELECT 2"), ("SELECT 1", "\t\n"), ("  ", " ")] {
            let outcome = submit_memory_correction_if_needed(
                &commands,
                &correction_submission(original, current),
            );
            assert_eq!(outcome, None, "({original:?}, {current:?})");
            assert!(commands.corrections().is_empty());
        }
    }

    // [tester] The trim feeds the wire input, not just the guard — and the
    // optional scope fields pass through verbatim: never trimmed, never
    // defaulted (React forwards `submission.connectionId`/`database` as-is).

    #[test]
    fn the_input_carries_the_trimmed_sql_and_the_verbatim_scope() {
        let commands = RecordingCommands::default();
        commands.script_correction(Ok(Some(entry("m1"))));
        let outcome = submit_memory_correction_if_needed(
            &commands,
            &MemoryCorrectionSubmission {
                original_sql: "  SELECT 1  ".to_owned(),
                current_sql: "\n\tSELECT 2 ".to_owned(),
                connection_id: Some(" conn-1 ".to_owned()),
                database: None,
            },
        );
        assert_eq!(outcome, Some(entry("m1")));
        assert_eq!(
            commands.corrections(),
            vec![AiMemoryCorrectionInput {
                original_sql: "SELECT 1".to_owned(),
                new_sql: "SELECT 2".to_owned(),
                // The scope stays untrimmed — only the two SQL locals are.
                connection_id: Some(" conn-1 ".to_owned()),
                database: None,
            }]
        );
    }

    // [tester] The command's own `null` (`AiMemoryEntry | null`) is a third
    // fold, distinct from both the guard (no call) and the rejection (also
    // `None`): the call happened and the host answered empty-handed.

    #[test]
    fn the_hosts_own_null_return_folds_to_none_after_a_real_call() {
        let commands = RecordingCommands::default();
        commands.script_correction(Ok(None));
        let outcome = submit_memory_correction_if_needed(
            &commands,
            &correction_submission("SELECT 1", "SELECT 2"),
        );
        assert_eq!(outcome, None);
        assert_eq!(commands.corrections().len(), 1);
    }

    // [tester] The stale path's early return — `targets.length === 0`
    // never calls the seam. The three fixtures are sqlSchemaDropTargets' own
    // pinned no-target cases (a plain query, whitespace, an ADD COLUMN).

    #[test]
    fn a_statement_without_drops_never_marks_anything_stale() {
        let commands = RecordingCommands::default();
        commands.script_stale(Ok(Vec::new()));
        for sql in [
            "SELECT * FROM orders",
            "   ",
            "ALTER TABLE orders ADD COLUMN note TEXT",
        ] {
            mark_stale_for_schema_drops(
                &commands,
                &SchemaStaleSubmission {
                    sql: sql.to_owned(),
                    ..Default::default()
                },
            );
        }
        assert!(commands.stales().is_empty());
    }

    // [tester] The per-target catch: React's `Promise.all` issues every
    // call and each inner `try`/`catch` swallows its own rejection — one
    // target's failure never blocks the rest, and the fire-and-forget
    // caller always resolves. The two-target fixture is
    // sqlSchemaDropTargets' own pinned multi-target case.

    #[test]
    fn one_targets_failure_never_blocks_the_rest() {
        let commands = RecordingCommands::default();
        commands.script_stale(Err("host offline".to_owned()));
        commands.script_stale(Ok(vec![entry("m1")]));
        mark_stale_for_schema_drops(
            &commands,
            &SchemaStaleSubmission {
                sql: "DROP TABLE IF EXISTS \"sales\".\"orders\", customers CASCADE;".to_owned(),
                connection_id: Some("conn-1".to_owned()),
                database: Some("sales".to_owned()),
            },
        );
        assert_eq!(
            commands.stales(),
            vec![
                AiMemoryStaleInput {
                    connection_id: Some("conn-1".to_owned()),
                    database: Some("sales".to_owned()),
                    schema: Some("sales".to_owned()),
                    table: Some("orders".to_owned()),
                    column: None,
                },
                AiMemoryStaleInput {
                    connection_id: Some("conn-1".to_owned()),
                    database: Some("sales".to_owned()),
                    schema: None,
                    table: Some("customers".to_owned()),
                    column: None,
                },
            ]
        );
    }

    // [tester] The stale input's column arm: a DROP COLUMN target fills
    // `column` (the React suite's table-drop case leaves it `undefined`).
    // Fixture derived from sqlSchemaDropTargets' own pinned ALTER case.

    #[test]
    fn a_dropped_column_fills_the_stale_inputs_column() {
        let commands = RecordingCommands::default();
        commands.script_stale(Ok(Vec::new()));
        mark_stale_for_schema_drops(
            &commands,
            &SchemaStaleSubmission {
                sql: "ALTER TABLE sales.orders DROP COLUMN IF EXISTS legacy_id;".to_owned(),
                connection_id: Some("conn-1".to_owned()),
                database: Some("sales".to_owned()),
            },
        );
        assert_eq!(
            commands.stales(),
            vec![AiMemoryStaleInput {
                connection_id: Some("conn-1".to_owned()),
                database: Some("sales".to_owned()),
                schema: Some("sales".to_owned()),
                table: Some("orders".to_owned()),
                column: Some("legacy_id".to_owned()),
            }]
        );
    }

    // [tester T179-audit] React `aiMemoryCorrection.ts:25` — the sameness
    // arm is strict `===`: case-sensitive on the trimmed locals. Every
    // other pin uses SQL that differs beyond case, so a case-insensitive
    // port (`eq_ignore_ascii_case`, lowercase-compare) survived the round;
    // both directions of a case-only difference must submit — and the
    // wire input must forward each side's original casing. The content
    // assertion's `connection_id`/`database` cells are also the only
    // content-checked `None` scope in this suite (the M4 `unwrap_or_default`
    // mutant survived without it): React's `connectionId: undefined` lands
    // as `None`, never as `Some("")`.

    #[test]
    fn the_sameness_guard_is_case_sensitive() {
        let commands = RecordingCommands::default();
        commands.script_correction(Ok(Some(entry("m1"))));
        commands.script_correction(Ok(Some(entry("m1"))));
        for (original, current) in [("select 1", "SELECT 1"), ("SELECT 1", "select 1")] {
            let outcome = submit_memory_correction_if_needed(
                &commands,
                &correction_submission(original, current),
            );
            assert_eq!(outcome, Some(entry("m1")), "({original:?}, {current:?})");
        }
        // Both directions submitted — each side's own casing on the wire.
        assert_eq!(
            commands.corrections(),
            vec![
                AiMemoryCorrectionInput {
                    original_sql: "select 1".to_owned(),
                    new_sql: "SELECT 1".to_owned(),
                    connection_id: None,
                    database: None,
                },
                AiMemoryCorrectionInput {
                    original_sql: "SELECT 1".to_owned(),
                    new_sql: "select 1".to_owned(),
                    connection_id: None,
                    database: None,
                },
            ]
        );
    }

    // [tester T179-audit] React `aiMemoryCorrection.ts:23-24` — JS
    // `.trim()` strips the whole Unicode White_Space set (NBSP U+00A0,
    // ideographic space U+3000), and `str::trim` agrees. All prior fixtures
    // pad with ASCII only, so an ASCII-only trim (`trim_ascii`) survived
    // the round; these three arms kill it on the guard's blank arm, its
    // sameness arm, and the wire input alike.

    #[test]
    fn the_trim_vocabulary_covers_unicode_whitespace() {
        let commands = RecordingCommands::default();
        commands.script_correction(Ok(Some(entry("m1"))));
        // U+3000 padding only — equal after trim, so no submission.
        let outcome = submit_memory_correction_if_needed(
            &commands,
            &correction_submission("\u{3000}SELECT 1\u{3000}", "SELECT 1"),
        );
        assert_eq!(outcome, None);
        assert!(commands.corrections().is_empty());
        // NBSP padding reaches the host trimmed.
        let outcome = submit_memory_correction_if_needed(
            &commands,
            &correction_submission("\u{00A0}SELECT 1\u{00A0}", "SELECT 2"),
        );
        assert_eq!(outcome, Some(entry("m1")));
        // A NBSP-only original is a blank half, not a submission.
        let outcome = submit_memory_correction_if_needed(
            &commands,
            &correction_submission("\u{00A0}", "SELECT 2"),
        );
        assert_eq!(outcome, None);
        assert_eq!(
            commands.corrections(),
            vec![AiMemoryCorrectionInput {
                original_sql: "SELECT 1".to_owned(),
                new_sql: "SELECT 2".to_owned(),
                connection_id: None,
                database: None,
            }]
        );
    }

    // [tester T179-audit] React `aiMemoryCorrection.ts:51-57` — the stale
    // path's `connectionId: submission.connectionId, database:
    // submission.database` are verbatim per target, and every prior stale
    // pin used a trimmed `Some` scope: a scope-trimming port survived (M2),
    // as did a `None → Some("")` default (M3). Both invocations below
    // forward what the editor handed in — padded or absent.

    #[test]
    fn the_stale_scope_is_forwarded_verbatim_padded_or_absent() {
        let commands = RecordingCommands::default();
        commands.script_stale(Ok(Vec::new()));
        commands.script_stale(Ok(Vec::new()));
        // Padded scope reaches the host untrimmed.
        mark_stale_for_schema_drops(
            &commands,
            &SchemaStaleSubmission {
                sql: "DROP TABLE sales.orders;".to_owned(),
                connection_id: Some(" conn-1 ".to_owned()),
                database: Some(" sales ".to_owned()),
            },
        );
        // Absent scope reaches the host as `None` — with the ALTER column
        // arm still filled.
        mark_stale_for_schema_drops(
            &commands,
            &SchemaStaleSubmission {
                sql: "ALTER TABLE sales.orders DROP COLUMN IF EXISTS legacy_id;".to_owned(),
                connection_id: None,
                database: None,
            },
        );
        assert_eq!(
            commands.stales(),
            vec![
                AiMemoryStaleInput {
                    connection_id: Some(" conn-1 ".to_owned()),
                    database: Some(" sales ".to_owned()),
                    schema: Some("sales".to_owned()),
                    table: Some("orders".to_owned()),
                    column: None,
                },
                AiMemoryStaleInput {
                    connection_id: None,
                    database: None,
                    schema: Some("sales".to_owned()),
                    table: Some("orders".to_owned()),
                    column: Some("legacy_id".to_owned()),
                },
            ]
        );
    }
}
