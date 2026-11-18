//! T012 independent audit: `AiWriteProposalHost`.
//!
//! Fresh vectors against the React source
//! (`packages/frontend/src/components/AiWriteProposalHost.tsx`) and the
//! zh-CN pack (`packages/frontend/src/locales/zh-CN/ai.ts`), written without
//! reusing the coder's clocks, ids, SQL or sizes: a different base clock
//! (2027-01-16), different proposal ids / connection / SQL, and the
//! 1440x816 geometry the task names (the coder used 1440x900). Runtime
//! tests read the laid-out element tree from `rt.elements()` — never the
//! component's own report of itself.

use chrono::{DateTime, SecondsFormat};
use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::ai::{
    AiWriteApplyReceipt, AiWriteImpact, AiWriteProposal, AiWriteProposalHost,
    AiWriteProposalHostState, AiWriteStatement,
};
use crate::i18n::t;
use crate::icons::{ICON_SHIELD_ALERT, ICON_SHIELD_CHECK};
use crate::{view, AppState};

// Literals re-copied from packages/frontend/src/locales/zh-CN/ai.ts.
const TITLE: &str = "批准数据库写入";
const REMAINING_TMPL: &str = "{seconds} 秒后过期";
const EXPIRED: &str = "已过期";
const APPLIED_TMPL: &str = "写入已执行：影响 {count} 行";
const APPLIED_UNKNOWN: &str = "写入已执行：影响行数未知";
const STATEMENTS_TMPL: &str = "{count} 条语句";
const TABLES_TMPL: &str = "表：{tables}";
const RESPOND_FAILED: &str = "无法提交写入决定";
const AUDIT_FAILED: &str = "AI 写入审计失败";

/// Audit clock: 2027-01-16T08:00:00Z in millis (coder used 2027-01-15).
const NOW_A: u64 = 1_800_086_400_000;
const SQL_A: &str = "DELETE FROM sessions WHERE expired = 1";
const HASH_A: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn expires_at(ms: u64) -> String {
    DateTime::from_timestamp_millis(ms as i64)
        .unwrap()
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn proposal_a() -> AiWriteProposal {
    let mut impact = AiWriteImpact::new("2 statement(s): DELETE");
    impact.statement_count = 2;
    impact.tables = vec!["sessions".to_string()];
    impact.has_where = true;
    impact.writes_data = true;
    let mut p = AiWriteProposal::new("audit-p1", "audit-conn", SQL_A, expires_at(NOW_A + 120_000));
    p.database = "audit_db".to_string();
    p.sql_hash = HASH_A.to_string();
    p.impact = impact;
    p.statements = vec![AiWriteStatement::new(0, "DELETE", "c".repeat(64))];
    p
}

fn proposal_b() -> AiWriteProposal {
    let mut p = proposal_a();
    p.proposal_id = "audit-p2".to_string();
    p.sql = "UPDATE sessions SET expired = 0 WHERE id = 7".to_string();
    p
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

struct Probe {
    host: AiWriteProposalHostState,
    dark: bool,
}

impl Probe {
    fn new() -> Self {
        Self {
            host: AiWriteProposalHostState::new(),
            dark: true,
        }
    }

    /// A live proposal, ticked at the audit clock: 120s remaining.
    fn live() -> Self {
        let mut probe = Self::new();
        assert!(probe.host.enqueue(proposal_a()));
        assert!(probe.host.tick(NOW_A));
        probe
    }
}

fn audit_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    AiWriteProposalHost::<Probe>::new(&state.host)
        .host_mut(|s: &mut Probe| &mut s.host)
        .build(state.dark, &mut layer);
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .child(col().key("probe-content").w_full().h_full()),
        layer,
    )
}

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, audit_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn focused_key<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

/// Tab through the focus order until `key` is focused (at most 60 stops),
/// so keyboard tests do not depend on the exact tab order.
fn focus_by_tab<S>(rt: &mut Runtime<S>, key: &str) {
    for _ in 0..60 {
        if focused_key(rt).as_deref() == Some(key) {
            return;
        }
        press(rt, Key::Tab);
    }
    panic!("{key} never took focus after 60 Tabs");
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn element_by_prefix<S>(rt: &Runtime<S>, prefix: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref().is_some_and(|k| k.starts_with(prefix)))
        .unwrap_or_else(|| panic!("an element starting with {prefix} is laid out"))
}

fn element_text<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn any_text<S>(rt: &Runtime<S>, text: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(text))
}

fn any_text_containing<S>(rt: &Runtime<S>, needle: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref().is_some_and(|t| t.contains(needle)))
}

fn rects_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] - 0.5
        && b[0] < a[0] + a[2] - 0.5
        && a[1] < b[1] + b[3] - 0.5
        && b[1] < a[1] + a[3] - 0.5
}

fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[0] + r[2] > w + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) horizontally out of window {w}: {outside:?}",
        outside.len()
    );
}

// ---- state machine: fresh vectors -------------------------------------------

#[test]
fn audit_whitespace_only_id_and_sql_are_accepted_like_react_truthiness() {
    // React drops only falsy `proposalId` / `sql` (`!proposal.proposalId`);
    // a whitespace-only string is truthy there, so it must queue here too.
    let mut host = AiWriteProposalHostState::new();
    let mut p = proposal_a();
    p.proposal_id = " ".to_string();
    p.sql = " ".to_string();
    assert!(host.enqueue(p));
    assert_eq!(host.queue_len(), 1);
}

#[test]
fn audit_requeue_after_drop_is_allowed_but_expiry_autodeny_is_not_rearmed() {
    // React's `timedOutRef` is never cleared, so a re-queued id that
    // expires again is NOT auto-denied a second time; the Rune `timed_out`
    // set must match. Manual deny still works (allow stays blocked: expired).
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal_a()));
    assert!(host.tick(NOW_A + 120_000));
    assert_eq!(host.deny_requests(), ["audit-p1"]);
    host.deny_succeeded("audit-p1");
    assert!(host.enqueue(proposal_a()));
    assert!(
        !host.tick(NOW_A + 120_000),
        "the re-tick is quiet: remaining already 0, no second auto-deny"
    );
    assert_eq!(
        host.deny_requests(),
        ["audit-p1"],
        "no second auto-deny for the same id"
    );
    assert!(host.expired());
    host.allow();
    assert!(host.apply_requests().is_empty());
    host.deny();
    assert_eq!(host.deny_requests(), ["audit-p1", "audit-p1"]);
}

#[test]
fn audit_tick_dirty_flag_only_on_change_or_deny() {
    let mut host = AiWriteProposalHostState::new();
    assert!(!host.tick(NOW_A), "empty queue tick is quiet");
    assert!(host.enqueue(proposal_a()));
    assert!(host.tick(NOW_A));
    assert_eq!(host.remaining_ms(), 120_000);
    assert!(!host.tick(NOW_A), "same clock, same remaining: quiet");
    assert!(host.tick(NOW_A + 1), "1ms less remaining is dirty");
    assert_eq!(host.remaining_ms(), 119_999);
}

#[test]
fn audit_remaining_seconds_boundaries() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal_a()));
    assert!(host.tick(NOW_A));
    assert_eq!(host.remaining_seconds(), 120);
    assert!(host.tick(NOW_A + 119_000));
    assert_eq!(host.remaining_ms(), 1_000);
    assert_eq!(host.remaining_seconds(), 1);
    assert!(host.tick(NOW_A + 118_999));
    assert_eq!(host.remaining_ms(), 1_001);
    assert_eq!(host.remaining_seconds(), 2);
    assert!(host.tick(NOW_A + 119_999));
    assert_eq!(host.remaining_ms(), 1);
    assert_eq!(host.remaining_seconds(), 1);
    assert!(host.tick(NOW_A + 120_000));
    assert_eq!(host.remaining_seconds(), 0);
    assert!(host.expired());
}

#[test]
fn audit_expiry_with_timezone_offset_parses() {
    // `2027-01-16T16:00:00+08:00` is 08:00Z, i.e. NOW_A; React's
    // `Date.parse` and RFC3339 agree here.
    let mut host = AiWriteProposalHostState::new();
    let mut p = proposal_a();
    p.expires_at = "2027-01-16T16:00:00+08:00".to_string();
    assert!(host.enqueue(p));
    assert!(host.tick(NOW_A - 60_000));
    assert_eq!(host.remaining_ms(), 60_000);
    assert!(!host.expired());
}

#[test]
fn audit_empty_and_garbage_expiry_expire_immediately() {
    for bad in ["", "not-a-date", "2027-13-45T99:99:99Z"] {
        let mut host = AiWriteProposalHostState::new();
        let mut p = proposal_a();
        p.expires_at = bad.to_string();
        assert!(host.enqueue(p));
        assert!(host.tick(NOW_A));
        assert!(host.expired(), "{bad:?} must read as expired");
        assert_eq!(host.deny_requests(), ["audit-p1"]);
    }
}

#[test]
fn audit_date_only_expiry_is_treated_as_expired() {
    // Divergence note (non-blocking: the backend always emits RFC3339
    // `to_rfc3339_opts(Secs, true)`): React's `Date.parse("2027-01-16")`
    // yields midnight UTC, while RFC3339 parsing rejects a date-only
    // string, so Rune reads it as expired at once.
    let mut host = AiWriteProposalHostState::new();
    let mut p = proposal_a();
    p.expires_at = "2027-01-16".to_string();
    assert!(host.enqueue(p));
    assert!(host.tick(NOW_A));
    assert!(host.expired());
    assert_eq!(host.deny_requests(), ["audit-p1"]);
}

#[test]
fn audit_tick_long_before_expiry_reports_the_full_remaining() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal_a()));
    assert!(host.tick(NOW_A - 61_000));
    assert_eq!(host.remaining_ms(), 181_000);
    assert_eq!(host.remaining_seconds(), 181);
}

#[test]
fn audit_failed_deny_rearms_allow_and_clears_the_error() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal_a()));
    assert!(host.tick(NOW_A));
    host.deny();
    host.deny_failed("denied by transport");
    assert!(!host.responding());
    assert_eq!(host.error(), Some("denied by transport"));
    host.allow();
    assert_eq!(host.error(), None, "allow clears the previous error");
    assert!(host.responding());
    assert_eq!(host.apply_requests().len(), 1);
    assert_eq!(host.apply_requests()[0].proposal_id, "audit-p1");
}

#[test]
fn audit_failed_apply_can_be_retried_and_records_again() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal_a()));
    assert!(host.tick(NOW_A));
    host.allow();
    host.apply_failed("first attempt failed");
    assert!(!host.responding());
    assert!(host.current().is_some());
    host.allow();
    assert_eq!(host.error(), None);
    assert_eq!(host.apply_requests().len(), 2);
}

#[test]
fn audit_wrong_id_answers_do_not_drop_the_current_proposal() {
    // Host-misuse contract: an answer naming another proposal clears
    // `responding` but drops nothing (and an apply answer still shows its
    // own receipt, as React's `setReceipt(applied)` would).
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal_a()));
    assert!(host.tick(NOW_A));
    host.deny();
    host.deny_succeeded("someone-else");
    assert!(!host.responding());
    assert_eq!(host.current().unwrap().proposal_id, "audit-p1");

    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal_a()));
    assert!(host.tick(NOW_A));
    host.allow();
    let mut receipt = AiWriteApplyReceipt::new("someone-else");
    receipt.row_count = Some(9);
    host.apply_succeeded(receipt);
    assert!(!host.responding());
    assert_eq!(host.current().unwrap().proposal_id, "audit-p1");
    assert_eq!(host.receipt().unwrap().proposal_id, "someone-else");
}

#[test]
fn audit_queue_advances_and_open_epoch_only_bumps_on_empty_to_current() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal_a()));
    assert!(host.enqueue(proposal_b()));
    let mut third = proposal_a();
    third.proposal_id = "audit-p3".to_string();
    assert!(host.enqueue(third));
    assert_eq!(host.open_epoch(), 1);
    host.deny();
    host.deny_succeeded("audit-p1");
    assert_eq!(host.current().unwrap().proposal_id, "audit-p2");
    assert_eq!(host.open_epoch(), 1);
    host.deny();
    host.deny_succeeded("audit-p2");
    assert_eq!(host.current().unwrap().proposal_id, "audit-p3");
    assert_eq!(host.open_epoch(), 1);
    host.deny();
    host.deny_succeeded("audit-p3");
    assert!(host.current().is_none());
    assert!(host.enqueue(proposal_a()));
    assert_eq!(host.open_epoch(), 2);
}

#[test]
fn audit_production_protected_does_not_block_allow_when_apply_allowed() {
    // React parity: the allow guard is only `responding || expired ||
    // !applyAllowed` — `productionProtected` shows a warning but does not
    // disable the button. (The warning text says "never allow"; that is a
    // React-level wording quirk, matched here, not a Rune divergence.)
    let mut host = AiWriteProposalHostState::new();
    let mut p = proposal_a();
    p.production_protected = true;
    assert!(host.enqueue(p));
    assert!(host.tick(NOW_A));
    host.allow();
    assert_eq!(host.apply_requests().len(), 1);
    assert!(host.responding());
}

#[test]
fn audit_receipt_survives_the_next_proposal_and_returns_after_it() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal_a()));
    assert!(host.tick(NOW_A));
    host.allow();
    host.apply_succeeded(AiWriteApplyReceipt::new("audit-p1"));
    assert!(host.enqueue(proposal_b()));
    assert_eq!(host.current().unwrap().proposal_id, "audit-p2");
    assert!(host.receipt().is_some(), "receipt is kept while queued");
    host.deny();
    host.deny_succeeded("audit-p2");
    assert!(host.current().is_none());
    assert!(host.receipt().is_some());
    assert!(host.is_visible());
}

#[test]
fn audit_audit_error_overwrites_and_dismiss_clears() {
    let mut host = AiWriteProposalHostState::new();
    host.set_audit_error("first audit boom");
    host.set_audit_error("second audit boom");
    assert_eq!(host.audit_error(), Some("second audit boom"));
    host.dismiss_audit_error();
    assert!(host.audit_error().is_none());
    assert!(!host.is_visible());
}

#[test]
fn audit_sync_never_drains_the_outboxes_or_the_receipt() {
    let mut app = AppState::default();
    let mut p = proposal_a();
    p.expires_at = "2020-06-01T00:00:00Z".to_string();
    assert!(app.ai_write_proposal.enqueue(p));
    assert!(app.sync_ai_write_proposals());
    assert_eq!(app.ai_write_proposal.deny_requests(), ["audit-p1"]);
    assert!(
        !app.sync_ai_write_proposals(),
        "second tick is quiet: no new deny, no drain"
    );
    assert_eq!(
        app.ai_write_proposal.deny_requests(),
        ["audit-p1"],
        "sync must not drain or duplicate the deny outbox"
    );
    // A receipt also survives repeated polls.
    let mut app = AppState::default();
    assert!(app.ai_write_proposal.enqueue(proposal_a()));
    assert!(app.ai_write_proposal.tick(NOW_A));
    app.ai_write_proposal.allow();
    app.ai_write_proposal
        .apply_succeeded(AiWriteApplyReceipt::new("audit-p1"));
    assert!(
        app.sync_ai_write_proposals(),
        "first poll after the drop resets the leftover remaining time"
    );
    assert!(!app.sync_ai_write_proposals(), "then the poll goes quiet");
    assert!(app.ai_write_proposal.receipt().is_some());
    assert_eq!(app.ai_write_proposal.apply_requests().len(), 1);
}

// ---- runtime render: fresh sizes / vectors ---------------------------------

#[test]
fn audit_dialog_role_label_and_all_react_testids() {
    let mut p = proposal_a();
    p.diff = Some(
        "-DELETE FROM sessions WHERE expired = 1\n+DELETE FROM sessions WHERE expired = 0"
            .to_string(),
    );
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(p));
    assert!(host.tick(NOW_A));
    let mut probe = Probe::new();
    probe.host = host;
    let rt = runtime(probe, 1440.0, 816.0);
    let dialog = element(&rt, "ai-write-proposal-dialog");
    assert_eq!(dialog.role, "dialog", "React role=\"dialog\"");
    assert_eq!(dialog.label.as_deref(), Some(TITLE));
    for key in [
        "ai-write-proposal-backdrop",
        "ai-write-proposal-dialog",
        "ai-write-proposal-connection",
        "ai-write-proposal-database",
        "ai-write-proposal-sql",
        "ai-write-proposal-diff",
        "ai-write-proposal-impact",
        "ai-write-proposal-summary",
        "ai-write-proposal-deny",
        "ai-write-proposal-allow",
    ] {
        assert!(has_key(&rt, key), "React data-testid {key} is present");
    }
    assert_eq!(
        element_text(&rt, "ai-write-proposal-connection"),
        "audit-conn"
    );
    assert_eq!(element_text(&rt, "ai-write-proposal-database"), "audit_db");
    assert!(any_text(&rt, SQL_A));
    assert!(any_text_containing(
        &rt,
        "+DELETE FROM sessions WHERE expired = 0"
    ));
    assert!(any_text(&rt, HASH_A));
    assert!(any_text(&rt, "2 条语句 · 表：sessions"));
    assert!(!has_key(&rt, "ai-write-proposal-error"));
}

#[test]
fn audit_geometry_1440x816_and_960x640_both_themes_no_overflow_no_overlap() {
    for (w, h) in [(1440.0, 816.0), (960.0, 640.0)] {
        for dark in [true, false] {
            let mut probe = Probe::live();
            probe.dark = dark;
            let rt = runtime(probe, w, h);
            let backdrop = element(&rt, "ai-write-proposal-backdrop");
            assert_eq!(backdrop.rect, [0.0, 0.0, w, h], "backdrop fills {w}x{h}");
            let dialog = element(&rt, "ai-write-proposal-dialog");
            let (x, y, dw, dh) = (
                dialog.rect[0],
                dialog.rect[1],
                dialog.rect[2],
                dialog.rect[3],
            );
            assert!(dw <= 672.0 + 0.5, "panel caps at max-w-2xl, got {dw}");
            assert!((x + dw / 2.0 - w / 2.0).abs() < 1.5, "centred horizontally");
            assert!((y + dh / 2.0 - h / 2.0).abs() < 1.5, "centred vertically");
            assert!(x >= -0.5 && x + dw <= w + 0.5, "panel inside the window");
            assert!(
                y >= -0.5 && y + dh <= h + 0.5,
                "panel inside the window vertically"
            );
            // Header above body above footer, no overlap; deny left of allow.
            let header = element(&rt, "ai-write-proposal-header");
            let body = element_by_prefix(&rt, "ai-write-proposal-body-");
            let footer = element(&rt, "ai-write-proposal-footer");
            assert!(header.rect[1] + header.rect[3] <= body.rect[1] + 0.5);
            assert!(body.rect[1] + body.rect[3] <= footer.rect[1] + 0.5);
            assert!(!rects_overlap(header.rect, footer.rect));
            let deny = element(&rt, "ai-write-proposal-deny");
            let allow = element(&rt, "ai-write-proposal-allow");
            assert!(!rects_overlap(deny.rect, allow.rect));
            assert!(deny.rect[0] + deny.rect[2] <= allow.rect[0] + 0.5);
            assert!(deny.rect[1] >= footer.rect[1] - 0.5);
            assert_no_horizontal_overflow(&rt, w, "dialog {w}x{h} dark={dark}");
        }
    }
}

#[test]
fn audit_receipt_hidden_while_a_proposal_is_current() {
    // React shows the receipt only when `receipt && !current`.
    let mut probe = Probe::live();
    probe.host.allow();
    probe
        .host
        .apply_succeeded(AiWriteApplyReceipt::new("audit-p1"));
    assert!(probe.host.enqueue(proposal_b()));
    let rt = runtime(probe, 1440.0, 816.0);
    assert!(has_key(&rt, "ai-write-proposal-dialog"));
    assert!(!has_key(&rt, "ai-write-applied-receipt"));
    assert_eq!(
        element_text(&rt, "ai-write-proposal-connection"),
        "audit-conn"
    );
}

#[test]
fn audit_audit_error_alone_renders_no_dialog_and_stays_inside_960x640() {
    let mut probe = Probe::new();
    probe.host.set_audit_error("audit boom alone");
    let rt = runtime(probe, 960.0, 640.0);
    assert!(!has_key(&rt, "ai-write-proposal-dialog"));
    assert!(!has_key(&rt, "ai-write-applied-receipt"));
    assert!(has_key(&rt, "ai-write-audit-error"));
    assert!(any_text(&rt, "audit boom alone"));
    let toast = element(&rt, "ai-write-audit-error");
    assert!(toast.rect[0] >= -0.5 && toast.rect[0] + toast.rect[2] <= 960.0 + 0.5);
    assert!(toast.rect[1] + toast.rect[3] <= 640.0 + 0.5);
}

#[test]
fn audit_receipt_and_audit_toasts_together_do_not_overlap() {
    for (w, h) in [(1440.0, 816.0), (960.0, 640.0)] {
        let mut probe = Probe::live();
        probe.host.allow();
        probe
            .host
            .apply_succeeded(AiWriteApplyReceipt::new("audit-p1"));
        probe.host.set_audit_error("audit boom");
        let rt = runtime(probe, w, h);
        assert!(has_key(&rt, "ai-write-applied-receipt"));
        assert!(has_key(&rt, "ai-write-audit-error"));
        let receipt = element(&rt, "ai-write-applied-receipt");
        let audit = element(&rt, "ai-write-audit-error");
        assert!(
            !rects_overlap(receipt.rect, audit.rect),
            "toasts overlap at {w}x{h}: {receipt:?} vs {audit:?}"
        );
        assert!(receipt.rect[0] + receipt.rect[2] <= w + 0.5);
        assert!(audit.rect[0] + audit.rect[2] <= w + 0.5);
    }
}

#[test]
fn audit_long_sql_and_diff_stay_capped_and_the_dialog_fits_960x640() {
    let mut p = proposal_a();
    p.sql = (0..200)
        .map(|i| format!("DELETE FROM sessions WHERE id = {i};"))
        .collect::<Vec<_>>()
        .join("\n");
    p.diff = Some(
        (0..100)
            .map(|i| format!("-row {i}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(p));
    assert!(host.tick(NOW_A));
    let mut probe = Probe::new();
    probe.host = host;
    let rt = runtime(probe, 960.0, 640.0);
    let dialog = element(&rt, "ai-write-proposal-dialog");
    assert!(dialog.rect[1] >= -0.5 && dialog.rect[1] + dialog.rect[3] <= 640.0 + 0.5);
    let sql = element(&rt, "ai-write-proposal-sql");
    assert!(
        sql.rect[3] <= 224.0 + 16.0 + 0.5,
        "SQL pre caps at max-h-56 (224pt) + padding, got {}",
        sql.rect[3]
    );
    let diff = element(&rt, "ai-write-proposal-diff");
    assert!(
        diff.rect[3] <= 160.0 + 16.0 + 0.5,
        "diff pre caps at max-h-40 (160pt) + padding, got {}",
        diff.rect[3]
    );
    assert_no_horizontal_overflow(&rt, 960.0, "long SQL 960x640");
}

#[test]
fn audit_impact_counts_without_tables_and_with_tables() {
    // No tables: bare counts, no separator, no warning when safe.
    let mut probe = Probe::live();
    probe.host = {
        let mut host = AiWriteProposalHostState::new();
        let mut p = proposal_a();
        p.impact.tables = Vec::new();
        assert!(host.enqueue(p));
        assert!(host.tick(NOW_A));
        host
    };
    let rt = runtime(probe, 1440.0, 816.0);
    assert!(any_text(&rt, "2 条语句"));
    assert!(!any_text_containing(&rt, "·"));
    assert!(!any_text(&rt, "破坏性语句：允许前请核对目标。"));
    assert!(!any_text(&rt, "数据变更没有 WHERE 子句，可能影响所有行。"));

    // Several tables join with ", " exactly as React's `.join(', ')`.
    let mut probe = Probe::live();
    probe.host = {
        let mut host = AiWriteProposalHostState::new();
        let mut p = proposal_a();
        p.impact.tables = vec!["orders".to_string(), "items".to_string()];
        assert!(host.enqueue(p));
        assert!(host.tick(NOW_A));
        host
    };
    let rt = runtime(probe, 1440.0, 816.0);
    assert!(any_text(&rt, "2 条语句 · 表：orders, items"));
}

#[test]
fn audit_remaining_line_counts_down_live_and_expires_in_view() {
    let mut rt = runtime(Probe::live(), 1440.0, 816.0);
    assert_eq!(
        element_text(&rt, "ai-write-proposal-remaining"),
        "120 秒后过期"
    );
    // Direct state mutation does not mark the runtime dirty (the view
    // re-runs "only if state changed"), so force a rebuild through the
    // runtime's own `settle_all` — an observation channel outside the
    // component under test.
    rt.state.host.tick(NOW_A + 119_000);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    assert_eq!(
        element_text(&rt, "ai-write-proposal-remaining"),
        "1 秒后过期"
    );
    rt.state.host.tick(NOW_A + 120_000);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    assert_eq!(element_text(&rt, "ai-write-proposal-remaining"), EXPIRED);
    assert_eq!(rt.state.host.deny_requests(), ["audit-p1"]);
}

#[test]
fn audit_expired_deny_click_still_records_but_allow_click_records_nothing() {
    // The expiry tick already auto-denied (responding is held), so release
    // it with a failed answer first: the proposal stays queued and expired,
    // and only then is the manual deny click meaningful.
    let mut probe = Probe::new();
    assert!(probe.host.enqueue(proposal_a()));
    assert!(probe.host.tick(NOW_A + 120_000));
    assert!(probe.host.expired());
    probe.host.deny_failed("transport");
    assert!(!probe.host.responding());
    let mut rt = runtime(probe, 1440.0, 816.0);
    assert_eq!(element_text(&rt, "ai-write-proposal-remaining"), EXPIRED);
    click(&mut rt, "ai-write-proposal-allow");
    assert!(rt.state.host.apply_requests().is_empty());
    click(&mut rt, "ai-write-proposal-deny");
    assert_eq!(rt.state.host.deny_requests(), ["audit-p1", "audit-p1"]);
}

#[test]
fn audit_receipt_zero_rows_is_not_unknown() {
    // React branches on `rowCount == null`; 0 is a real count.
    let mut probe = Probe::live();
    probe.host.allow();
    let mut receipt = AiWriteApplyReceipt::new("audit-p1");
    receipt.row_count = Some(0);
    probe.host.apply_succeeded(receipt);
    let rt = runtime(probe, 1440.0, 816.0);
    assert!(has_key(&rt, "ai-write-applied-receipt"));
    assert!(any_text(&rt, "写入已执行：影响 0 行"));
    assert!(!any_text(&rt, APPLIED_UNKNOWN));
}

#[test]
fn audit_dismissing_the_receipt_leaves_the_audit_toast() {
    let mut probe = Probe::live();
    probe.host.allow();
    probe
        .host
        .apply_succeeded(AiWriteApplyReceipt::new("audit-p1"));
    probe.host.set_audit_error("audit boom");
    let mut rt = runtime(probe, 1440.0, 816.0);
    click(&mut rt, "ai-write-applied-receipt-dismiss");
    assert!(!has_key(&rt, "ai-write-applied-receipt"));
    assert!(has_key(&rt, "ai-write-audit-error"));
}

#[test]
fn audit_non_enter_key_on_a_dismiss_button_dismisses_nothing() {
    // Covers the `else { false }` arm of both toasts' `on_key`: only
    // Enter dismisses.
    let mut probe = Probe::live();
    probe.host.allow();
    probe
        .host
        .apply_succeeded(AiWriteApplyReceipt::new("audit-p1"));
    probe.host.set_audit_error("audit boom");
    let mut rt = runtime(probe, 1440.0, 816.0);
    focus_by_tab(&mut rt, "ai-write-applied-receipt-dismiss");
    press(&mut rt, Key::Escape);
    assert!(has_key(&rt, "ai-write-applied-receipt"));
    focus_by_tab(&mut rt, "ai-write-audit-error-dismiss");
    press(&mut rt, Key::Escape);
    assert!(has_key(&rt, "ai-write-audit-error"));
}

#[test]
fn audit_shield_icons_are_the_registered_lucide_glyphs() {
    assert_eq!(ICON_SHIELD_ALERT.name, "shield-alert");
    assert_eq!(ICON_SHIELD_CHECK.name, "shield-check");
    let rt = runtime(Probe::live(), 1440.0, 816.0);
    assert!(has_key(&rt, "ai-write-proposal-shield"));
}

#[test]
fn audit_all_react_i18n_keys_exist_including_the_error_keys() {
    // The 22 keys the view asks for, plus the two the state machine's
    // callers must pass (`respondFailed` on transport failure,
    // `auditFailed` for a non-string audit payload) — all 24 from
    // zh-CN/ai.ts.
    for key in [
        "ai.write.title",
        "ai.write.description",
        "ai.write.remaining",
        "ai.write.expired",
        "ai.write.connection",
        "ai.write.database",
        "ai.write.noDatabase",
        "ai.write.hash",
        "ai.write.productionWarning",
        "ai.write.readOnlyWarning",
        "ai.write.exactSql",
        "ai.write.diff",
        "ai.write.impact",
        "ai.write.statements",
        "ai.write.tables",
        "ai.write.destructiveWarning",
        "ai.write.noWhereWarning",
        "ai.write.allowOnce",
        "ai.write.deny",
        "ai.write.respondFailed",
        "ai.write.auditFailed",
        "ai.write.applied",
        "ai.write.appliedUnknown",
        "ai.write.dismiss",
    ] {
        assert!(
            crate::i18n::has(key),
            "{key} is missing from the zh-CN packs"
        );
        assert_ne!(t(key), key, "{key} falls back to itself");
    }
    assert_eq!(t("ai.write.remaining"), REMAINING_TMPL);
    assert_eq!(t("ai.write.applied"), APPLIED_TMPL);
    assert_eq!(t("ai.write.appliedUnknown"), APPLIED_UNKNOWN);
    assert_eq!(t("ai.write.statements"), STATEMENTS_TMPL);
    assert_eq!(t("ai.write.tables"), TABLES_TMPL);
    assert_eq!(t("ai.write.respondFailed"), RESPOND_FAILED);
    assert_eq!(t("ai.write.auditFailed"), AUDIT_FAILED);
}

#[test]
fn audit_full_app_view_mounts_the_host_and_the_audit_toast_together() {
    let mut app = AppState::default();
    assert!(app.ai_write_proposal.enqueue(proposal_a()));
    assert!(app.ai_write_proposal.tick(NOW_A));
    app.ai_write_proposal.set_audit_error("audit boom");
    let mut rt = Runtime::new(app, view, (1440.0, 816.0), env(true));
    rt.frame(0.0);
    assert!(has_key(&rt, "ai-write-proposal-dialog"));
    assert!(has_key(&rt, "ai-write-audit-error"));
    assert_eq!(
        element_text(&rt, "ai-write-proposal-connection"),
        "audit-conn"
    );
}
