//! Independent audit for T146 (the MonitorScheduleAuthorization Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/dashboard/MonitorScheduleAuthorization.tsx`
//! and its React test
//! `__tests__/MonitorScheduleAuthorization.test.tsx`: the id-keyed fetch and
//! its cancel token, the `authorized` derivation (unrevoked + future
//! expiry), the authorize → `authorized until` → revoke → `not authorized`
//! journey, `dirty` gating only the authorize button, the backend scope
//! rejection surfacing as a `role="alert"` line, and the busy flag freezing
//! both actions. Element evidence comes from `Runtime::elements()` (runtime
//! layout), never from a value the component reports about itself; presses
//! go through real pointer events.

use rune::{Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::dashboard::monitor_schedule_authorization::{
    MonitorGrantVm, MonitorScheduleAuthorization, MonitorScheduleAuthorizationState,
};
use crate::views::locale_string;

/// `Date.now()` for every build: 2023-11-14T22:13:20Z on the Unix clock.
const NOW_MS: i64 = 1_700_000_000_000;
/// A far-future expiry (the React test's own).
const FUTURE: &str = "2099-01-01T00:00:00Z";

fn grant(expires_at: &str, revoked: bool) -> MonitorGrantVm {
    MonitorGrantVm { expires_at: expires_at.to_owned(), revoked }
}

#[derive(Clone)]
struct AuditState {
    card: MonitorScheduleAuthorizationState,
    dark: bool,
    dirty: bool,
    authorizes: usize,
    revokes: usize,
}

impl AuditState {
    /// The mount: the effect ran for `('d', 'w')`, nothing delivered yet.
    fn base() -> Self {
        let mut card = MonitorScheduleAuthorizationState::default();
        assert!(card.needs_resync("d", "w"), "the mount always fetches");
        card.begin_resync("d", "w");
        Self { card, dark: true, dirty: false, authorizes: 0, revokes: 0 }
    }

    /// The fetch answered with a grant.
    fn with_grant(expires_at: &str, revoked: bool) -> Self {
        let mut scene = Self::base();
        let token = scene.card.request_epoch;
        assert!(scene.card.deliver_grant(grant(expires_at, revoked), token));
        scene
    }
}

fn view(s: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    MonitorScheduleAuthorization::<AuditState>::new(&s.card, NOW_MS)
        .dirty(s.dirty)
        .on_authorize(|s: &mut AuditState| {
            s.authorizes += 1;
            s.card.begin_change();
        })
        .on_revoke(|s: &mut AuditState| {
            s.revokes += 1;
            s.card.begin_change();
        })
        .build(s.dark)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<AuditState>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn enabled(rt: &Runtime<AuditState>, key: &str) -> bool {
    maybe(rt, key).map(|e| e.focusable).unwrap_or(false)
}

/// A kit Button's label is its keyed `label` child (the button container's
/// `ElementInfo::text` is None), resolved by path prefix.
fn button_label(rt: &Runtime<AuditState>, key: &str) -> String {
    let Some(button) = maybe(rt, key) else {
        return String::new();
    };
    let prefix = button.path;
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some("label") && e.path.starts_with(&prefix))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    // Move onto the control first and let the clock run: a control mounted by
    // this runtime's first frame is pressable only after its enter motion.
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
}

/// Applies a state mutation the way the host would (a promise landing) and
/// lets the view re-render.
fn land<F>(rt: &mut Runtime<AuditState>, f: F)
where
    F: FnOnce(&mut AuditState),
{
    rt.update(|s, _| f(s));
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
}

// ---- the state machine (React's useState + the id effect) ------------------

#[test]
fn resync_follows_reacts_dep_array_and_clears_the_card() {
    let mut state = MonitorScheduleAuthorizationState::default();
    state.begin_resync("d", "w");
    assert!(!state.needs_resync("d", "w"), "same pair: no refetch");
    assert!(state.needs_resync("d", "x"), "widget change refetches");
    assert!(state.needs_resync("other", "w"), "dashboard change refetches");

    // The effect's synchronous half clears grant + error and bumps the
    // token, but leaves `busy` alone (React's effect never touches it).
    state.grant = Some(grant(FUTURE, false));
    state.error = Some("boom".to_owned());
    state.busy = true;
    let first_epoch = state.request_epoch;
    state.begin_resync("d", "x");
    assert_eq!(state.request_epoch, first_epoch + 1, "a new token per resync");
    assert!(state.grant.is_none(), "setGrant(null)");
    assert!(state.error.is_none(), "setError(null)");
    assert!(state.busy, "the effect does not clear busy");
}

#[test]
fn a_delivery_lands_only_for_the_current_token() {
    // React's `active` flag: the cleanup on an id change cancels the flight.
    let mut state = MonitorScheduleAuthorizationState::default();
    state.begin_resync("d", "w");
    let stale = state.request_epoch;
    state.begin_resync("d", "w2"); // the widget id changed mid-flight
    assert!(!state.deliver_grant(grant(FUTURE, false), stale));
    assert!(state.grant.is_none(), "the superseded grant lands nothing");
    assert!(!state.deliver_resync_error("late".to_owned(), stale));
    assert!(state.error.is_none(), "the superseded error lands nothing");

    let current = state.request_epoch;
    assert!(state.deliver_grant(grant(FUTURE, false), current));
    assert!(state.grant.is_some());
}

#[test]
fn authorized_needs_an_unrevoked_future_expiry() {
    let mut state = MonitorScheduleAuthorizationState::default();
    state.begin_resync("d", "w");
    assert!(!state.authorized(NOW_MS), "no grant (React's null)");

    state.deliver_grant(grant(FUTURE, true), state.request_epoch);
    assert!(!state.authorized(NOW_MS), "revoked");

    state.deliver_grant(grant("2000-01-01T00:00:00Z", false), state.request_epoch);
    assert!(!state.authorized(NOW_MS), "expired");

    state.deliver_grant(grant("not-a-date", false), state.request_epoch);
    assert!(!state.authorized(NOW_MS), "unparseable is Date.parse NaN");

    state.deliver_grant(grant("2099-01-01 00:00:00", false), state.request_epoch);
    assert!(!state.authorized(NOW_MS), "zone-less has no instant (documented stance)");

    // `2023-11-14T22:13:20Z` is exactly NOW_MS: React's strict `>` is false.
    state.deliver_grant(grant("2023-11-14T22:13:20Z", false), state.request_epoch);
    assert!(!state.authorized(NOW_MS), "an expiry at now is not in the future");

    state.deliver_grant(grant(FUTURE, false), state.request_epoch);
    assert!(state.authorized(NOW_MS));
}

#[test]
fn the_change_lifecycle_authorize() {
    let mut state = MonitorScheduleAuthorizationState::default();
    state.begin_resync("d", "w");
    state.deliver_grant(grant("2000-01-01T00:00:00Z", false), state.request_epoch);

    // The synchronous head: busy on, previous error cleared.
    state.error = Some("earlier failure".to_owned());
    state.begin_change();
    assert!(state.busy);
    assert!(state.error.is_none());

    // authorize's .then: the fresh grant lands, the finally clears busy.
    state.change_resolved_with_grant(grant(FUTURE, false));
    assert!(!state.busy);
    assert_eq!(state.grant, Some(grant(FUTURE, false)));
}

#[test]
fn the_change_lifecycle_revoke_clears_the_grant() {
    let mut state = AuditState::with_grant(FUTURE, false).card;
    state.begin_change();
    assert!(state.busy);
    state.change_resolved_revoked();
    assert!(state.grant.is_none(), "revoke's setGrant(null)");
    assert!(!state.busy);
}

#[test]
fn a_failed_change_sets_the_error_and_clears_busy() {
    // The React test's own rejection text (backend scope rejection).
    let mut state = MonitorScheduleAuthorizationState::default();
    state.begin_resync("d", "w");
    state.begin_change();
    state.change_failed("explicit database required".to_owned());
    assert_eq!(state.error.as_deref(), Some("explicit database required"));
    assert!(!state.busy, "the finally runs on failure too");
}

#[test]
fn the_status_line_formats_the_authorized_until_label() {
    let mut state = MonitorScheduleAuthorizationState::default();
    state.begin_resync("d", "w");
    assert_eq!(state.status_line(NOW_MS), "定时刷新尚未授权。");

    state.deliver_grant(grant(FUTURE, false), state.request_epoch);
    let label = state.status_line(NOW_MS);
    assert!(label.starts_with("已授权至"), "t('dashboard.scheduleAuthorizedUntil')");
    // The {time} placeholder took the toLocaleString rendering; any real
    // local zone keeps the 2099 (or 2098) year digits.
    assert!(label.contains("2099") || label.contains("2098"), "{label}");
    assert_eq!(
        label,
        t("dashboard.scheduleAuthorizedUntil").replace("{time}", &locale_string(FUTURE)),
        "the placeholder is replaced, nothing else"
    );

    // A revoked grant shows unauthorized even with a future expiry.
    state.deliver_grant(grant(FUTURE, true), state.request_epoch);
    assert_eq!(state.status_line(NOW_MS), "定时刷新尚未授权。");
}

// ---- runtime evidence (element tree, presses) -------------------------------

#[test]
fn the_card_renders_its_lines_and_the_unauthorized_status() {
    let rt = rt(AuditState::base());
    assert!(has(&rt, "monitor-schedule-authorization"), "the section root");
    assert_eq!(
        text_of(&rt, "monitor-schedule-title"),
        t("dashboard.scheduleAuthorization"),
        "text-xs font-medium title"
    );
    assert_eq!(
        text_of(&rt, "monitor-schedule-hint"),
        t("dashboard.scheduleAuthorizationHint")
    );
    assert_eq!(text_of(&rt, "monitor-schedule-status"), "定时刷新尚未授权。");
    assert!(has(&rt, "monitor-schedule-actions"), "the flex gap-2 row");
    assert_eq!(
        button_label(&rt, "monitor-schedule-authorize"),
        t("dashboard.scheduleAuthorizeHour")
    );
    assert_eq!(button_label(&rt, "monitor-schedule-revoke"), t("dashboard.scheduleRevoke"));

    assert!(!has(&rt, "monitor-schedule-save-first"), "no dirty line while clean");
    assert!(!has(&rt, "monitor-schedule-error"), "no error line");
    assert!(enabled(&rt, "monitor-schedule-authorize"), "clean + idle: pressable");
    assert!(!enabled(&rt, "monitor-schedule-revoke"), "no grant: revoke disabled");
}

#[test]
fn dirty_adds_the_save_first_line_and_gates_only_authorize() {
    // Without a grant, dirty or not, revoke stays disabled. (The bare
    // runtime is built before the `rt` binding shadows the module-level
    // constructor, the sibling-audit convention.)
    let mut bare = AuditState::base();
    bare.dirty = true;
    let bare_rt = rt(bare);

    // React test 2: dirty disables the authorize button outright.
    let mut scene = AuditState::with_grant(FUTURE, false);
    scene.dirty = true;
    let rt = rt(scene);
    assert_eq!(
        text_of(&rt, "monitor-schedule-save-first"),
        t("dashboard.scheduleSaveFirst")
    );
    assert!(!enabled(&rt, "monitor-schedule-authorize"), "busy || dirty");
    // React's revoke guard reads only busy + the grant: dirty does NOT gate
    // it — an unsaved card may still revoke what the saved widget holds.
    assert!(enabled(&rt, "monitor-schedule-revoke"), "dirty keeps revoke live");
    assert!(!enabled(&bare_rt, "monitor-schedule-revoke"), "!grant disables");
}

#[test]
fn pressing_authorize_runs_the_change_head_and_busy_freezes_both() {
    let mut rt = rt(AuditState::base());
    press(&mut rt, "monitor-schedule-authorize");
    assert_eq!(rt.state.authorizes, 1, "change(false) dispatched");
    assert!(rt.state.card.busy, "the host applied begin_change");

    // The busy flag disables both buttons (React's two guards).
    assert!(!enabled(&rt, "monitor-schedule-authorize"));
    assert!(!enabled(&rt, "monitor-schedule-revoke"));
    // A second press on a disabled button fires nothing.
    press(&mut rt, "monitor-schedule-authorize");
    assert_eq!(rt.state.authorizes, 1);

    // The rejection lands as the alert line and busy clears (the finally).
    land(&mut rt, |s| s.card.change_failed("explicit database required".to_owned()));
    let alert = maybe(&rt, "monitor-schedule-error").expect("the alert exists");
    assert_eq!(alert.role, "alert", "React role=\"alert\"");
    assert_eq!(alert.text.as_deref(), Some("explicit database required"));
    assert!(enabled(&rt, "monitor-schedule-authorize"), "busy cleared");
}

#[test]
fn a_grant_shows_authorized_until_and_pressing_revoke_clears_it() {
    // React test 1's journey: authorize -> authorized until -> revoke ->
    // not authorized.
    let mut rt = rt(AuditState::with_grant(FUTURE, false));
    assert!(text_of(&rt, "monitor-schedule-status").starts_with("已授权至"));
    assert!(enabled(&rt, "monitor-schedule-revoke"));

    press(&mut rt, "monitor-schedule-revoke");
    assert_eq!(rt.state.revokes, 1, "change(true) dispatched");
    assert!(rt.state.card.busy);

    land(&mut rt, |s| s.card.change_resolved_revoked());
    assert_eq!(text_of(&rt, "monitor-schedule-status"), "定时刷新尚未授权。");
    assert!(!enabled(&rt, "monitor-schedule-revoke"), "grant gone: disabled again");
}

#[test]
fn a_revoked_grant_keeps_revoke_disabled_and_shows_unauthorized() {
    let rt = rt(AuditState::with_grant(FUTURE, true));
    assert!(!enabled(&rt, "monitor-schedule-revoke"), "grant.revoked disables");
    assert_eq!(text_of(&rt, "monitor-schedule-status"), "定时刷新尚未授权。");
}

#[test]
fn the_card_fits_the_window_in_both_themes() {
    for dark in [true, false] {
        let mut scene = AuditState::with_grant(FUTURE, false);
        scene.dirty = true; // the tallest variant: the save-first line shows
        scene.dark = dark;
        let rt = rt(scene);
        let card = maybe(&rt, "monitor-schedule-authorization").expect("the card is keyed");
        let [x, y, w, h] = card.rect;
        assert!(x >= -0.5 && y >= -0.5, "on-screen at {x},{y} (dark={dark})");
        assert!(x + w <= 960.5 && y + h <= 640.5, "inside the window (dark={dark})");
    }
}
