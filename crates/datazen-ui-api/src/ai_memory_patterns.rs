//! Automatic memory-pattern extraction — ported from React
//! `packages/frontend/src/lib/aiMemoryPatterns.ts` (tasklist T205,
//! 91 lines).
//!
//! The execution-complete seam that feeds the T179 correction hooks feeds
//! this one too: after a successful run the editor asks the host to mine
//! the recent query history for repeating SQL shapes (the T14 extractor's
//! candidate pass). The UI reads one page of successful statements — the
//! last 14 days, at most 100 rows, most recent first — and hands the
//! samples plus the SQL that just ran to
//! `aiCommands.memoryExtractCandidates`; the host owns the actual pattern
//! mining. Learning must never make the successful query look failed, so
//! a history read or IPC rejection is swallowed and never surfaces as a
//! query failure (the React doc comments' own contract).
//!
//! ## The seams (each names its owning migration task)
//!
//! | React import | Port seam | Owner |
//! | --- | --- | --- |
//! | `commands/query` (`queryCommands.getQueryHistoryPage`) | [`MemoryPatternCommands::get_query_history_page`] over the lean [`QueryHistoryPageParams`] / [`QueryHistoryPage`] / [`HistorySort`] mirrors | commands/query port task |
//! | `commands/ai` (`aiCommands.memoryExtractCandidates`) | [`MemoryPatternCommands::memory_extract_candidates`] — `Result` per the T040 await→`Result` precedent | commands/ai port task |
//! | `../types` (`AiMemorySqlSample`) | already ported (T042): [`AiMemorySqlSample`](crate::ai_types::AiMemorySqlSample) | done |
//! | `../types` (`QueryHistoryEntry`) | already ported: [`QueryHistoryEntry`](crate::connection_types::QueryHistoryEntry) | done |
//!
//! ## The promise loop → state machine (this port's one substitution)
//!
//! React keeps two module-level maps — `inFlight` (the running extraction
//! promise per connection/database) and `pending` (a single follow-up slot
//! per scope) — and a promise loop that reruns with the newest queued
//! input whenever a pass lands. The crate is sync, so the loop becomes an
//! explicit machine the host drives:
//! [`MemoryPatternScheduler::submit`] decides what runs now (a fresh pass,
//! a coalesced join behind the in-flight pass, or a guarded no-op), and
//! [`MemoryPatternScheduler::finish`] closes a completed pass — any
//! outcome — and hands back the one queued rerun. The desktop host drives
//! it step-wise (dispatch the pass to a worker, report `finish` on
//! completion), while [`extract_memory_patterns_if_needed`] is the
//! synchronous whole-loop arm for hosts that run passes inline. The burst
//! semantics are unchanged: N submits while one pass is in flight
//! coalesce into exactly one rerun carrying the newest input (the single
//! pending slot is overwritten, so a third submit replaces the second and
//! is never lost), and the joined caller observes nothing — React's shared
//! promise is `void` with swallowed errors, so the join has no observable
//! completion beyond the rerun itself.
//!
//! ## Mirror rules applied
//!
//! - **Async → sync fold (the T040 precedent)**: each `await`ed invoke is
//!   one seam call returning `Result<T, String>` (`Err` is the rejection's
//!   display message — the `AiSessionBackend` convention). The loop's
//!   `try`/`catch` becomes `finish`'s outcome parameter: received and
//!   dropped on purpose, so a failed pass advances the machine exactly
//!   like a successful one.
//! - **The `typeof` guards**: `typeof queryCommands?.getQueryHistoryPage
//!   !== 'function' || typeof aiCommands?.memoryExtractCandidates !==
//!   'function'` folds into the seam's two availability probes; the guard
//!   fires before any call and is the plain `return` (`Ok(())`), not a
//!   rejection.
//! - **`now` handling**: `now ?? Date.now()` is `Option<f64>` with the
//!   wall-clock fallback spelled like `relative_time`'s precedent (that
//!   sibling keeps its read private, so this one spells it out again);
//!   `??` is not a finiteness check, so a non-finite `Some` survives to
//!   the ISO step. `new Date(now - 14d).toISOString()` reuses the public
//!   `ai_sessions::iso_from_epoch_ms`; the RangeError an invalid date
//!   throws there is the pass's `Err`.
//! - **Numeric choices**: the two exported constants are `u64` because
//!   the wire input's own `window_days`/`min_occurrences` are
//!   `Option<u64>` — the constants feed the payload without casts.
//!   `HISTORY_LIMIT` stays private (React never exports it) and rides
//!   the same `u64`, as do the page's `limit`/`total` (the query-history
//!   family's `execution_time_ms` precedent).
//! - **Scope shaping asymmetry**: the history read forwards the pair
//!   verbatim (`connectionId: input.connectionId ?? null` — an empty
//!   string stays an empty string, `None` lands as null), while the
//!   extract payload includes the pair only when truthy (the spread
//!   conditionals: `Some("")` is omitted, `Some(" ")` is kept —
//!   truthiness is non-emptiness, not trim).
//! - **Verbatim strings**: the guards trim ([`str::trim`], the same
//!   vocabulary T179 pinned) but the payloads carry the raw `input.sql`
//!   and `entry.sql`; the history filter trims only to test emptiness,
//!   and the sample's `observedAt` is the entry's own `executedAt`.
//! - **Ownership**: `submit` takes the input by value (the pending slot
//!   stores it). `extractOnce` is React-private, but the step-wise host
//!   runs the passes, so the port promotes it — with the loop's catch
//!   moved to `finish`'s dropped outcome.
//! - **No serde on this module's own shapes**: the seam is in-process;
//!   the wire shapes (`AiMemoryExtractionInput`, `AiMemorySqlSample`)
//!   already live in `ai_types` and `QueryHistoryEntry` in
//!   `connection_types` — reused, not forked. `QueryHistoryPage`,
//!   `QueryHistoryPageParams`, and `HistorySort` are lean serde-less
//!   mirrors of the `types/index.ts` shapes the query-history surface
//!   owns (the T213 `types/index.ts` split carries the page); when that
//!   port lands it should adopt or replace them, not fork them.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ai_sessions::iso_from_epoch_ms;
use crate::ai_types::{AiMemoryEntry, AiMemoryExtractionInput, AiMemorySqlSample};
use crate::connection_types::QueryHistoryEntry;

/// React `MEMORY_PATTERN_WINDOW_DAYS = 14` — the look-back window both the
/// history read and the extractor request are cut to. The host applies
/// the same window; keeping it here makes the request explicit.
pub const MEMORY_PATTERN_WINDOW_DAYS: u64 = 14;

/// React `MEMORY_PATTERN_MIN_OCCURRENCES = 3` — the occurrence count the
/// extractor needs before it records a pattern.
pub const MEMORY_PATTERN_MIN_OCCURRENCES: u64 = 3;

/// React's private `HISTORY_LIMIT = 100` — the single history page the
/// pass reads. Private here too: React never exports it.
const HISTORY_LIMIT: u64 = 100;

/// React's inline `new Date(now - MEMORY_PATTERN_WINDOW_DAYS * 24 * 60 *
/// 60 * 1000).toISOString()` — the pure window math, named because the
/// pass and the tests share it. `None` is the RangeError `toISOString`
/// throws on an invalid date (a non-finite or out-of-range `now`), which
/// the loop's catch — here the pass's `Err` — swallows. Reuses the public
/// `ai_sessions::iso_from_epoch_ms` rather than forking its arithmetic.
fn since_iso(now: f64) -> Option<String> {
    iso_from_epoch_ms(now - MEMORY_PATTERN_WINDOW_DAYS as f64 * 24.0 * 60.0 * 60.0 * 1000.0)
}

/// React's `Date.now()` fallback — the wall clock in epoch milliseconds.
/// The same read `relative_time` spells (that sibling keeps its copy
/// private, so this module spells it out again rather than widening a
/// sibling's surface for it).
fn real_clock_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as f64)
        .unwrap_or(0.0)
}

/// The two command surfaces `lib/aiMemoryPatterns.ts` consumes — the
/// paged history read off `commands/query` and the extraction request off
/// `commands/ai` (both commands segments, not ported). One trait fuses
/// them because this module is their only shared caller.
///
/// React's promises map to `Result` exactly the way the T040 sessions
/// port mapped `aiCommands.sessions*` and the T179 correction port mapped
/// `aiCommands.memory*`: `Ok` is the invoke's resolved value, `Err(String)`
/// is the rejection's display message. The two availability probes are
/// React's `typeof … === 'function'` guards: a host whose surface lacks
/// the command answers `false`, and the pass never issues the call (the
/// guard's plain `return`, not a rejection). `memory_extract_candidates`
/// resolves with the entries it recorded; this module discards that list
/// (React awaits it and drops it too). **Ownership**: the commands/query
/// and commands/ai port tasks own the real adapters; when they land they
/// should implement this seam rather than fork it.
pub trait MemoryPatternCommands {
    /// `typeof queryCommands?.getQueryHistoryPage === 'function'`.
    fn history_available(&self) -> bool;
    /// `typeof aiCommands?.memoryExtractCandidates === 'function'`.
    fn extraction_available(&self) -> bool;
    /// `queryCommands.getQueryHistoryPage(opts)` — await → `Result`.
    fn get_query_history_page(
        &self,
        params: &QueryHistoryPageParams,
    ) -> Result<QueryHistoryPage, String>;
    /// `aiCommands.memoryExtractCandidates(input)` — await → `Result`.
    fn memory_extract_candidates(
        &self,
        input: &AiMemoryExtractionInput,
    ) -> Result<Vec<AiMemoryEntry>, String>;
}

/// React `HistorySort = 'recent' | 'oldest' | 'slowest'`
/// (`types/index.ts`) — the paged history read's backend sort order. A
/// lean, serde-less mirror: this module only ever sends `Recent`, and the
/// union's real home is the query-history surface (the T213
/// `types/index.ts` split) — promote it there when that task lands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistorySort {
    /// The commands wrapper's own fallback (`order ?? 'recent'`), hence
    /// this port's [`Default`] arm.
    #[default]
    Recent,
    Oldest,
    Slowest,
}

impl HistorySort {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recent => "recent",
            Self::Oldest => "oldest",
            Self::Slowest => "slowest",
        }
    }
}

/// React `getQueryHistoryPage`'s opts object — the five filters this
/// module exercises, mirrored as an in-process call parameter (no serde:
/// nothing here crosses a boundary; the commands/query adapter converts
/// at its edge). The scope pair is the verbatim `?? null` forwarding — an
/// empty string stays an empty string, `None` lands as null. The
/// command's `schema`/`search` filters stay with the commands/query port:
/// this seam mirrors only the call `aiMemoryPatterns.ts` makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryHistoryPageParams {
    pub limit: u64,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub since: Option<String>,
    pub order: HistorySort,
}

/// React `QueryHistoryPage` — one page of history plus the count the
/// filter matched before the page was cut. This module reads `entries`
/// only (React ignores `total` here exactly the same way); the field
/// rides anyway because it is the command's own contract. Lean
/// serde-less mirror with the same owner as [`HistorySort`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryHistoryPage {
    pub entries: Vec<QueryHistoryEntry>,
    pub total: u64,
}

/// React's `MemoryPatternExtractionInput` — what the execution-complete
/// seam hands the extraction hook: the SQL that just ran, the scope it
/// ran under, the auto-learn switch, and an optional clock read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryPatternExtractionInput {
    pub sql: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    /// Mirrors the two T13 memory switches; off means no extraction at
    /// all.
    pub auto_learn: bool,
    /// `now?: number` — epoch milliseconds; `None` is React's `undefined`
    /// and falls back to the wall clock inside the pass.
    pub now: Option<f64>,
}

/// React's private `scopeKey` — `connectionId ?? ''`, U+0000, `database
/// ?? ''`. The NUL separator is React's own; ids containing U+0000 would
/// collide exactly like the React module's, so the hazard is inherited,
/// not fixed. Callers treat the key as an opaque token handed from
/// `submit` to `finish`.
fn scope_key(input: &MemoryPatternExtractionInput) -> String {
    format!(
        "{}\u{0}{}",
        input.connection_id.as_deref().unwrap_or(""),
        input.database.as_deref().unwrap_or("")
    )
}

/// React's private `extractOnce` — one full pass: read the recent
/// successful history for the scope, then hand it plus the current SQL to
/// the extractor. Errors propagate (the promise loop's `catch` — the
/// scheduler's `finish` — is the swallow point), so the caller sees the
/// rejection's message and the machine still advances.
pub fn extract_once<B: MemoryPatternCommands>(
    commands: &B,
    input: &MemoryPatternExtractionInput,
) -> Result<(), String> {
    // React: `typeof queryCommands?.getQueryHistoryPage !== 'function' ||
    // typeof aiCommands?.memoryExtractCandidates !== 'function'` — the
    // plain `return`, not a rejection, and before any clock read.
    if !commands.history_available() || !commands.extraction_available() {
        return Ok(());
    }
    // React: `const now = input.now ?? Date.now();` — `??` only, so a
    // non-finite `Some` survives to the ISO step below.
    let now = input.now.unwrap_or_else(real_clock_ms);
    // React: `new Date(now - MEMORY_PATTERN_WINDOW_DAYS * 24*60*60*1000)
    // .toISOString()` — the RangeError an invalid date throws is the
    // pass's `Err`, before any host call.
    let since = since_iso(now).ok_or_else(|| "invalid `now` for the pattern window".to_owned())?;
    // React: `queryCommands.getQueryHistoryPage({ limit: HISTORY_LIMIT,
    // connectionId: input.connectionId ?? null, database:
    // input.database ?? null, since, order: 'recent' })` — the scope pair
    // verbatim, never truthiness.
    let page = commands.get_query_history_page(&QueryHistoryPageParams {
        limit: HISTORY_LIMIT,
        connection_id: input.connection_id.clone(),
        database: input.database.clone(),
        since: Some(since),
        order: HistorySort::Recent,
    })?;
    // React: `page.entries.filter((entry) => entry.success &&
    // entry.sql.trim().length > 0).map((entry) => ({ sql: entry.sql,
    // observedAt: entry.executedAt }))` — trim only tests emptiness; the
    // sample's own fields ride raw.
    let history = page
        .entries
        .iter()
        .filter(|entry| entry.success && !entry.sql.trim().is_empty())
        .map(|entry| AiMemorySqlSample {
            sql: entry.sql.clone(),
            observed_at: entry.executed_at.clone(),
        })
        .collect::<Vec<_>>();
    // React: `aiCommands.memoryExtractCandidates({ sql: input.sql,
    // ...(input.connectionId ? { connectionId: input.connectionId } : {}),
    // ...(input.database ? { database: input.database } : {}), history,
    // windowDays: MEMORY_PATTERN_WINDOW_DAYS, minOccurrences:
    // MEMORY_PATTERN_MIN_OCCURRENCES })` — the scope pair rides only when
    // truthy; the resolved candidate list is awaited and dropped, like
    // the rejection.
    commands.memory_extract_candidates(&AiMemoryExtractionInput {
        sql: input.sql.clone(),
        connection_id: input.connection_id.clone().filter(|id| !id.is_empty()),
        database: input
            .database
            .clone()
            .filter(|database| !database.is_empty()),
        history: Some(history),
        window_days: Some(MEMORY_PATTERN_WINDOW_DAYS),
        min_occurrences: Some(MEMORY_PATTERN_MIN_OCCURRENCES),
        model_output_json: None,
    })?;
    Ok(())
}

/// What [`MemoryPatternScheduler::submit`] decided — the three arms of
/// React's `extractMemoryPatternsIfNeeded` entry.
#[derive(Clone, Debug, PartialEq)]
pub enum SubmitDecision {
    /// No pass in flight for the scope: run one `extract_once` pass with
    /// this input now, then report it finished.
    Run {
        /// The opaque scope token `finish` wants back.
        key: String,
        /// The loop's first `next`.
        input: MemoryPatternExtractionInput,
    },
    /// A pass is in flight for the scope: the input took the single
    /// pending slot (the newest wins) and the caller joined the in-flight
    /// pass — which observes nothing, because React's shared promise is
    /// `void` with swallowed errors; the eventual rerun carrying this
    /// input is the only observable.
    Coalesced,
    /// The off-switch or blank-SQL guard: nothing ran and no state
    /// changed (React's resolved `Promise.resolve()` return).
    Skipped,
}

/// React's two module-level maps made explicit — the `inFlight` promise
/// per scope and the single `pending` follow-up slot. React's module
/// singleton becomes a construction-time choice: the host holds one
/// instance beside its [`MemoryPatternCommands`] seam and drives it
/// step-wise (see the module docs for the promise-loop trade).
#[derive(Debug, Default)]
pub struct MemoryPatternScheduler {
    /// React's `inFlight` — one running pass per scope key; the promise
    /// itself is unobservable (`void`, errors swallowed), so only
    /// membership remains.
    in_flight: HashSet<String>,
    /// React's `pending` — the single follow-up slot per scope; a newer
    /// submit overwrites an older one (the newest wins, the dropped
    /// middle is by design).
    pending: HashMap<String, MemoryPatternExtractionInput>,
}

impl MemoryPatternScheduler {
    /// React's `extractMemoryPatternsIfNeeded` submit half — the guard,
    /// the in-flight check, the pending overwrite, folded into one
    /// decision.
    pub fn submit(&mut self, input: MemoryPatternExtractionInput) -> SubmitDecision {
        // React: `if (!input.autoLearn || input.sql.trim().length === 0)
        // return Promise.resolve();` — the empty-string falsy arm on the
        // trimmed SQL (`str::trim`, the same vocabulary T179 pinned).
        if !input.auto_learn || input.sql.trim().is_empty() {
            return SubmitDecision::Skipped;
        }
        let key = scope_key(&input);
        // React: `const current = inFlight.get(key); if (current) {
        // pending.set(key, input); return current; }` — the newest input
        // takes the one follow-up slot; the join itself is unobservable.
        if self.in_flight.contains(&key) {
            self.pending.insert(key, input);
            return SubmitDecision::Coalesced;
        }
        // React: `inFlight.set(key, task)` — the pass starts with this
        // input as the loop's first `next`.
        self.in_flight.insert(key.clone());
        SubmitDecision::Run { key, input }
    }

    /// React's loop tail — the pass for `key` completed. The `_outcome`
    /// is the loop's `catch`: received and dropped on purpose, so a
    /// failed pass advances exactly like a successful one (a rejection
    /// never blocks the next attempt and never reaches the query). The
    /// return is the queued rerun when a newer input coalesced behind the
    /// pass; `None` means the scope went idle.
    pub fn finish(
        &mut self,
        key: &str,
        _outcome: Result<(), String>,
    ) -> Option<MemoryPatternExtractionInput> {
        // React: `const queued = pending.get(key); if (!queued) {
        // inFlight.delete(key); return; } pending.delete(key); next =
        // queued;` — a stray finish on an idle key stays a no-op, like
        // `Map.delete` tolerating a missing entry.
        match self.pending.remove(key) {
            Some(queued) => Some(queued),
            None => {
                self.in_flight.remove(key);
                None
            }
        }
    }
}

/// React's `extractMemoryPatternsIfNeeded` — the whole promise loop for a
/// host that runs passes inline: submit, run the pass, close it out, and
/// rerun once with the newest queued input if any coalesced behind it.
/// The loop's `catch` is the swallowed `extract_once` outcome handed to
/// `finish`; the joined callers observe nothing (React's shared `void`
/// promise with swallowed errors).
///
/// Under the exclusive `&mut` borrow no second submit can interleave, so
/// the drain is provably single-pass — the rerun tail exists for the
/// step-wise host that interleaves submits between `submit` and
/// `finish`, and keeping the loop's shape here makes both arms identical.
pub fn extract_memory_patterns_if_needed<B: MemoryPatternCommands>(
    commands: &B,
    scheduler: &mut MemoryPatternScheduler,
    input: &MemoryPatternExtractionInput,
) {
    // React: the two guard arms fold to the resolved promise; only a
    // started pass reaches the loop.
    if let SubmitDecision::Run { key, input } = scheduler.submit(input.clone()) {
        let mut next = input;
        loop {
            // React: `try { await extractOnce(next); } catch { … }` — the
            // outcome is reported to and dropped by `finish` below.
            let outcome = extract_once(commands, &next);
            // React: `const queued = pending.get(key); if (!queued) {
            // inFlight.delete(key); return; } pending.delete(key); next =
            // queued;` — the newest queued input reruns; none ends the
            // loop and frees the scope.
            match scheduler.finish(&key, outcome) {
                Some(queued) => next = queued,
                None => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// The commands double the vitest `vi.mock('../../commands/query')` /
    /// `vi.mock('../../commands/ai')` pair plays: every call is recorded,
    /// each call replays the scripted outcome in order, and an exhausted
    /// script answers the seam's inert default (`Ok` with the empty page
    /// / empty list) so a forgotten script never invents a failure. The
    /// two availability flags model the `typeof` guards'
    /// missing-surface arms.
    struct RecordingCommands {
        history_inputs: RefCell<Vec<QueryHistoryPageParams>>,
        extract_inputs: RefCell<Vec<AiMemoryExtractionInput>>,
        history_script: RefCell<VecDeque<Result<QueryHistoryPage, String>>>,
        extract_script: RefCell<VecDeque<Result<Vec<AiMemoryEntry>, String>>>,
        history_available: bool,
        extraction_available: bool,
    }

    impl RecordingCommands {
        /// Both command surfaces present — the vitest mocks' implicit
        /// stance, and the desktop shell's answer.
        fn new() -> Self {
            Self {
                history_inputs: RefCell::default(),
                extract_inputs: RefCell::default(),
                history_script: RefCell::default(),
                extract_script: RefCell::default(),
                history_available: true,
                extraction_available: true,
            }
        }
        /// The `typeof queryCommands?.getQueryHistoryPage !== 'function'`
        /// arm.
        fn without_history_surface() -> Self {
            Self {
                history_available: false,
                ..Self::new()
            }
        }
        /// The `typeof aiCommands?.memoryExtractCandidates !== 'function'`
        /// arm.
        fn without_extraction_surface() -> Self {
            Self {
                extraction_available: false,
                ..Self::new()
            }
        }
        fn script_history(&self, outcome: Result<QueryHistoryPage, String>) {
            self.history_script.borrow_mut().push_back(outcome);
        }
        fn script_extract(&self, outcome: Result<Vec<AiMemoryEntry>, String>) {
            self.extract_script.borrow_mut().push_back(outcome);
        }
        fn history_calls(&self) -> Vec<QueryHistoryPageParams> {
            self.history_inputs.borrow().clone()
        }
        fn extract_calls(&self) -> Vec<AiMemoryExtractionInput> {
            self.extract_inputs.borrow().clone()
        }
    }

    impl MemoryPatternCommands for RecordingCommands {
        fn history_available(&self) -> bool {
            self.history_available
        }
        fn extraction_available(&self) -> bool {
            self.extraction_available
        }
        fn get_query_history_page(
            &self,
            params: &QueryHistoryPageParams,
        ) -> Result<QueryHistoryPage, String> {
            self.history_inputs.borrow_mut().push(params.clone());
            self.history_script
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Ok(QueryHistoryPage::default()))
        }
        fn memory_extract_candidates(
            &self,
            input: &AiMemoryExtractionInput,
        ) -> Result<Vec<AiMemoryEntry>, String> {
            self.extract_inputs.borrow_mut().push(input.clone());
            self.extract_script
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Ok(Vec::new()))
        }
    }

    /// 2026-04-20T12:00:00.000Z — the React suite's fixed `NOW`
    /// (`Date.parse('2026-04-20T12:00:00.000Z')`) in epoch ms.
    const NOW: f64 = 1_776_686_400_000.0;

    /// `NOW - 14 days` as the ISO string the history read carries — the
    /// same value the React test computes inline.
    const SINCE: &str = "2026-04-06T12:00:00.000Z";

    /// The join SQL the React suite's history page repeats three times.
    const JOIN_SQL: &str = "SELECT * FROM orders o JOIN users u ON o.user_id = u.id";

    /// The React suite's `entry()` — one history row.
    fn entry(id: &str, sql: &str, success: bool) -> QueryHistoryEntry {
        QueryHistoryEntry {
            id: id.to_owned(),
            connection_id: "conn-1".to_owned(),
            database: "sales".to_owned(),
            schema: None,
            sql: sql.to_owned(),
            executed_at: "2026-04-19T12:00:00.000Z".to_owned(),
            execution_time_ms: 5,
            rows_affected: None,
            success,
            error_message: None,
        }
    }

    /// The React suite's `page()` — the entries plus the matched count.
    fn page(entries: Vec<QueryHistoryEntry>) -> QueryHistoryPage {
        QueryHistoryPage {
            total: entries.len() as u64,
            entries,
        }
    }

    /// The React suite's per-test input literal (conn-1/sales, fixed
    /// `now`); the caller varies the SQL and the switch.
    fn input(sql: &str, auto_learn: bool) -> MemoryPatternExtractionInput {
        MemoryPatternExtractionInput {
            sql: sql.to_owned(),
            connection_id: Some("conn-1".to_owned()),
            database: Some("sales".to_owned()),
            auto_learn,
            now: Some(NOW),
        }
    }

    // The React suite (`lib/__tests__/aiMemoryPatterns.test.ts`) mirrored
    // it() by it(): the four under `describe('automatic memory pattern
    // extraction')`, then the tester pins for the arms the suite leaves
    // open.

    #[test]
    fn does_nothing_when_the_extraction_switch_is_off() {
        let commands = RecordingCommands::new();
        let mut scheduler = MemoryPatternScheduler::default();
        // React: `autoLearn: false` → the resolved promise; history and
        // extract both `not.toHaveBeenCalled()`.
        extract_memory_patterns_if_needed(&commands, &mut scheduler, &input("SELECT 1", false));
        assert!(commands.history_calls().is_empty());
        assert!(commands.extract_calls().is_empty());
        // [tester] the guard's blank-SQL arm — the empty-string falsy test
        // on the trimmed SQL, with U+3000 padding covering the whole
        // Unicode White_Space vocabulary (the T179 pin).
        extract_memory_patterns_if_needed(&commands, &mut scheduler, &input("\u{3000} \t", true));
        assert!(commands.history_calls().is_empty());
        assert!(commands.extract_calls().is_empty());
        // [tester] the guard leaves the machine untouched — a valid
        // submit afterwards still starts a pass (no phantom in-flight).
        assert!(matches!(
            scheduler.submit(input("SELECT 1", true)),
            SubmitDecision::Run { .. }
        ));
    }

    #[test]
    fn feeds_recent_successful_history_and_the_current_sql_to_the_extractor() {
        let commands = RecordingCommands::new();
        // The beforeEach default page: three successful repeats of the
        // join plus one failed statement.
        commands.script_history(Ok(page(vec![
            entry("h1", JOIN_SQL, true),
            entry("h2", JOIN_SQL, true),
            entry("h3", JOIN_SQL, true),
            entry("failed", "SELECT broken", false),
        ])));
        commands.script_extract(Ok(Vec::new()));
        let mut scheduler = MemoryPatternScheduler::default();
        extract_memory_patterns_if_needed(&commands, &mut scheduler, &input(JOIN_SQL, true));

        // React: `expect(mocks.history).toHaveBeenCalledWith({ limit:
        // 100, connectionId: 'conn-1', database: 'sales', since:
        // <NOW-14d ISO>, order: 'recent' })`.
        assert_eq!(
            commands.history_calls(),
            vec![QueryHistoryPageParams {
                limit: 100,
                connection_id: Some("conn-1".to_owned()),
                database: Some("sales".to_owned()),
                since: Some(SINCE.to_owned()),
                order: HistorySort::Recent,
            }]
        );
        // React: `expect(mocks.extract).toHaveBeenCalledWith({ sql: …,
        // connectionId: 'conn-1', database: 'sales', history: [three
        // samples], windowDays: 14, minOccurrences: 3 })`.
        let sample = AiMemorySqlSample {
            sql: JOIN_SQL.to_owned(),
            observed_at: "2026-04-19T12:00:00.000Z".to_owned(),
        };
        assert_eq!(
            commands.extract_calls(),
            vec![AiMemoryExtractionInput {
                sql: JOIN_SQL.to_owned(),
                connection_id: Some("conn-1".to_owned()),
                database: Some("sales".to_owned()),
                history: Some(vec![sample.clone(), sample.clone(), sample]),
                window_days: Some(MEMORY_PATTERN_WINDOW_DAYS),
                min_occurrences: Some(MEMORY_PATTERN_MIN_OCCURRENCES),
                model_output_json: None,
            }]
        );
        // React: "Current SQL + three historical samples is the ≥3
        // occurrence proof."
        let calls = commands.extract_calls();
        let history = calls[0]
            .history
            .as_ref()
            .expect("history rides the payload");
        assert!(
            history.len() as u64 + 1 >= MEMORY_PATTERN_MIN_OCCURRENCES,
            "the current SQL plus the samples must clear the occurrence bar"
        );
    }

    #[test]
    fn swallows_history_and_extraction_failures() {
        let commands = RecordingCommands::new();
        let mut scheduler = MemoryPatternScheduler::default();
        // React: `mocks.history.mockRejectedValueOnce(...)` — the call
        // resolves (`resolves.toBeUndefined()`), and the failure never
        // reaches the extract call.
        commands.script_history(Err("history unavailable".to_owned()));
        extract_memory_patterns_if_needed(&commands, &mut scheduler, &input("SELECT 1", true));
        assert!(commands.extract_calls().is_empty());
        // React: `mocks.extract.mockRejectedValueOnce(...)` — the call is
        // attempted and the rejection swallowed after it, not instead of
        // it.
        commands.script_extract(Err("extract unavailable".to_owned()));
        extract_memory_patterns_if_needed(&commands, &mut scheduler, &input("SELECT 2", true));
        assert_eq!(commands.extract_calls().len(), 1);
        // [tester] the React doc comment's own rule — a failure "must
        // not … block the next attempt": both passes went idle and a
        // fresh submit starts a pass again.
        assert!(matches!(
            scheduler.submit(input("SELECT 3", true)),
            SubmitDecision::Run { .. }
        ));
    }

    #[test]
    fn coalesces_a_burst_and_reruns_once_with_the_newest_sql() {
        let commands = RecordingCommands::new();
        // React holds the first history call unresolved across the second
        // submit; the sync port's in-flight window is the span between
        // `submit` and `finish`, so the same ordering needs no gate.
        commands.script_history(Ok(page(Vec::new())));
        commands.script_history(Ok(page(Vec::new())));
        commands.script_extract(Ok(Vec::new()));
        commands.script_extract(Ok(Vec::new()));
        let mut scheduler = MemoryPatternScheduler::default();
        let first = scheduler.submit(input("SELECT first", true));
        let second = scheduler.submit(input("SELECT second", true));
        // The second submit joined the in-flight pass — the burst's newest
        // input took the pending slot.
        assert!(matches!(second, SubmitDecision::Coalesced));
        let (key, first_input) = match first {
            SubmitDecision::Run { key, input } => (key, input),
            decision => panic!("expected a started pass, got {decision:?}"),
        };
        // Run pass 1 — React asserts the history mock was called exactly
        // once while the pass was still holding the release.
        let outcome = extract_once(&commands, &first_input);
        assert_eq!(commands.history_calls().len(), 1);
        // React: `release?.(page([]))` + `await Promise.all` — the pass
        // lands, the queued newest input reruns once, and only then does
        // everything settle.
        let rerun = scheduler.finish(&key, outcome);
        assert_eq!(
            rerun.as_ref().map(|input| input.sql.as_str()),
            Some("SELECT second")
        );
        let outcome = extract_once(&commands, &rerun.expect("the rerun input"));
        assert_eq!(commands.history_calls().len(), 2);
        assert!(scheduler.finish(&key, outcome).is_none());
        // React: `expect(mocks.extract.mock.calls[1][0]).toMatchObject({
        // sql: 'SELECT second' })` — the rerun carries the newest SQL
        // (pass 1's own extract call carries `SELECT first`).
        assert_eq!(
            commands
                .extract_calls()
                .iter()
                .map(|input| input.sql.as_str())
                .collect::<Vec<_>>(),
            vec!["SELECT first", "SELECT second"]
        );
        // [tester] the empty page rides as an empty history array — the
        // field is present, never omitted.
        assert_eq!(commands.extract_calls()[0].history, Some(Vec::new()));
    }

    // [tester] React's doc comment: "a newer execution queues exactly one
    // follow-up pass so the third occurrence in a burst is not lost" — the
    // single pending slot is overwritten by the newest submit, so three
    // submits during one pass coalesce into ONE rerun carrying the third
    // (the second is dropped by design), and the machine goes idle after
    // that single rerun.

    #[test]
    fn a_burst_of_three_coalesces_into_one_rerun_with_the_newest_input() {
        let commands = RecordingCommands::new();
        let mut scheduler = MemoryPatternScheduler::default();
        let first = scheduler.submit(input("SELECT first", true));
        assert!(matches!(
            scheduler.submit(input("SELECT second", true)),
            SubmitDecision::Coalesced
        ));
        assert!(matches!(
            scheduler.submit(input("SELECT third", true)),
            SubmitDecision::Coalesced
        ));
        let (key, first_input) = match first {
            SubmitDecision::Run { key, input } => (key, input),
            decision => panic!("expected a started pass, got {decision:?}"),
        };
        assert!(extract_once(&commands, &first_input).is_ok());
        // The newest (third) input is the one rerun — the second was
        // overwritten in the single pending slot.
        let rerun = scheduler.finish(&key, Ok(())).expect("exactly one rerun");
        assert_eq!(rerun.sql, "SELECT third");
        assert!(extract_once(&commands, &rerun).is_ok());
        // Exactly one rerun: the machine is idle again afterwards.
        assert!(scheduler.finish(&key, Ok(())).is_none());
        assert_eq!(commands.history_calls().len(), 2);
    }

    // [tester] The loop's `catch` sits before the pending check — a failed
    // pass must still hand back the queued rerun (the React doc comment's
    // "never … block the next attempt"); the outcome parameter being
    // dropped by `finish` is that catch, made observable.

    #[test]
    fn a_failed_pass_still_reruns_the_pending_input() {
        let commands = RecordingCommands::new();
        commands.script_history(Err("history unavailable".to_owned()));
        let mut scheduler = MemoryPatternScheduler::default();
        let first = scheduler.submit(input("SELECT first", true));
        assert!(matches!(
            scheduler.submit(input("SELECT second", true)),
            SubmitDecision::Coalesced
        ));
        let (key, first_input) = match first {
            SubmitDecision::Run { key, input } => (key, input),
            decision => panic!("expected a started pass, got {decision:?}"),
        };
        // The failing pass propagates its Err to the caller — the seam's
        // rejection message verbatim.
        assert_eq!(
            extract_once(&commands, &first_input),
            Err("history unavailable".to_owned())
        );
        // … and the machine still hands back the queued rerun.
        assert!(scheduler
            .finish(&key, Err("history unavailable".to_owned()))
            .is_some());
    }

    // [tester] React's `typeof` guards — either command surface missing
    // and the pass is the plain `return` (`Ok`, not a rejection) with no
    // host call at all; the guard also fires before the clock read, so a
    // NaN `now` never reaches the ISO step.

    #[test]
    fn missing_command_surfaces_never_reach_the_host() {
        for commands in [
            RecordingCommands::without_history_surface(),
            RecordingCommands::without_extraction_surface(),
        ] {
            let outcome = extract_once(
                &commands,
                &MemoryPatternExtractionInput {
                    // The guard precedes the clock read — this must not
                    // error.
                    now: Some(f64::NAN),
                    ..input("SELECT 1", true)
                },
            );
            assert_eq!(outcome, Ok(()));
            assert!(commands.history_calls().is_empty());
            assert!(commands.extract_calls().is_empty());
        }
    }

    // [tester] `now` is `??`-only in React (`input.now ?? Date.now()`) —
    // a non-finite `Some` survives to the ISO step, where the RangeError
    // becomes the pass's `Err` before any host call. The pure window math
    // itself pins the 14-day cut and the invalid arms' `None`.

    #[test]
    fn an_invalid_now_fails_the_pass_before_any_host_call() {
        let commands = RecordingCommands::new();
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e16, -1e16] {
            let outcome = extract_once(
                &commands,
                &MemoryPatternExtractionInput {
                    now: Some(now),
                    ..input("SELECT 1", true)
                },
            );
            assert_eq!(
                outcome,
                Err("invalid `now` for the pattern window".to_owned()),
                "now = {now}"
            );
            assert!(commands.history_calls().is_empty());
        }
        // The window math: NOW-14d pins the ISO cut; the invalid arms
        // return `None` (the RangeError).
        assert_eq!(since_iso(NOW).as_deref(), Some(SINCE));
        for now in [f64::NAN, f64::INFINITY, 1e16] {
            assert_eq!(since_iso(now), None, "now = {now}");
        }
    }

    // [tester] The scope shaping's asymmetry — the history read forwards
    // the pair verbatim (`input.connectionId ?? null`: an empty string
    // stays an empty string, `None` lands as null), while the extract
    // payload includes the pair only when truthy (the spread
    // conditionals: `Some("")` is omitted, `Some(" ")` is kept —
    // truthiness is non-emptiness, not trim), and `sql` rides the payload
    // raw (the guard trims, the payload does not).

    #[test]
    fn the_history_scope_rides_verbatim_and_the_extract_scope_rides_truthy() {
        let commands = RecordingCommands::new();
        let mut scheduler = MemoryPatternScheduler::default();
        extract_memory_patterns_if_needed(
            &commands,
            &mut scheduler,
            &MemoryPatternExtractionInput {
                sql: "  SELECT 1  ".to_owned(),
                connection_id: Some(String::new()),
                database: Some(" ".to_owned()),
                auto_learn: true,
                now: Some(NOW),
            },
        );
        assert_eq!(
            commands.history_calls(),
            vec![QueryHistoryPageParams {
                limit: 100,
                // `?? null` — the empty string survives verbatim.
                connection_id: Some(String::new()),
                database: Some(" ".to_owned()),
                since: Some(SINCE.to_owned()),
                order: HistorySort::Recent,
            }]
        );
        assert_eq!(
            commands.extract_calls(),
            vec![AiMemoryExtractionInput {
                // The payload's sql is the raw input — the guard trimmed
                // it only for the emptiness test.
                sql: "  SELECT 1  ".to_owned(),
                // `Some("")` is falsy → the field is omitted (None).
                connection_id: None,
                // `Some(" ")` is truthy → kept, untrimmed.
                database: Some(" ".to_owned()),
                // The inert default page: no samples.
                history: Some(Vec::new()),
                window_days: Some(MEMORY_PATTERN_WINDOW_DAYS),
                min_occurrences: Some(MEMORY_PATTERN_MIN_OCCURRENCES),
                model_output_json: None,
            }]
        );
        // The `None` arms — null on the history read, omitted from the
        // extract payload — in their own pass (a distinct scope key).
        extract_memory_patterns_if_needed(
            &commands,
            &mut scheduler,
            &MemoryPatternExtractionInput {
                sql: "SELECT 2".to_owned(),
                connection_id: None,
                database: None,
                auto_learn: true,
                now: Some(NOW),
            },
        );
        assert_eq!(commands.history_calls().len(), 2);
        assert_eq!(commands.history_calls()[1].connection_id, None);
        assert_eq!(commands.history_calls()[1].database, None);
        assert_eq!(commands.extract_calls()[1].connection_id, None);
        assert_eq!(commands.extract_calls()[1].database, None);
    }

    // [tester] The history filter — `entry.success && entry.sql.trim().
    // length > 0` — trims only to test emptiness: a padded statement
    // survives and rides the sample raw, a whitespace-only statement is
    // dropped, a failed statement is dropped, and the sample's
    // `observedAt` is the entry's own `executedAt`.

    #[test]
    fn the_history_filter_keeps_only_successful_non_blank_statements_raw() {
        let commands = RecordingCommands::new();
        commands.script_history(Ok(page(vec![
            entry("padded", "  SELECT padded  ", true),
            entry("blank", "\t\n", true),
            entry("failed", "SELECT broken", false),
        ])));
        commands.script_extract(Ok(Vec::new()));
        let mut scheduler = MemoryPatternScheduler::default();
        extract_memory_patterns_if_needed(&commands, &mut scheduler, &input("SELECT 1", true));
        assert_eq!(
            commands.extract_calls()[0].history,
            Some(vec![AiMemorySqlSample {
                sql: "  SELECT padded  ".to_owned(),
                observed_at: "2026-04-19T12:00:00.000Z".to_owned(),
            }])
        );
    }

    // [tester] The scope key is per connection/database — two scopes run
    // their own passes concurrently, and closing one leaves the other's
    // in flight (a keyless port would coalesce the burst across scopes).

    #[test]
    fn separate_scopes_do_not_share_an_in_flight_pass() {
        let commands = RecordingCommands::new();
        let mut scheduler = MemoryPatternScheduler::default();
        let first = scheduler.submit(input("SELECT first", true));
        let other = scheduler.submit(MemoryPatternExtractionInput {
            sql: "SELECT other".to_owned(),
            connection_id: Some("conn-2".to_owned()),
            database: Some("sales".to_owned()),
            auto_learn: true,
            now: Some(NOW),
        });
        // The second scope started its own pass — no coalescing across the
        // NUL-joined key.
        assert!(matches!(other, SubmitDecision::Run { .. }));
        let (key, first_input) = match first {
            SubmitDecision::Run { key, input } => (key, input),
            decision => panic!("expected a started pass, got {decision:?}"),
        };
        assert!(extract_once(&commands, &first_input).is_ok());
        // Closing the first scope's pass leaves the second scope's pass
        // in flight — and a fresh submit for the first scope starts again
        // instead of joining it.
        assert!(scheduler.finish(&key, Ok(())).is_none());
        assert!(matches!(
            scheduler.submit(input("SELECT third", true)),
            SubmitDecision::Run { .. }
        ));
    }

    // [tester] Mixed driving — the one-shot against a scope whose pass is
    // already in flight joins it exactly like a bare `submit`: it starts
    // nothing (no host call) and queues its input as the pending rerun the
    // step-wise host will hand back.

    #[test]
    fn the_one_shot_joins_an_in_flight_scope_instead_of_duplicating_it() {
        let commands = RecordingCommands::new();
        let mut scheduler = MemoryPatternScheduler::default();
        let first = scheduler.submit(input("SELECT first", true));
        let (key, first_input) = match first {
            SubmitDecision::Run { key, input } => (key, input),
            decision => panic!("expected a started pass, got {decision:?}"),
        };
        // The synchronous whole-loop arm meets the same in-flight fact
        // the bare submit does.
        extract_memory_patterns_if_needed(&commands, &mut scheduler, &input("SELECT joiner", true));
        assert!(commands.history_calls().is_empty());
        assert!(commands.extract_calls().is_empty());
        assert!(extract_once(&commands, &first_input).is_ok());
        // The joiner is the pending rerun the in-flight pass hands back.
        let rerun = scheduler
            .finish(&key, Ok(()))
            .expect("the joined input reruns");
        assert_eq!(rerun.sql, "SELECT joiner");
    }

    // [tester] The lean mirror's literals — the React union's own
    // spellings, and the commands wrapper's `order ?? 'recent'` default.

    #[test]
    fn history_sort_keeps_the_react_literals() {
        let table = [
            (HistorySort::Recent, "recent"),
            (HistorySort::Oldest, "oldest"),
            (HistorySort::Slowest, "slowest"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
        }
        assert_eq!(HistorySort::default(), HistorySort::Recent);
    }

    // [tester] `now: None` is React's `undefined` — the pass falls back to
    // the wall clock (the relative_time precedent's read), so the window
    // is still a valid ISO cut. Only the fallback itself is pinned; the
    // exact cut is the clock's, not the suite's.

    #[test]
    fn a_missing_now_falls_back_to_the_wall_clock() {
        let commands = RecordingCommands::new();
        let outcome = extract_once(
            &commands,
            &MemoryPatternExtractionInput {
                now: None,
                ..input("SELECT 1", true)
            },
        );
        assert_eq!(outcome, Ok(()));
        let calls = commands.history_calls();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].since.is_some(),
            "the wall-clock fallback yields a valid window"
        );
    }

    #[test]
    fn the_extraction_constants_are_the_react_literals() {
        // [tester] Supplementary: the payload pins assert the window and
        // the occurrence count symbolically (`Some(MEMORY_PATTERN_…)`),
        // so drifting either constant escapes them — the window's literal
        // only surfaces through the since-ISO pin, and the occurrence
        // count has no literal channel at all (a 3→4 drift survived every
        // other test). The literals are the extraction contract the host
        // applies, so they are pinned at their source.
        assert_eq!(MEMORY_PATTERN_WINDOW_DAYS, 14);
        assert_eq!(MEMORY_PATTERN_MIN_OCCURRENCES, 3);
    }
}
