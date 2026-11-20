//! Backup / restore progress formatting and log bounding — ported from React
//! `packages/frontend/src/lib/backupProgress.ts` (task T310, seg2 lib layer).
//!
//! Every item React's `useI18n` / `getTranslation` hands to `formatBackupProgress`
//! and friends is a pure derivation: choose an i18n key, choose its params, or
//! compute a `0.0..=1.0` ratio, with no Rune tree and no state. The `t` callback
//! ([`Translate`]) is the Rust twin of React's `(key, params?) => string`, so a
//! host supplies its own i18n implementation and the tests can pin exactly
//! which key and params each branch selects.
//!
//! [`ProgressLogPump`] is `createProgressLogPump` without the 80 ms timer:
//! Rune renders immediate-mode, so a push is visible at once and `flush` is a
//! no-op kept only so the React call sites stay 1:1. The log bounds are the
//! React ones verbatim — see [`MAX_PROGRESS_LOG_LINES`] and the in-file tests.

// The production consumers (the backup window host) are not wired yet; until
// they are, the component tests are the only callers, so silence dead_code
// locally rather than with a crate-wide allow — the same note as the ui crate's
// backup module.
#![allow(dead_code)]

/// React `BackupDumpPhase` (`'object' | 'writing' | 'done'`). `Object` is the
/// default, the way an untouched `BackupProgressPayload` starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BackupDumpPhase {
    #[default]
    Object,
    Writing,
    Done,
}

/// React `BackupProgressPayload`. `current`/`total` are `i64` rather than the
/// ui view's `u64` because React's `number` can be negative, and the ratio's
/// `total <= 0` / `current <= 0` fallbacks are pinned by tests here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackupProgressPayload {
    pub current: i64,
    pub total: i64,
    pub object_name: String,
    pub phase: BackupDumpPhase,
}

/// The subset of i18n keys this file can ask for — the literal strings from
/// `backupProgress.ts`. Exposed so hosts and tester can pin the exact contract.
pub const KEY_PROGRESS_WRITING: &str = "backup.progressWriting";
pub const KEY_SUCCESS: &str = "backup.success";
pub const KEY_PROGRESS_OBJECT: &str = "backup.progressObject";
pub const KEY_PROGRESS_PREPARING: &str = "backup.progressPreparing";
pub const KEY_RESTORE_SUCCESS: &str = "backup.restoreSuccess";
pub const KEY_RESTORE_PREPARING: &str = "backup.restorePreparing";
pub const KEY_RESTORE_PROGRESS: &str = "backup.restoreProgress";
pub const KEY_RESTORING: &str = "backup.restoring";
pub const KEY_LOG_OMITTED: &str = "backup.logOmitted";

/// The translation callback, Rust twin of React's `TFunc`. `params` is empty
/// for no-argument keys; entries are already stringified, mirroring the values
/// React passes (numbers as decimal text, `'…'` verbatim for an unknown total).
pub trait Translate {
    fn translate(&self, key: &str, params: &[(&str, String)]) -> String;
}

/// Any closure accepting `(key, params)` is a [`Translate`], so hosts and tests
/// can pass one directly wherever a `&dyn Translate` is expected.
impl<F> Translate for F
where
    F: Fn(&str, &[(&str, String)]) -> String,
{
    fn translate(&self, key: &str, params: &[(&str, String)]) -> String {
        self(key, params)
    }
}

/// `formatBackupProgress`: the status line for a backup progress payload.
///
/// Branch order matters and is pinned: `writing` first, then `done`, then the
/// object line only when `total > 0` **and** the object name is non-empty;
/// otherwise the preparing fallback.
pub fn format_backup_progress(payload: &BackupProgressPayload, t: &dyn Translate) -> String {
    match payload.phase {
        BackupDumpPhase::Writing => t.translate(KEY_PROGRESS_WRITING, &[]),
        BackupDumpPhase::Done => t.translate(KEY_SUCCESS, &[]),
        BackupDumpPhase::Object => {
            if payload.total > 0 && !payload.object_name.is_empty() {
                t.translate(
                    KEY_PROGRESS_OBJECT,
                    &[
                        ("name", payload.object_name.clone()),
                        ("current", payload.current.to_string()),
                        ("total", payload.total.to_string()),
                    ],
                )
            } else {
                t.translate(KEY_PROGRESS_PREPARING, &[])
            }
        }
    }
}

/// `backupProgressRatio`: the progress-bar fill, `0.0..=1.0`.
///
/// `done` and `writing` pin `1.0` / `0.95`. With an unknown/zero `total` the
/// ratio grows logarithmically with `current` (capped at `0.9`); with a real
/// total it is `current / total` capped at `0.94` so the bar never reads full
/// before the `writing`/`done` phase takes over.
pub fn backup_progress_ratio(payload: &BackupProgressPayload) -> f64 {
    match payload.phase {
        BackupDumpPhase::Done => 1.0,
        BackupDumpPhase::Writing => 0.95,
        BackupDumpPhase::Object => {
            if payload.total <= 0 {
                if payload.current <= 0 {
                    0.0
                } else {
                    (0.12 + (payload.current as f64 + 1.0).log10() / 5.0).min(0.9)
                }
            } else {
                (payload.current as f64 / payload.total as f64).min(0.94)
            }
        }
    }
}

/// `formatRestoreProgress`: the status line for a restore progress payload.
///
/// Unlike the backup form, an object line is emitted whenever the object name
/// is non-empty, and an unknown total renders as the single char `…` (React's
/// `total: '…'` branch) rather than falling back to the preparing key.
pub fn format_restore_progress(payload: &BackupProgressPayload, t: &dyn Translate) -> String {
    match payload.phase {
        BackupDumpPhase::Done => t.translate(KEY_RESTORE_SUCCESS, &[]),
        BackupDumpPhase::Writing => t.translate(KEY_RESTORE_PREPARING, &[]),
        BackupDumpPhase::Object => {
            if !payload.object_name.is_empty() {
                let total = if payload.total > 0 {
                    payload.total.to_string()
                } else {
                    "…".to_owned()
                };
                t.translate(
                    KEY_RESTORE_PROGRESS,
                    &[
                        ("name", payload.object_name.clone()),
                        ("current", payload.current.to_string()),
                        ("total", total),
                    ],
                )
            } else {
                t.translate(KEY_RESTORING, &[])
            }
        }
    }
}

/// `MAX_PROGRESS_LOG_LINES` (`backupProgress.ts`).
pub const MAX_PROGRESS_LOG_LINES: usize = 1500;
/// `MAX_PROGRESS_LOG_CHARS` (`backupProgress.ts`): 256 KiB hard cap.
pub const MAX_PROGRESS_LOG_CHARS: usize = 256 * 1024;
/// `MAX_LINE_CHARS` (`backupProgress.ts`).
const MAX_LINE_CHARS: usize = 4096;
/// `LOG_HEAD_KEEP` (`backupProgress.ts`): the restore-start lines kept on a trim.
const LOG_HEAD_KEEP: usize = 100;
/// `LOG_TAIL_KEEP` (`backupProgress.ts`): lines − head − one marker.
const LOG_TAIL_KEEP: usize = MAX_PROGRESS_LOG_LINES - LOG_HEAD_KEEP - 1;

fn total_chars(lines: &[String]) -> usize {
    lines.iter().map(String::len).sum()
}

/// `omittedLabel`: the localized `… N lines omitted …` marker. With no `t`
/// (React's optional argument) the English fallback is used verbatim.
fn omitted_label(omitted: usize, t: Option<&dyn Translate>) -> String {
    match t {
        Some(t) => t.translate(KEY_LOG_OMITTED, &[("count", omitted.to_string())]),
        None => format!("… {omitted} lines omitted …"),
    }
}

/// Recognize `OMITTED_MARKER_RE = /^…\s.*?(\d+)/`: a line starting with `…`,
/// then whitespace, then the first run of digits anywhere after it. A count
/// carried in an earlier marker is folded into the next one (see
/// `trimProgressLog`). Rust's `char::is_whitespace` is a hair broader than JS
/// `\s` for a few exotic code points; no caller can produce those here.
fn omitted_count(line: &str) -> Option<usize> {
    let rest = line.strip_prefix('…')?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<usize>().ok()
}

/// `trimLine`: truncate one oversized line (e.g. a long error dump) to bound
/// memory. `MAX_LINE_CHARS` is a byte index; back off to a UTF-8 char boundary
/// so a multi-byte character is never split (a JS slice counts UTF-16 units,
/// an accepted divergence for these ASCII-dominant log lines).
fn trim_line(line: &str) -> String {
    if line.len() <= MAX_LINE_CHARS {
        return line.to_owned();
    }
    let mut end = MAX_LINE_CHARS.min(line.len());
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… (truncated)", &line[..end])
}

/// `trimProgressLog`: bound the log to the head (restore start), one omitted
/// marker, and the tail (newest entries, including any error). The character
/// budget is enforced by dropping the oldest tail lines first, then the head.
fn trim_progress_log(lines: Vec<String>, t: Option<&dyn Translate>) -> Vec<String> {
    if lines.len() <= MAX_PROGRESS_LOG_LINES && total_chars(&lines) <= MAX_PROGRESS_LOG_CHARS {
        return lines;
    }
    let head_count = LOG_HEAD_KEEP.min(lines.len());
    let mut tail_start = lines.len().saturating_sub(LOG_TAIL_KEEP).max(head_count);

    let mut total = total_chars(&lines[..head_count]) + total_chars(&lines[tail_start..]);
    while total > MAX_PROGRESS_LOG_CHARS && tail_start < lines.len() - 1 {
        total -= lines[tail_start].len();
        tail_start += 1;
    }
    let mut head_end = head_count;
    while total > MAX_PROGRESS_LOG_CHARS && head_end > 1 {
        head_end -= 1;
        total -= lines[head_end].len();
    }
    let kept_head = &lines[..head_end];
    let kept_tail = &lines[tail_start..];

    // Count every dropped line, absorbing the count of any earlier marker
    // being dropped so the new marker reports the cumulative total.
    let mut omitted = tail_start - head_end;
    if omitted > 0 {
        for line in &lines[head_end..tail_start] {
            if let Some(n) = omitted_count(line) {
                omitted += n - 1;
            }
        }
    }
    let mut out = kept_head.to_vec();
    if omitted > 0 {
        out.push(omitted_label(omitted, t));
    }
    out.extend_from_slice(kept_tail);
    out
}

/// `appendProgressLog`: append one line — blank lines and a line identical to
/// the last are dropped, oversized lines are truncated, then the log is
/// trimmed to its line and character budgets.
pub fn append_progress_log(lines: &[String], next: &str, t: Option<&dyn Translate>) -> Vec<String> {
    let trimmed = next.trim();
    if trimmed.is_empty() {
        return lines.to_vec();
    }
    if lines.last().is_some_and(|last| last == trimmed) {
        return lines.to_vec();
    }
    let mut out = lines.to_vec();
    out.push(trim_line(trimmed));
    trim_progress_log(out, t)
}

/// `createProgressLogPump` without its timer. The pump owns the lines;
/// [`reset`](Self::reset) replaces them, [`push`](Self::push) appends through
/// [`append_progress_log`], and [`flush`](Self::flush) is a no-op that keeps
/// the React call sites 1:1. There is no 80 ms batching: a push is visible on
/// the next frame.
#[derive(Clone, Debug, Default)]
pub struct ProgressLogPump {
    lines: Vec<String>,
}

impl ProgressLogPump {
    pub fn new() -> Self {
        Self::default()
    }

    /// The pump's lines, for the caller to copy into the Vm.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// `reset(initial)`: replace the log with `initial`, immediately (React's
    /// `reset` also calls `setLines` at once).
    pub fn reset(&mut self, initial: Vec<String>) {
        self.lines = initial;
    }

    /// `push(line)`: append through [`append_progress_log`].
    pub fn push(&mut self, line: &str, t: Option<&dyn Translate>) {
        self.lines = append_progress_log(&self.lines, line, t);
    }

    /// `flush()`: a no-op here — the caller reads [`lines`](Self::lines) after
    /// every pump step, so there is nothing pending to apply.
    pub fn flush(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Translate` that records each `(key, params)` call in a shared buffer
    /// and echoes the key. Its `backup.logOmitted` arm returns React's English
    /// fallback so omission markers can be recognized by `omitted_count` in
    /// the trimming tests.
    struct Recording<'a> {
        calls: &'a std::cell::RefCell<Vec<(String, Vec<(String, String)>)>>,
    }

    impl Translate for Recording<'_> {
        fn translate(&self, key: &str, params: &[(&str, String)]) -> String {
            self.calls.borrow_mut().push((
                key.to_owned(),
                params
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), v.clone()))
                    .collect(),
            ));
            if key == KEY_LOG_OMITTED {
                let count = params
                    .iter()
                    .find(|(k, _)| *k == "count")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("0");
                return format!("… {count} lines omitted …");
            }
            key.to_owned()
        }
    }

    fn echo(key: &str, _params: &[(&str, String)]) -> String {
        key.to_owned()
    }

    fn payload(
        phase: BackupDumpPhase,
        current: i64,
        total: i64,
        name: &str,
    ) -> BackupProgressPayload {
        BackupProgressPayload {
            current,
            total,
            object_name: name.to_owned(),
            phase,
        }
    }

    // ---- format_backup_progress ------------------------------------------

    #[test]
    fn backup_format_picks_writing_then_done_first() {
        assert_eq!(
            format_backup_progress(&payload(BackupDumpPhase::Writing, 1, 2, "orders"), &echo),
            KEY_PROGRESS_WRITING
        );
        assert_eq!(
            format_backup_progress(&payload(BackupDumpPhase::Done, 1, 2, "orders"), &echo),
            KEY_SUCCESS
        );
    }

    #[test]
    fn backup_format_object_requires_positive_total_and_nonempty_name() {
        assert_eq!(
            format_backup_progress(&payload(BackupDumpPhase::Object, 1, 2, "orders"), &echo),
            KEY_PROGRESS_OBJECT
        );
        // total == 0 -> fallback even with a name.
        assert_eq!(
            format_backup_progress(&payload(BackupDumpPhase::Object, 1, 0, "orders"), &echo),
            KEY_PROGRESS_PREPARING
        );
        // negative total -> fallback.
        assert_eq!(
            format_backup_progress(&payload(BackupDumpPhase::Object, 1, -3, "orders"), &echo),
            KEY_PROGRESS_PREPARING
        );
        // empty name -> fallback even with a total.
        assert_eq!(
            format_backup_progress(&payload(BackupDumpPhase::Object, 1, 2, ""), &echo),
            KEY_PROGRESS_PREPARING
        );
    }

    #[test]
    fn backup_format_object_passes_name_current_total_as_strings() {
        let calls = std::cell::RefCell::new(Vec::new());
        let t = Recording { calls: &calls };
        let result = format_backup_progress(
            &payload(BackupDumpPhase::Object, 2, 5, "orders"),
            &t as &dyn Translate,
        );
        assert_eq!(result, KEY_PROGRESS_OBJECT);
        let calls = calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, KEY_PROGRESS_OBJECT);
        assert_eq!(
            calls[0].1,
            vec![
                ("name".to_owned(), "orders".to_owned()),
                ("current".to_owned(), "2".to_owned()),
                ("total".to_owned(), "5".to_owned()),
            ]
        );
    }

    #[test]
    fn default_payload_falls_back_to_preparing() {
        let out = format_backup_progress(&BackupProgressPayload::default(), &echo);
        assert_eq!(out, KEY_PROGRESS_PREPARING);
    }

    // ---- backup_progress_ratio -------------------------------------------

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn ratio_pins_phase_constants() {
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Done, 0, 0, "")),
            1.0,
        );
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Writing, 0, 0, "")),
            0.95,
        );
    }

    #[test]
    fn ratio_with_zero_or_negative_total_uses_log_fallback() {
        // current <= 0 -> 0 for both total == 0 and negative total.
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Object, 0, 0, "")),
            0.0,
        );
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Object, -3, 0, "")),
            0.0,
        );
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Object, -3, -7, "")),
            0.0,
        );
        // current = 9 -> 0.12 + log10(10)/5 = 0.32.
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Object, 9, 0, "")),
            0.32,
        );
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Object, 9, -5, "")),
            0.32,
        );
        // The 0.9 cap kicks in for a large current.
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Object, 100_000, 0, "")),
            0.9,
        );
    }

    #[test]
    fn ratio_with_real_total_is_current_over_total_capped_at_094() {
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Object, 1, 4, "")),
            0.25,
        );
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Object, 3, 4, "")),
            0.75,
        );
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Object, 100, 50, "")),
            0.94,
        );
        // Negative current with a positive total is a (negative) raw ratio,
        // still under the cap — pinned to the JS arithmetic, not clamped at 0.
        close(
            backup_progress_ratio(&payload(BackupDumpPhase::Object, -1, 4, "")),
            -0.25,
        );
    }

    // ---- format_restore_progress -----------------------------------------

    #[test]
    fn restore_format_picks_phase_keys_first() {
        assert_eq!(
            format_restore_progress(&payload(BackupDumpPhase::Done, 1, 2, "x"), &echo),
            KEY_RESTORE_SUCCESS
        );
        assert_eq!(
            format_restore_progress(&payload(BackupDumpPhase::Writing, 1, 2, "x"), &echo),
            KEY_RESTORE_PREPARING
        );
    }

    #[test]
    fn restore_format_uses_ellipsis_for_unknown_total() {
        let calls = std::cell::RefCell::new(Vec::new());
        let t = Recording { calls: &calls };
        let result = format_restore_progress(
            &payload(BackupDumpPhase::Object, 7, 0, "dump.sql"),
            &t as &dyn Translate,
        );
        assert_eq!(result, KEY_RESTORE_PROGRESS);
        let calls = calls.borrow();
        assert_eq!(calls[0].1[2], ("total".to_owned(), "…".to_owned()));
        assert_eq!(calls[0].1[0], ("name".to_owned(), "dump.sql".to_owned()));
        assert_eq!(calls[0].1[1], ("current".to_owned(), "7".to_owned()));
    }

    #[test]
    fn restore_format_uses_numeric_total_when_known() {
        let calls = std::cell::RefCell::new(Vec::new());
        let t = Recording { calls: &calls };
        format_restore_progress(
            &payload(BackupDumpPhase::Object, 2, 5, "dump.sql"),
            &t as &dyn Translate,
        );
        let calls = calls.borrow();
        assert_eq!(calls[0].1[2], ("total".to_owned(), "5".to_owned()));
    }

    #[test]
    fn restore_format_empty_name_falls_back_to_restoring() {
        assert_eq!(
            format_restore_progress(&payload(BackupDumpPhase::Object, 1, 2, ""), &echo),
            KEY_RESTORING
        );
        assert_eq!(
            format_restore_progress(&BackupProgressPayload::default(), &echo),
            KEY_RESTORING
        );
    }

    // ---- append_progress_log ---------------------------------------------

    #[test]
    fn append_drops_blank_and_consecutive_duplicate_lines() {
        let lines: Vec<String> = Vec::new();
        let lines = append_progress_log(&lines, "   ", None);
        assert!(lines.is_empty(), "blank lines are ignored");
        let lines = append_progress_log(&lines, "  hello  ", None);
        assert_eq!(lines, ["hello"]);
        let lines = append_progress_log(&lines, " hello ", None);
        assert_eq!(
            lines,
            ["hello"],
            "trimmed duplicate of the last line is ignored"
        );
        let lines = append_progress_log(&lines, "world", None);
        assert_eq!(lines, ["hello", "world"]);
    }

    #[test]
    fn append_truncates_an_oversized_line() {
        let long = "x".repeat(5000);
        let lines = append_progress_log(&[], &long, None);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("… (truncated)"));
        assert!(lines[0].starts_with(&"x".repeat(4096)));
    }

    #[test]
    fn append_truncates_multibyte_line_on_a_char_boundary() {
        let long = "你".repeat(2000);
        let lines = append_progress_log(&[], &long, None);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("… (truncated)"));
        // Still valid UTF-8 (it is a String); no mid-character slice panic.
        assert_eq!(lines[0].chars().last(), Some(')'));
    }

    #[test]
    fn append_bounds_to_the_line_budget_with_an_omission_marker() {
        let mut lines: Vec<String> = Vec::new();
        for i in 0..1501 {
            lines = append_progress_log(&lines, &format!("line-{i:04}"), None);
        }
        assert_eq!(lines.len(), MAX_PROGRESS_LOG_LINES);
        assert_eq!(lines[99], "line-0099");
        assert_eq!(lines[100], "… 2 lines omitted …");
        assert_eq!(lines[101], "line-0102");
        assert_eq!(lines.last().unwrap(), "line-1500");
    }

    #[test]
    fn append_carries_an_earlier_marker_count_forward() {
        let mut lines: Vec<String> = Vec::new();
        for i in 0..1501 {
            lines = append_progress_log(&lines, &format!("line-{i:04}"), None);
        }
        assert_eq!(lines[100], "… 2 lines omitted …");
        lines = append_progress_log(&lines, "line-1501", None);
        assert_eq!(lines.len(), MAX_PROGRESS_LOG_LINES);
        // The dropped lines are the old marker ("… 2 …") plus one line, so
        // the new marker reports 2 + (2 - 1) = 3.
        assert_eq!(lines[100], "… 3 lines omitted …");
        assert_eq!(lines[101], "line-0103");
        assert_eq!(lines.last().unwrap(), "line-1501");
    }

    #[test]
    fn append_bounds_to_the_character_budget_and_keeps_the_newest() {
        let mut lines: Vec<String> = Vec::new();
        for i in 0..1600 {
            let line = format!("{i:04}{}", "x".repeat(196));
            assert_eq!(line.len(), 200);
            lines = append_progress_log(&lines, &line, None);
        }
        assert!(lines.len() < 1600);
        assert_eq!(lines[100], "… 290 lines omitted …");
        assert!(lines.last().unwrap().starts_with("1599"));
        assert!(total_chars(&lines) <= MAX_PROGRESS_LOG_CHARS);
    }

    #[test]
    fn append_uses_the_injected_translator_for_the_marker() {
        let calls = std::cell::RefCell::new(Vec::new());
        let t = Recording { calls: &calls };
        let mut lines: Vec<String> = Vec::new();
        for i in 0..1501 {
            lines =
                append_progress_log(&lines, &format!("line-{i:04}"), Some(&t as &dyn Translate));
        }
        assert_eq!(lines[100], "… 2 lines omitted …");
        let calls = calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, KEY_LOG_OMITTED);
        assert_eq!(calls[0].1, vec![("count".to_owned(), "2".to_owned())]);
    }

    // ---- ProgressLogPump --------------------------------------------------

    #[test]
    fn pump_reset_push_and_flush() {
        let mut pump = ProgressLogPump::new();
        assert!(pump.lines().is_empty());
        pump.reset(vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(pump.lines(), ["a", "b"]);
        pump.push("c", None);
        assert_eq!(pump.lines(), ["a", "b", "c"]);
        pump.push(" c ", None);
        assert_eq!(pump.lines(), ["a", "b", "c"], "duplicate is dropped");
        pump.flush();
        assert_eq!(
            pump.lines(),
            ["a", "b", "c"],
            "flush is a no-op in immediate mode"
        );
    }
}
