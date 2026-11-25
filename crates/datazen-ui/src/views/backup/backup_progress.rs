//! T026-S2: the Backup window's progress formatting and log pump.
//!
//! Ported from `packages/frontend/src/lib/backupProgress.ts`:
//! [`format_backup_progress`] / [`format_restore_progress`] /
//! [`backup_progress_ratio`] are the status-line and progress-bar
//! derivations `handleBackup` / `handleRestore` (`BackupWindow.tsx`)
//! render, and [`BackupLogPump`] is `createProgressLogPump` without
//! its 80 ms render batching — Rune renders immediate-mode, so a push
//! lands at once and [`BackupLogPump::flush`] is a no-op that keeps
//! the React call sites 1:1 (push + flush on the success/error paths).
//!
//! The pump's bounds are the React ones verbatim:
//! [`MAX_PROGRESS_LOG_LINES`] lines, [`MAX_PROGRESS_LOG_CHARS`]
//! characters, [`MAX_LINE_CHARS`] per line, and the head/tail trim
//! that keeps the newest entries (and any error) while an
//! `… N lines omitted …` marker counts what was dropped.

// The production consumer is the Backup window host (not yet
// written); until then the component tests are the only callers, so
// the dead-code lint is silenced here rather than in a crate-wide
// allow — the same note as `backup_window.rs`.
#![allow(dead_code)]

use crate::i18n::t;
use crate::views::{BackupProgress, BackupProgressPhase};

/// `MAX_PROGRESS_LOG_LINES` (`backupProgress.ts`).
pub const MAX_PROGRESS_LOG_LINES: usize = 1500;
/// `MAX_PROGRESS_LOG_CHARS` (`backupProgress.ts`): 256 KiB.
pub const MAX_PROGRESS_LOG_CHARS: usize = 256 * 1024;
/// `MAX_LINE_CHARS` (`backupProgress.ts`).
const MAX_LINE_CHARS: usize = 4096;
/// `LOG_HEAD_KEEP` (`backupProgress.ts`).
const LOG_HEAD_KEEP: usize = 100;
/// `LOG_TAIL_KEEP` (`backupProgress.ts`): lines − head − marker.
const LOG_TAIL_KEEP: usize = MAX_PROGRESS_LOG_LINES - LOG_HEAD_KEEP - 1;

/// Fill `{name}`-style placeholders in a `t()` template, in the order
/// given. React's `t(key, params)` replaces every occurrence; each
/// template here names a placeholder once.
fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = template.to_owned();
    for (key, value) in pairs {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

/// React `formatBackupProgress` (`lib/backupProgress.ts`): the status
/// line for a `backup-progress` payload.
pub fn format_backup_progress(payload: &BackupProgress) -> String {
    match payload.phase {
        BackupProgressPhase::Writing => t("backup.progressWriting").to_owned(),
        BackupProgressPhase::Done => t("backup.success").to_owned(),
        BackupProgressPhase::Object => {
            if payload.total > 0 && !payload.object_name.is_empty() {
                fill(
                    t("backup.progressObject"),
                    &[
                        ("name", payload.object_name.as_str()),
                        ("current", &payload.current.to_string()),
                        ("total", &payload.total.to_string()),
                    ],
                )
            } else {
                t("backup.progressPreparing").to_owned()
            }
        }
    }
}

/// React `formatRestoreProgress` (`lib/backupProgress.ts`): the status
/// line for a `restore-progress` payload, as `runSqlFileExecution`
/// (`lib/sqlFileExecution.ts`) formats it. An unknown total renders
/// as `…`, like the JS `total: '…'` branch.
pub fn format_restore_progress(payload: &BackupProgress) -> String {
    match payload.phase {
        BackupProgressPhase::Done => t("backup.restoreSuccess").to_owned(),
        BackupProgressPhase::Writing => t("backup.restorePreparing").to_owned(),
        BackupProgressPhase::Object => {
            if !payload.object_name.is_empty() {
                let total = if payload.total > 0 {
                    payload.total.to_string()
                } else {
                    "…".to_owned()
                };
                fill(
                    t("backup.restoreProgress"),
                    &[
                        ("name", payload.object_name.as_str()),
                        ("current", &payload.current.to_string()),
                        ("total", total.as_str()),
                    ],
                )
            } else {
                t("backup.restoring").to_owned()
            }
        }
    }
}

/// React `backupProgressRatio` (`lib/backupProgress.ts`): the progress
/// bar's fill, 0.0–1.0.
pub fn backup_progress_ratio(payload: &BackupProgress) -> f64 {
    match payload.phase {
        BackupProgressPhase::Done => 1.0,
        BackupProgressPhase::Writing => 0.95,
        BackupProgressPhase::Object => {
            if payload.total == 0 {
                if payload.current == 0 {
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

fn total_chars(lines: &[String]) -> usize {
    lines.iter().map(String::len).sum()
}

/// `omittedLabel` (`backupProgress.ts`): the localized
/// `… N lines omitted …` marker.
fn omitted_label(omitted: usize) -> String {
    fill(t("backup.logOmitted"), &[("count", &omitted.to_string())])
}

/// The `… N` prefix that marks an omitted-lines marker, so a later
/// trim can recognise it and carry its count forward
/// (`OMITTED_MARKER_RE` in `backupProgress.ts`: `^…\s.*?(\d+)` — the
/// count is the first digit run after the `…` and its whitespace, so
/// the zh-CN `… 已省略 5 行 …` marker counts too).
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

/// `trimLine` (`backupProgress.ts`): truncate one oversized line.
fn trim_line(line: &str) -> String {
    if line.len() <= MAX_LINE_CHARS {
        line.to_owned()
    } else {
        // Back off to a char boundary: `MAX_LINE_CHARS` is a byte
        // index, and slicing a multi-byte character mid-way panics.
        let mut end = MAX_LINE_CHARS.min(line.len());
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}… (truncated)", &line[..end])
    }
}

/// `trimProgressLog` (`backupProgress.ts`): bound the log to the head
/// (restore start), one omitted marker, and the tail (newest entries).
fn trim_progress_log(lines: Vec<String>) -> Vec<String> {
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

    let mut omitted = tail_start - head_end;
    if omitted > 0 {
        for line in &lines[head_end..tail_start] {
            if let Some(n) = omitted_count(line) {
                omitted += n - 1;
            }
        }
    }
    let marker = if omitted > 0 {
        vec![omitted_label(omitted)]
    } else {
        Vec::new()
    };
    let mut out = kept_head.to_vec();
    out.extend(marker);
    out.extend_from_slice(kept_tail);
    out
}

/// `appendProgressLog` (`backupProgress.ts`): append one line —
///
/// blank lines are dropped, a line identical to the last one is
/// dropped, the line is truncated, and the log is trimmed to budget.
pub fn append_progress_log(lines: &[String], next: &str) -> Vec<String> {
    let trimmed = next.trim();
    if trimmed.is_empty() {
        return lines.to_vec();
    }
    if lines.last().is_some_and(|last| last == trimmed) {
        return lines.to_vec();
    }
    let mut out = lines.to_vec();
    out.push(trim_line(trimmed));
    trim_progress_log(out)
}

/// `createProgressLogPump` (`backupProgress.ts`) without its timer:
/// the pump owns the lines; [`BackupLogPump::reset`] replaces them,
/// [`BackupLogPump::push`] appends through [`append_progress_log`],
/// and [`BackupLogPump::flush`] applies them — in Rune that means the
/// caller copies [`BackupLogPump::lines`] into the Vm, so flush is a
/// no-op that exists to keep the React call sites 1:1. There is no
/// 80 ms batching: a push is visible on the next frame.
#[derive(Clone, Debug, Default)]
pub struct BackupLogPump {
    lines: Vec<String>,
}

impl BackupLogPump {
    pub fn new() -> Self {
        Self::default()
    }

    /// The pump's lines, for the caller to copy into the Vm.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// `reset(initial)`: replace the log with `initial`, immediately
    /// (React's `reset` also calls `setLines` at once).
    pub fn reset(&mut self, initial: Vec<String>) {
        self.lines = initial;
    }

    /// `push(line)`: append through [`append_progress_log`].
    pub fn push(&mut self, line: &str) {
        self.lines = append_progress_log(&self.lines, line);
    }

    /// `flush()`: apply the lines. A no-op here — the caller reads
    /// [`BackupLogPump::lines`] after every pump step, so there is
    /// nothing pending to apply.
    pub fn flush(&mut self) {}
}
