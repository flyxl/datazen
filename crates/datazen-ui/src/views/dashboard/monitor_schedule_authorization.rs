//! Port of `packages/frontend/src/windows/dashboard/MonitorScheduleAuthorization.tsx`
//! (T146).
//!
//! The scheduled-refresh authorization card the widget editor drawer mounts
//! for an interval widget: it loads the widget's frozen monitor grant, shows
//! whether that grant still authorizes future runs (`authorized until …` /
//! `not authorized`), and offers the one-hour authorize and revoke actions
//! with React's disabled guards.
//!
//! ## What maps where
//!
//! - `dashboardCommands.getMonitorScheduleGrant(dashboardId, widgetId)` and
//!   the two mutations are host (IPC) concerns, not Rune's. The host watches
//!   the `[dashboardId, widgetId]` pair (React's effect deps, mount
//!   included) through [`MonitorScheduleAuthorizationState::needs_resync`],
//!   applies [`MonitorScheduleAuthorizationState::begin_resync`] when it
//!   flips, fetches out-of-band and delivers the answer with
//!   `deliver_grant` / `deliver_resync_error`.
//! - React's `active` flag — set false by the effect's cleanup when the ids
//!   change or the card unmounts — is the `request_epoch` token (the T018
//!   convention): a delivery carrying a superseded token returns `false`
//!   and lands nothing, exactly as a cancelled promise's `setState` never
//!   runs. The authorize/revoke answers carry no token in React (they land
//!   even if the ids changed mid-flight) and the port keeps that.
//! - `change(revoke)` splits the way the settled-promise ports do: the
//!   synchronous head (`setBusy(true); setError(null)`) is
//!   [`MonitorScheduleAuthorizationState::begin_change`] — the host calls it
//!   inside the button callback before dispatching — and the three
//!   resolutions are `change_resolved_with_grant` (authorize's `.then`),
//!   `change_resolved_revoked` (revoke's `setGrant(null)` + `finally`) and
//!   `change_failed` (`.catch` + `finally`). `String(reason)` — JS's
//!   stringification of the rejection — is the host's job; the state takes
//!   the finished text.
//! - React's `MonitorScheduleGrantSummary` is the wire type; the card reads
//!   only `expiresAt` + `revoked`, so [`MonitorGrantVm`] is the projection
//!   (the `DashboardOptionVm` convention of T018).
//! - `authorized` — `grant && !grant.revoked && Date.parse(grant.expiresAt)
//!   > Date.now()`, evaluated at render — is
//!   [`MonitorScheduleAuthorizationState::authorized`] with the host's
//!   clock injected per build (no wall-clock read inside a build).
//!   Unparseable text is `None`, which is React's `NaN > now` → false. The
//!   parser is `support::formatters::parse_zoned_instant`, the same one
//!   behind the label, so one stance covers both reads; zone-less text
//!   (which V8 would read as local time) is therefore unauthorized — the
//!   backend emits RFC3339, so the edge is documented, not reachable.
//! - `new Date(grant.expiresAt).toLocaleString()` is
//!   `views::workflow::history_section::locale_string` — the stance that
//!   helper already owns for the same React expression (local-zone
//!   `YYYY-MM-DD HH:MM:SS`, unparseable input verbatim). The `{time}`
//!   placeholder is replaced in the translated string, as React's
//!   `t('…', { time })` interpolation does.
//! - React's `dirty` prop (the drawer's `draft !== widget` comparison) is a
//!   builder input, not state: the immediate-mode build reads it fresh.
//!
//! ## Keys
//!
//! React renders no `data-testid` here; the keys derive from the card:
//!
//! | part | key |
//! |------|-----|
//! | section root | `monitor-schedule-authorization` |
//! | title line | `monitor-schedule-title` |
//! | hint line | `monitor-schedule-hint` |
//! | status line | `monitor-schedule-status` |
//! | save-first line | `monitor-schedule-save-first` |
//! | error line (`role="alert"`) | `monitor-schedule-error` |
//! | actions row | `monitor-schedule-actions` |
//! | authorize button | `monitor-schedule-authorize` |
//! | revoke button | `monitor-schedule-revoke` |
//!
//! Known deltas (documented, not silent): kit button skins stand in for the
//! React variants — `secondary` (transparent + edge border + fg text) is the
//! kit `Outline` skin and `ghost` is `Ghost` (the T022 mapping) — and
//! `size="sm"` (h-7) renders `ControlSize::Large` (the wapp-card mapping);
//! `toLocaleString`'s locale-specific ordering is the `locale_string` stance
//! above. Nothing else is dropped: the section's border, padding, gap and
//! text sizes are Tailwind's values verbatim.

use std::rc::Rc;

use rune::{col, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::support::formatters::parse_zoned_instant;
use crate::ui::kit::kit;
use crate::views::locale_string;

// ---- Geometry (Tailwind, verbatim) ------------------------------------------

/// React `space-y-2` (the section) and `gap-2` (the button row).
const GAP: f32 = 8.0;
/// React `p-3`.
const PAD: f32 = 12.0;
/// React `text-xs`.
const TEXT_XS: f32 = 12.0;

/// React's `text-xs` regular (12px / 400).
fn type_xs() -> TextStyle {
    TextStyle::new(TEXT_XS, 400.0)
}

// ---- The grant projection ---------------------------------------------------

/// The slice of React's `MonitorScheduleGrantSummary` the card reads (the
/// host converts the wire type; `id` / `workflowRevision` / the scope lists
/// are the drawer's and the backend's business).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitorGrantVm {
    /// RFC3339 from the backend; unparseable text counts as expired
    /// (React's `Date.parse` → `NaN`).
    pub expires_at: String,
    pub revoked: bool,
}

// ---- State ------------------------------------------------------------------

/// The card's three `useState` values plus the fetch token, kept in the
/// owning state `S`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, Default)]
pub struct MonitorScheduleAuthorizationState {
    pub grant: Option<MonitorGrantVm>,
    pub busy: bool,
    pub error: Option<String>,
    /// Bumped on every resync; the effect-cleanup `active` flag. A delivery
    /// carrying an older token is dropped.
    pub request_epoch: u64,
    /// The pair the current epoch was issued for (React's dep array); the
    /// host compares through [`Self::needs_resync`].
    pub dashboard_id: String,
    pub widget_id: String,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl MonitorScheduleAuthorizationState {
    /// React's dep-array check: `[dashboardId, widgetId]` changed (or this
    /// is the mount — the empty default never matches a real pair).
    pub fn needs_resync(&self, dashboard_id: &str, widget_id: &str) -> bool {
        self.dashboard_id != dashboard_id || self.widget_id != widget_id
    }

    /// The effect's synchronous half: drop the grant and the error, then
    /// bump the cancel token (the host fetches and delivers). `busy` is
    /// deliberately untouched — React's effect does not clear it either.
    pub fn begin_resync(&mut self, dashboard_id: &str, widget_id: &str) {
        self.dashboard_id = dashboard_id.to_owned();
        self.widget_id = widget_id.to_owned();
        self.request_epoch = self.request_epoch.saturating_add(1);
        self.grant = None;
        self.error = None;
    }

    /// The fetch's `.then`: the grant lands. Returns `false` — with the
    /// state untouched — when the token is stale, which is React's
    /// `cancelled` guard.
    pub fn deliver_grant(&mut self, grant: MonitorGrantVm, token: u64) -> bool {
        if token != self.request_epoch {
            return false;
        }
        self.grant = Some(grant);
        true
    }

    /// The fetch's `.catch`: `setError(String(reason))` under the same
    /// token guard.
    pub fn deliver_resync_error(&mut self, reason: String, token: u64) -> bool {
        if token != self.request_epoch {
            return false;
        }
        self.error = Some(reason);
        true
    }

    /// The synchronous head of `change` (identical for both branches):
    /// `setBusy(true); setError(null)`. The host applies this inside the
    /// button callback before dispatching the IPC.
    pub fn begin_change(&mut self) {
        self.busy = true;
        self.error = None;
    }

    /// `change(false)`'s `.then` + `finally`: the fresh grant lands and the
    /// busy flag clears. Token-free, like React's.
    pub fn change_resolved_with_grant(&mut self, grant: MonitorGrantVm) {
        self.grant = Some(grant);
        self.busy = false;
    }

    /// `change(true)`'s `.then` + `finally`: `setGrant(null)` and the busy
    /// flag clears.
    pub fn change_resolved_revoked(&mut self) {
        self.grant = None;
        self.busy = false;
    }

    /// `change`'s `.catch` + `finally`: the error text lands, busy clears.
    pub fn change_failed(&mut self, reason: String) {
        self.error = Some(reason);
        self.busy = false;
    }

    /// React's `authorized` derivation against the host's clock:
    /// `grant && !grant.revoked && Date.parse(expiresAt) > now`. Unparseable
    /// text is `NaN > now` → false; an expiry exactly at `now` is false
    /// (React's strict `>`).
    pub fn authorized(&self, now_ms: i64) -> bool {
        match self.grant.as_ref() {
            Some(grant) if !grant.revoked => {
                parse_zoned_instant(&grant.expires_at).is_some_and(|ms| ms > now_ms)
            }
            _ => false,
        }
    }

    /// The status line's text: `t('dashboard.scheduleAuthorizedUntil',
    /// { time: new Date(expiresAt).toLocaleString() })` while authorized,
    /// else `t('dashboard.scheduleUnauthorized')`.
    pub fn status_line(&self, now_ms: i64) -> String {
        if self.authorized(now_ms) {
            // `authorized` implies the grant is present; the `else` arm is
            // the compiler's, not a state the card can reach.
            if let Some(grant) = &self.grant {
                let time = locale_string(&grant.expires_at);
                return t("dashboard.scheduleAuthorizedUntil").replace("{time}", &time);
            }
        }
        t("dashboard.scheduleUnauthorized").to_owned()
    }
}

// ---- Builder ----------------------------------------------------------------

type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// Builder for the card, generic over the app state `S` that owns the state
/// slice (the ported-dialog pattern of T018/T069).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub struct MonitorScheduleAuthorization<'a, S: 'static> {
    state: &'a MonitorScheduleAuthorizationState,
    /// React's `dirty` prop: unsaved edits add the save-first line and
    /// disable the authorize button (the revoke guard reads only the grant).
    dirty: bool,
    /// `Date.now()` in Unix epoch millis, injected per build.
    now_ms: i64,
    on_authorize: Option<ActionFn<S>>,
    on_revoke: Option<ActionFn<S>>,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl<'a, S: 'static> MonitorScheduleAuthorization<'a, S> {
    /// `now_ms` is the render-time `Date.now()` (the host's clock; the
    /// authorized derivation reads it, exactly as React re-evaluates the
    /// expression on every render).
    pub fn new(state: &'a MonitorScheduleAuthorizationState, now_ms: i64) -> Self {
        Self { state, dirty: false, now_ms, on_authorize: None, on_revoke: None }
    }

    /// React's `dirty` prop.
    pub fn dirty(mut self, dirty: bool) -> Self {
        self.dirty = dirty;
        self
    }

    /// The authorize button's press — React's `change(false)`. The host
    /// applies `begin_change` and dispatches `authorizeMonitorSchedule`; the
    /// answer lands through `change_resolved_with_grant` /
    /// `change_failed`.
    pub fn on_authorize(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_authorize = Some(Rc::new(f));
        self
    }

    /// The revoke button's press — React's `change(true)`. The host applies
    /// `begin_change` and dispatches `revokeMonitorSchedule`; the answer
    /// lands through `change_resolved_revoked` / `change_failed`.
    pub fn on_revoke(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_revoke = Some(Rc::new(f));
        self
    }

    /// Builds the `<section>`. It always paints: React's parent gates the
    /// mount on `refresh.mode === 'interval'`, which is the drawer's
    /// concern, not the card's.
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut section = col()
            .key("monitor-schedule-authorization")
            .w_full()
            .gap(GAP) // React space-y-2
            .radius(k.radius.md) // rounded-md
            .border(1.0, k.c.border_base) // border border-edge
            .padding(PAD) // p-3
            .child(
                text(t("dashboard.scheduleAuthorization"))
                    .key("monitor-schedule-title")
                    .font(TextStyle::new(TEXT_XS, 500.0)) // text-xs font-medium
                    .color(k.c.text_base), // text-fg
            )
            .child(
                text(t("dashboard.scheduleAuthorizationHint"))
                    .key("monitor-schedule-hint")
                    .font(type_xs())
                    .color(k.c.text_muted), // text-fg-muted
            )
            .child(
                text(self.state.status_line(self.now_ms))
                    .key("monitor-schedule-status")
                    .font(type_xs())
                    .color(k.c.text_muted), // text-fg-muted
            );
        // React `{dirty && …}`: the line exists only while dirty.
        if self.dirty {
            section = section.child(
                text(t("dashboard.scheduleSaveFirst"))
                    .key("monitor-schedule-save-first")
                    .font(type_xs())
                    .color(k.c.text_muted), // text-fg-muted
            );
        }
        // React `{error && …}`: the alert exists only with an error.
        if let Some(error) = &self.state.error {
            section = section.child(
                text(error.clone())
                    .key("monitor-schedule-error")
                    .role("alert")
                    .font(type_xs())
                    .color(k.c.danger.fg), // React text-red-400
            );
        }
        section.child(self.actions(&k))
    }

    /// React's `<div className="flex gap-2">` button row.
    fn actions(&self, k: &Kit) -> El {
        row()
            .key("monitor-schedule-actions")
            .gap(GAP) // React gap-2
            .child(self.authorize_button(k))
            .child(self.revoke_button(k))
    }

    /// The authorize button: `variant="secondary" size="sm"`, disabled by
    /// `busy || dirty` — unsaved edits must not be scheduled.
    fn authorize_button(&self, k: &Kit) -> El {
        let disabled = self.state.busy || self.dirty;
        let mut button = Button::new(t("dashboard.scheduleAuthorizeHour"))
            .variant(ButtonVariant::Outline) // React secondary (T022 mapping)
            .control_size(ControlSize::Large) // React size="sm" (h-7)
            .disabled(disabled);
        if !disabled {
            if let Some(authorize) = &self.on_authorize {
                let authorize = Rc::clone(authorize);
                button = button.on_press(move |s: &mut S| authorize(s));
            }
        }
        button.build(k).key("monitor-schedule-authorize")
    }

    /// The revoke button: `variant="ghost" size="sm"`, disabled by
    /// `busy || !grant || grant.revoked` — note React does NOT include
    /// `dirty` or expiry here, so an expired-but-unrevoked grant keeps the
    /// button enabled.
    fn revoke_button(&self, k: &Kit) -> El {
        let no_grant_or_revoked =
            self.state.grant.as_ref().map_or(true, |grant| grant.revoked);
        let disabled = self.state.busy || no_grant_or_revoked;
        let mut button = Button::new(t("dashboard.scheduleRevoke"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Large) // React size="sm" (h-7)
            .disabled(disabled);
        if !disabled {
            if let Some(revoke) = &self.on_revoke {
                let revoke = Rc::clone(revoke);
                button = button.on_press(move |s: &mut S| revoke(s));
            }
        }
        button.build(k).key("monitor-schedule-revoke")
    }
}
