//! T012: `AiWriteProposalHost` — the T24 confirmed-write dialog.
//!
//! Pure-logic tests call the state machine directly; runtime tests render
//! the host in a probe `Runtime` and read back the laid-out element tree —
//! never the component's own report of itself. Geometry is asserted on
//! `rt.elements()` rects, which come from the layout engine, not from the
//! code under test; the zh-CN strings are pinned against the literals in
//! `packages/frontend/src/locales/zh-CN/ai.ts`.

use chrono::{DateTime, SecondsFormat};
use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::ai::{
    AiWriteApplyReceipt, AiWriteImpact, AiWriteProposal, AiWriteProposalHost,
    AiWriteProposalHostState, AiWriteStatement,
};
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::{view, AppState};

// Literal zh-CN text from packages/frontend/src/locales/zh-CN/ai.ts —
// pinned here, not through `t()`.
const TITLE: &str = "批准数据库写入";
const DESCRIPTION: &str = "请核对精确 SQL 与影响；目前尚未执行任何语句。";
const EXPIRED: &str = "已过期";
const PRODUCTION_WARNING: &str = "这是生产连接。即使确认也永不允许写入。";
const READONLY_WARNING: &str = "该连接为只读，只能拒绝提案，不能执行。";
const EXACT_SQL: &str = "精确 SQL（仅在你允许一次后执行）";
const DIFF: &str = "变更对比";
const IMPACT: &str = "影响";
const DESTRUCTIVE_WARNING: &str = "破坏性语句：允许前请核对目标。";
const NO_WHERE_WARNING: &str = "数据变更没有 WHERE 子句，可能影响所有行。";
const ALLOW_ONCE: &str = "允许一次";
const DENY: &str = "拒绝";
const APPLIED_UNKNOWN: &str = "写入已执行：影响行数未知";
const DISMISS: &str = "关闭";

/// Fixed clock for the expiry tests: 2027-01-15T08:00:00Z in millis.
const NOW_MS: u64 = 1_800_000_000_000;
const SQL: &str = "UPDATE users SET role = 'admin' WHERE id = 42";
const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn expires_at(ms: u64) -> String {
    DateTime::from_timestamp_millis(ms as i64)
        .unwrap()
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn proposal() -> AiWriteProposal {
    let mut impact = AiWriteImpact::new("1 statement(s): UPDATE");
    impact.statement_count = 1;
    impact.tables = vec!["users".to_string()];
    impact.has_where = true;
    impact.writes_data = true;
    let mut p = AiWriteProposal::new("proposal-1", "conn-1", SQL, expires_at(NOW_MS + 300_000));
    p.database = "app".to_string();
    p.sql_hash = HASH.to_string();
    p.impact = impact;
    p.statements = vec![AiWriteStatement::new(0, "UPDATE", "b".repeat(64))];
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

    /// A live proposal, ticked at the fixed clock: 300s remaining.
    fn live() -> Self {
        let mut probe = Self::new();
        assert!(probe.host.enqueue(proposal()));
        assert!(probe.host.tick(NOW_MS));
        probe
    }
}

fn host_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
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

/// The same host with no accessor: the children render unwired, covering
/// the `None` arms of every button.
fn bare_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    AiWriteProposalHost::<Probe>::new(&state.host).build(state.dark, &mut layer);
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
    let mut rt = Runtime::new(probe, host_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

fn runtime_bare(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, bare_view, (w, h), env(dark));
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

fn element_text<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn any_text<S>(rt: &Runtime<S>, text: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(text))
}

/// A multi-line text node (the diff `<pre>`) holds its lines in one
/// element, so the line is found by substring, not by exact equality.
fn any_text_containing<S>(rt: &Runtime<S>, needle: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref().is_some_and(|t| t.contains(needle)))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

/// Every laid-out element stays inside the window horizontally.
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

// ---- state machine ----------------------------------------------------------

#[test]
fn enqueue_ignores_empty_id_or_sql_and_dedups_by_id() {
    let mut host = AiWriteProposalHostState::new();
    let mut no_id = proposal();
    no_id.proposal_id = String::new();
    assert!(!host.enqueue(no_id));
    let mut no_sql = proposal();
    no_sql.sql = String::new();
    assert!(!host.enqueue(no_sql));
    assert_eq!(host.queue_len(), 0);
    assert!(host.enqueue(proposal()));
    assert!(!host.enqueue(proposal()));
    assert_eq!(host.queue_len(), 1);
    let mut second = proposal();
    second.proposal_id = "proposal-2".to_string();
    assert!(host.enqueue(second));
    assert_eq!(host.queue_len(), 2);
    assert_eq!(host.current().unwrap().proposal_id, "proposal-1");
}

#[test]
fn enqueue_bumps_open_epoch_only_when_becoming_current() {
    let mut host = AiWriteProposalHostState::new();
    assert_eq!(host.open_epoch(), 0);
    assert!(host.enqueue(proposal()));
    assert_eq!(host.open_epoch(), 1);
    let mut second = proposal();
    second.proposal_id = "proposal-2".to_string();
    assert!(host.enqueue(second));
    assert_eq!(host.open_epoch(), 1);
}

#[test]
fn tick_computes_remaining_seconds_with_ceiling() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal()));
    assert!(host.tick(NOW_MS));
    assert_eq!(host.remaining_ms(), 300_000);
    assert_eq!(host.remaining_seconds(), 300);
    assert!(!host.expired());
    // 500ms left still reads as 1s (React's Math.ceil).
    assert!(host.tick(NOW_MS + 299_500));
    assert_eq!(host.remaining_ms(), 500);
    assert_eq!(host.remaining_seconds(), 1);
}

#[test]
fn tick_with_no_current_resets_remaining() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal()));
    assert!(host.tick(NOW_MS));
    assert_eq!(host.remaining_ms(), 300_000);
    host.deny();
    host.deny_succeeded("proposal-1");
    assert!(host.tick(NOW_MS));
    assert_eq!(host.remaining_ms(), 0);
    assert!(!host.tick(NOW_MS));
}

#[test]
fn tick_at_expiry_auto_denies_exactly_once() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal()));
    assert!(host.tick(NOW_MS));
    assert!(host.deny_requests().is_empty());
    assert!(host.tick(NOW_MS + 300_000));
    assert!(host.expired());
    assert_eq!(host.deny_requests(), ["proposal-1"]);
    assert!(host.responding());
    // The auto-deny is recorded once, however many ticks follow.
    assert!(!host.tick(NOW_MS + 301_000));
    assert_eq!(host.deny_requests(), ["proposal-1"]);
}

#[test]
fn tick_with_unparseable_expiry_expires_immediately() {
    // React's Date.parse returns NaN, so remainingMs is 0 at once.
    let mut host = AiWriteProposalHostState::new();
    let mut p = proposal();
    p.expires_at = "not-a-date".to_string();
    assert!(host.enqueue(p));
    assert!(host.tick(NOW_MS));
    assert!(host.expired());
    assert_eq!(host.deny_requests(), ["proposal-1"]);
}

#[test]
fn allow_records_the_exact_apply_request_and_holds_responding() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal()));
    assert!(host.tick(NOW_MS));
    host.allow();
    assert!(host.responding());
    assert_eq!(host.apply_requests().len(), 1);
    let request = &host.apply_requests()[0];
    assert_eq!(request.proposal_id, "proposal-1");
    assert_eq!(request.connection_id, "conn-1");
    assert_eq!(request.database, "app");
    assert_eq!(request.sql, SQL);
    // A second allow while responding records nothing (React's button is
    // disabled then).
    host.allow();
    assert_eq!(host.apply_requests().len(), 1);
}

#[test]
fn allow_is_a_noop_when_expired_or_not_allowed() {
    let mut host = AiWriteProposalHostState::new();
    let mut readonly = proposal();
    readonly.apply_allowed = false;
    assert!(host.enqueue(readonly));
    assert!(host.tick(NOW_MS));
    host.allow();
    assert!(host.apply_requests().is_empty());
    assert!(!host.responding());

    let mut expired = AiWriteProposalHostState::new();
    assert!(expired.enqueue(proposal()));
    assert!(expired.tick(NOW_MS + 300_000));
    assert!(expired.responding());
    // The expiry auto-deny already holds `responding`; a fresh host that
    // expired without a deny in flight also refuses the allow.
    let mut plain = AiWriteProposalHostState::new();
    assert!(plain.enqueue(proposal()));
    plain.tick(NOW_MS + 300_000);
    plain.deny_succeeded("proposal-1");
    assert!(plain.enqueue(proposal()));
    plain.tick(NOW_MS + 300_000);
    assert!(plain.apply_requests().is_empty());
}

#[test]
fn apply_succeeded_shows_the_receipt_and_drops_the_proposal() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal()));
    assert!(host.tick(NOW_MS));
    host.allow();
    let mut receipt = AiWriteApplyReceipt::new("proposal-1");
    receipt.row_count = Some(3);
    host.apply_succeeded(receipt);
    assert!(!host.responding());
    assert!(host.current().is_none());
    assert_eq!(host.receipt().unwrap().row_count, Some(3));
    assert!(host.is_visible());
}

#[test]
fn apply_failed_keeps_the_dialog_and_surfaces_the_error() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal()));
    assert!(host.tick(NOW_MS));
    host.allow();
    host.apply_failed("boom");
    assert!(!host.responding());
    assert_eq!(host.error(), Some("boom"));
    assert!(host.current().is_some());
}

#[test]
fn deny_twice_records_only_one_request() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal()));
    assert!(host.tick(NOW_MS));
    host.deny();
    // The second deny lands while `responding` is held (React's button is
    // disabled then) and records nothing.
    host.deny();
    assert_eq!(host.deny_requests(), ["proposal-1"]);
}

#[test]
fn deny_records_the_request_and_drops_on_success() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal()));
    assert!(host.tick(NOW_MS));
    host.deny();
    assert!(host.responding());
    assert_eq!(host.deny_requests(), ["proposal-1"]);
    host.deny_succeeded("proposal-1");
    assert!(!host.responding());
    assert!(host.current().is_none());
    assert!(!host.is_visible());
}

#[test]
fn deny_failed_keeps_the_dialog_and_surfaces_the_error() {
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(proposal()));
    assert!(host.tick(NOW_MS));
    host.deny();
    host.deny_failed("boom");
    assert!(!host.responding());
    assert_eq!(host.error(), Some("boom"));
    assert!(host.current().is_some());
}

#[test]
fn deny_is_a_noop_with_no_current_proposal() {
    let mut host = AiWriteProposalHostState::new();
    host.deny();
    assert!(host.deny_requests().is_empty());
    assert!(!host.responding());
    host.allow();
    assert!(host.apply_requests().is_empty());
}

#[test]
fn audit_error_and_receipt_dismissals() {
    let mut host = AiWriteProposalHostState::new();
    assert!(!host.is_visible());
    host.set_audit_error("audit boom");
    assert_eq!(host.audit_error(), Some("audit boom"));
    assert!(host.is_visible());
    host.dismiss_audit_error();
    assert!(host.audit_error().is_none());
    assert!(!host.is_visible());

    assert!(host.enqueue(proposal()));
    host.allow();
    let mut receipt = AiWriteApplyReceipt::new("proposal-1");
    receipt.row_count = None;
    host.apply_succeeded(receipt);
    assert!(host.receipt().is_some());
    host.dismiss_receipt();
    assert!(host.receipt().is_none());
    assert!(!host.is_visible());
}

#[test]
fn app_poll_sync_ticks_the_host_with_the_wall_clock() {
    // A proposal that expired long ago auto-denies on the first poll tick,
    // whatever the wall clock reads.
    let mut app = AppState::default();
    let mut p = proposal();
    p.expires_at = "2020-01-01T00:00:00Z".to_string();
    assert!(app.ai_write_proposal.enqueue(p));
    assert!(app.sync_ai_write_proposals());
    assert_eq!(app.ai_write_proposal.deny_requests(), ["proposal-1"]);
    // And a host with nothing queued stays quiet.
    let mut empty = AppState::default();
    assert!(!empty.sync_ai_write_proposals());
}

// ---- runtime render ---------------------------------------------------------

#[test]
fn dialog_shows_exact_sql_target_hash_diff_and_impact() {
    let mut p = proposal();
    p.diff = Some("-UPDATE users SET role = 1\n+UPDATE users SET role = 2".to_string());
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(p));
    assert!(host.tick(NOW_MS));
    let mut probe = Probe::new();
    probe.host = host;
    let rt = runtime(probe, 1440.0, 900.0);
    assert!(has_key(&rt, "ai-write-proposal-backdrop"));
    assert!(has_key(&rt, "ai-write-proposal-dialog"));
    assert_eq!(element_text(&rt, "ai-write-proposal-title"), TITLE);
    assert!(any_text(&rt, DESCRIPTION));
    assert_eq!(element_text(&rt, "ai-write-proposal-connection"), "conn-1");
    assert_eq!(element_text(&rt, "ai-write-proposal-database"), "app");
    assert!(any_text(&rt, SQL));
    assert!(any_text_containing(&rt, "+UPDATE users SET role = 2"));
    assert!(any_text(&rt, HASH));
    assert_eq!(
        element_text(&rt, "ai-write-proposal-summary"),
        "1 statement(s): UPDATE"
    );
    assert!(any_text(&rt, "1 条语句 · 表：users"));
    assert!(any_text(&rt, EXACT_SQL));
    assert!(any_text(&rt, DIFF));
    assert!(any_text(&rt, IMPACT));
    assert_eq!(
        element_text(&rt, "ai-write-proposal-remaining"),
        "300 秒后过期"
    );
    assert!(any_text(&rt, ALLOW_ONCE));
    assert!(any_text(&rt, DENY));
    assert!(!has_key(&rt, "ai-write-proposal-production-warning"));
    assert!(!has_key(&rt, "ai-write-proposal-readonly-warning"));
    assert!(has_key(&rt, "ai-write-proposal-diff"));
    assert_no_horizontal_overflow(&rt, 1440.0, "dialog 1440x900");
}

#[test]
fn production_protected_shows_the_warning_and_the_danger_shield() {
    let mut p = proposal();
    p.production_protected = true;
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(p));
    assert!(host.tick(NOW_MS));
    let mut probe = Probe::new();
    probe.host = host;
    let rt = runtime(probe, 1440.0, 900.0);
    assert!(has_key(&rt, "ai-write-proposal-production-warning"));
    assert!(any_text(&rt, PRODUCTION_WARNING));
    assert!(!has_key(&rt, "ai-write-proposal-readonly-warning"));
    assert_eq!(
        element(&rt, "ai-write-proposal-shield").text_color,
        Some(kit(true).c.danger.icon),
        "production shield is danger-tinted"
    );
}

#[test]
fn readonly_proposal_shows_the_readonly_warning_and_the_check_shield() {
    let mut p = proposal();
    p.apply_allowed = false;
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(p));
    assert!(host.tick(NOW_MS));
    let mut probe = Probe::new();
    probe.host = host;
    let rt = runtime(probe, 1440.0, 900.0);
    assert!(has_key(&rt, "ai-write-proposal-readonly-warning"));
    assert!(any_text(&rt, READONLY_WARNING));
    assert!(!has_key(&rt, "ai-write-proposal-production-warning"));
    assert_eq!(
        element(&rt, "ai-write-proposal-shield").text_color,
        Some(kit(true).c.text_accent),
        "unprotected shield is accent-tinted"
    );
}

#[test]
fn empty_database_shows_the_default_label() {
    let mut p = proposal();
    p.database = String::new();
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(p));
    assert!(host.tick(NOW_MS));
    let mut probe = Probe::new();
    probe.host = host;
    let rt = runtime(probe, 1440.0, 900.0);
    assert_eq!(element_text(&rt, "ai-write-proposal-database"), "（默认）");
}

#[test]
fn destructive_and_no_where_warnings() {
    let mut destructive = proposal();
    destructive.impact.destructive = true;
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(destructive));
    assert!(host.tick(NOW_MS));
    let mut probe = Probe::new();
    probe.host = host;
    let rt = runtime(probe, 1440.0, 900.0);
    assert!(any_text(&rt, DESTRUCTIVE_WARNING));

    let mut no_where = proposal();
    no_where.impact.has_where = false;
    let mut host = AiWriteProposalHostState::new();
    assert!(host.enqueue(no_where));
    assert!(host.tick(NOW_MS));
    let mut probe = Probe::new();
    probe.host = host;
    let rt = runtime(probe, 1440.0, 900.0);
    assert!(any_text(&rt, NO_WHERE_WARNING));
    assert!(!any_text(&rt, DESTRUCTIVE_WARNING));
}

#[test]
fn allow_click_records_the_exact_request() {
    let mut rt = runtime(Probe::live(), 1440.0, 900.0);
    click(&mut rt, "ai-write-proposal-allow");
    assert_eq!(rt.state.host.apply_requests().len(), 1);
    let request = &rt.state.host.apply_requests()[0];
    assert_eq!(request.proposal_id, "proposal-1");
    assert_eq!(request.connection_id, "conn-1");
    assert_eq!(request.database, "app");
    assert_eq!(request.sql, SQL);
    assert!(rt.state.host.responding());
}

#[test]
fn deny_click_records_the_request_and_the_bare_host_records_nothing() {
    let mut rt = runtime(Probe::live(), 1440.0, 900.0);
    click(&mut rt, "ai-write-proposal-deny");
    assert_eq!(rt.state.host.deny_requests(), ["proposal-1"]);

    // Without the accessor the buttons render unwired: the press lands
    // nowhere and records nothing.
    let mut bare = runtime_bare(Probe::live(), 1440.0, 900.0);
    click(&mut bare, "ai-write-proposal-allow");
    assert!(bare.state.host.apply_requests().is_empty());
    click(&mut bare, "ai-write-proposal-deny");
    assert!(bare.state.host.deny_requests().is_empty());
}

#[test]
fn expired_allow_click_records_nothing() {
    let mut probe = Probe::new();
    assert!(probe.host.enqueue(proposal()));
    assert!(probe.host.tick(NOW_MS + 300_000));
    assert!(probe.host.expired());
    let mut rt = runtime(probe, 1440.0, 900.0);
    assert_eq!(element_text(&rt, "ai-write-proposal-remaining"), EXPIRED);
    click(&mut rt, "ai-write-proposal-allow");
    assert!(rt.state.host.apply_requests().is_empty());
}

#[test]
fn enter_on_allow_records_the_apply_request() {
    let mut rt = runtime(Probe::live(), 1440.0, 900.0);
    focus_by_tab(&mut rt, "ai-write-proposal-allow");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.host.apply_requests().len(), 1);
    assert_eq!(rt.state.host.apply_requests()[0].proposal_id, "proposal-1");
}

#[test]
fn enter_on_deny_records_the_deny_request() {
    let mut rt = runtime(Probe::live(), 1440.0, 900.0);
    focus_by_tab(&mut rt, "ai-write-proposal-deny");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.host.deny_requests(), ["proposal-1"]);
}

#[test]
fn enter_on_receipt_dismiss_clears_the_toast() {
    let mut probe = Probe::live();
    probe.host.allow();
    probe.host.apply_succeeded(AiWriteApplyReceipt::new("proposal-1"));
    let mut rt = runtime(probe, 1440.0, 900.0);
    focus_by_tab(&mut rt, "ai-write-applied-receipt-dismiss");
    press(&mut rt, Key::Enter);
    assert!(!has_key(&rt, "ai-write-applied-receipt"));
}

#[test]
fn enter_on_audit_dismiss_clears_the_toast() {
    let mut probe = Probe::live();
    probe.host.set_audit_error("audit boom");
    let mut rt = runtime(probe, 1440.0, 900.0);
    focus_by_tab(&mut rt, "ai-write-audit-error-dismiss");
    press(&mut rt, Key::Enter);
    assert!(!has_key(&rt, "ai-write-audit-error"));
    assert!(has_key(&rt, "ai-write-proposal-dialog"));
}

#[test]
fn receipt_toast_shows_the_row_count_and_dismisses() {
    let mut probe = Probe::live();
    probe.host.allow();
    let mut receipt = AiWriteApplyReceipt::new("proposal-1");
    receipt.row_count = Some(3);
    probe.host.apply_succeeded(receipt);
    let mut rt = runtime(probe, 1440.0, 900.0);
    assert!(!has_key(&rt, "ai-write-proposal-dialog"));
    assert!(has_key(&rt, "ai-write-applied-receipt"));
    assert!(any_text(&rt, "写入已执行：影响 3 行"));
    assert!(any_text(&rt, DISMISS));
    click(&mut rt, "ai-write-applied-receipt-dismiss");
    assert!(!has_key(&rt, "ai-write-applied-receipt"));
}

#[test]
fn receipt_toast_with_unknown_row_count() {
    let mut probe = Probe::live();
    probe.host.allow();
    probe
        .host
        .apply_succeeded(AiWriteApplyReceipt::new("proposal-1"));
    let rt = runtime(probe, 1440.0, 900.0);
    assert!(has_key(&rt, "ai-write-applied-receipt"));
    assert!(any_text(&rt, APPLIED_UNKNOWN));
}

#[test]
fn audit_error_toast_shows_beside_the_dialog_and_dismisses() {
    let mut probe = Probe::live();
    probe.host.set_audit_error("audit boom");
    let mut rt = runtime(probe, 1440.0, 900.0);
    assert!(has_key(&rt, "ai-write-proposal-dialog"));
    assert!(has_key(&rt, "ai-write-audit-error"));
    assert!(any_text(&rt, "audit boom"));
    click(&mut rt, "ai-write-audit-error-dismiss");
    assert!(!has_key(&rt, "ai-write-audit-error"));
    assert!(has_key(&rt, "ai-write-proposal-dialog"));
}

#[test]
fn response_error_renders_on_the_dialog() {
    let mut probe = Probe::live();
    probe.host.allow();
    probe.host.apply_failed("无法提交写入决定");
    let rt = runtime(probe, 1440.0, 900.0);
    assert!(has_key(&rt, "ai-write-proposal-error"));
    assert_eq!(
        element_text(&rt, "ai-write-proposal-error"),
        "无法提交写入决定"
    );
    assert!(has_key(&rt, "ai-write-proposal-dialog"));
}

#[test]
fn hidden_host_renders_nothing() {
    let rt = runtime(Probe::new(), 1440.0, 900.0);
    assert!(!has_key(&rt, "ai-write-proposal-backdrop"));
    assert!(!has_key(&rt, "ai-write-proposal-dialog"));
    assert!(!has_key(&rt, "ai-write-applied-receipt"));
    assert!(!has_key(&rt, "ai-write-audit-error"));
}

#[test]
fn dialog_is_centred_and_fits_at_both_sizes_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 900.0)] {
        for dark in [true, false] {
            let mut probe = Probe::live();
            probe.dark = dark;
            let rt = runtime(probe, w, h);
            let backdrop = element(&rt, "ai-write-proposal-backdrop");
            assert_eq!(backdrop.rect, [0.0, 0.0, w, h], "backdrop fills the window");
            let dialog = element(&rt, "ai-write-proposal-dialog");
            let (x, y, dw, dh) = (
                dialog.rect[0],
                dialog.rect[1],
                dialog.rect[2],
                dialog.rect[3],
            );
            assert!(dw <= 672.0 + 0.5, "panel caps at max-w-2xl, got {dw}");
            assert!(
                (x + dw / 2.0 - w / 2.0).abs() < 1.5,
                "panel centred horizontally"
            );
            assert!(
                (y + dh / 2.0 - h / 2.0).abs() < 1.5,
                "panel centred vertically"
            );
            assert!(x >= -0.5 && x + dw <= w + 0.5, "panel inside the window");
            assert_no_horizontal_overflow(&rt, w, "dialog");
        }
    }
}

#[test]
fn full_app_view_mounts_the_host_from_app_state() {
    // The modal_layer mount: the dialog's keys exist in the real `--tree`
    // snapshot once a proposal is queued.
    let mut app = AppState::default();
    assert!(app.ai_write_proposal.enqueue(proposal()));
    assert!(app.ai_write_proposal.tick(NOW_MS));
    let mut rt = Runtime::new(app, view, (1440.0, 900.0), env(true));
    rt.frame(0.0);
    assert!(has_key(&rt, "ai-write-proposal-dialog"));
    assert_eq!(element_text(&rt, "ai-write-proposal-connection"), "conn-1");
}

/// The `t()` keys the host asks for resolve in the zh-CN packs (the
/// `i18n::every_key_the_ui_asks_for_exists` guard also covers this, but the
/// list pins the exact set this port needs).
#[test]
fn used_i18n_keys_exist() {
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
}
