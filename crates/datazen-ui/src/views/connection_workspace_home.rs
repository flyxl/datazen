//! T144 support for `windows/connection/ConnectionWorkspaceHome.tsx`.
//!
//! This module holds the three pieces the A1-era `connections_home.rs` did not
//! have yet, without moving any of its existing translation call sites (the
//! i18n sweep pins them per file):
//!
//! * [`home_copy_button`] — a copy affordance with the React 2000 ms feedback
//!   window. It copies through `Tx`, schedules a **real runtime wake** and
//!   records a deadline in `AppState`; the label reverts when the app poll's
//!   [`home_copy_tick`] sees the deadline pass. Not a never-expiring boolean.
//! * [`kv_connection_home`] — the driver-claimed connection home (React state
//!   3b / 屏 A). The host sets [`ConnectionHomeSlotVm`] only for a live session,
//!   and the whole landing surface yields to it while the root test id survives.
//! * [`react_key`] — a zero-size marker carrying a React `data-testid` that has
//!   no Rune counterpart yet. Markers are inert: they never change hit-testing.
//!
//! Removal stage: the AI chat panel / driver-slot host work that consumes these
//! bindings is not landed; delete this module when the host wires the real
//! `KvConnectionHomeBinding` and the shared copy-feedback component.

use std::time::Duration;

use rune::{col, el, row, text, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_COPY};
use crate::state_def::AppState;
use crate::ui::kit::kit;

use super::engine_label;

/// React `COPIED_FEEDBACK_MS` (both the home row and the MCP bar use 2000 ms).
#[allow(dead_code)] // consumed by home_copy_button; delete with the host wiring.
pub(crate) const HOME_COPY_FEEDBACK_MS: u64 = 2000;

/// Minimal Rust projection of `KvConnectionHomeBinding`'s props. The real
/// driver binding is a later task; removal stage: when it lands, carry that
/// type in `AppState` and delete this stand-in.
#[allow(dead_code)] // host binding; wired by the driver-slot task.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConnectionHomeSlotVm {
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
    pub initial_database: String,
}

/// A zero-size, inert keyed marker for a React `data-testid` that has no Rune
/// element of its own. Keeps React's observable keys without disturbing the
/// existing A1 keys that other tests depend on.
#[allow(dead_code)] // key parity marker; delete with the A1 key migration.
pub(crate) fn react_key(key: &str) -> El {
    el().key(key).a11y_hidden()
}

/// Revert the feedback label once the real deadline passes. Called from the
/// app poll with the runtime clock (seconds). Returns `true` when visible state
/// changed, so the caller can ask for a redraw.
#[allow(dead_code)] // host poll hook; delete with the shared copy component.
pub(crate) fn home_copy_tick(state: &mut AppState, now: f64) -> bool {
    if state.home_copied_until.is_some_and(|until| now >= until) {
        state.home_copied_until = None;
        state.home_copied_id = None;
        true
    } else {
        false
    }
}

/// Whether `row_id` currently owns the per-row "copied" marker.
#[allow(dead_code)] // rendering helper; delete with the shared copy component.
pub(crate) fn home_row_copied(state: &AppState, row_id: &str) -> bool {
    state.home_copied_until.is_some()
        && state.home_copied_id.as_deref() == Some(row_id)
}

/// A kit button that copies `text`, arms a real 2000 ms wake and records the
/// deadline in `AppState`. `copy_label` is the caller's translated idle label,
/// so existing `t("common.copy")` call sites stay in `connections_home.rs`.
#[allow(dead_code)] // consumer: the connections home view (this wave).
pub(crate) fn home_copy_button(
    state: &AppState,
    key: &str,
    row_id: &str,
    text: &str,
    copy_label: &str,
    dark: bool,
) -> El {
    let k = kit(dark);
    let copied = home_row_copied(state, row_id);
    let label = if copied {
        t("connWin.home.aiIntegration.copied")
    } else {
        copy_label
    };
    let glyph = if copied { &ICON_CHECK } else { &ICON_COPY };

    let mut button = Button::new(label)
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .icon(glyph);
    let text = text.to_owned();
    let row = row_id.to_owned();
    button = button.on_press_tx(move |state: &mut AppState, tx: &mut Tx| {
        tx.copy(text.clone());
        // A real deadline: the runtime wakes the app poll at expiry, and the
        // poll's `home_copy_tick` clears `home_copied_until` for the next frame.
        tx.wake_after(Duration::from_millis(HOME_COPY_FEEDBACK_MS));
        state.home_copied_until =
            Some(tx.clock() + HOME_COPY_FEEDBACK_MS as f64 / 1000.0);
        state.home_copied_id = Some(row.clone());
    });
    button.build(k).key(key)
}

/// React state 3b: a driver-contributed connection home takes over the whole
/// landing surface. The host only sets `home_slot` for a live session, which is
/// what React's "no context → no takeover" case reduces to here.
#[allow(dead_code)] // consumer: the connections home view (this wave).
pub(crate) fn kv_connection_home(
    state: &AppState,
    theme: rune::tok::Theme,
    _dark: bool,
) -> El {
    let Some(slot) = state.home_slot.as_ref() else {
        return el().key("home-kv-connection-home-missing").a11y_hidden();
    };
    col()
        .key("connection-workspace-home")
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .child(
            col()
                .key("home-kv-connection-home")
                .label("kv-connection-home")
                .grow(1.0)
                .min_h(0.0)
                .w_full()
                .gap(6.0)
                .px(16.0)
                .py(12.0)
                .child(react_key("fixture-connection-home"))
                .child(kv_field("home-kv-connection-id", &slot.connection_id, theme))
                .child(kv_field("home-kv-db-session-id", &slot.db_session_id, theme))
                .child(kv_field("home-kv-connection-name", &slot.connection_name, theme))
                .child(kv_field(
                    "home-kv-database-type",
                    &engine_label(&slot.database_type),
                    theme,
                ))
                .child(kv_field(
                    "home-kv-initial-database",
                    &slot.initial_database,
                    theme,
                )),
        )
}

fn kv_field(key: &str, value: &str, theme: rune::tok::Theme) -> El {
    row()
        .key(key)
        .w_full()
        .gap(8.0)
        .center_y()
        .child(
            text(key.trim_start_matches("home-kv-"))
                .font(rune::tok::type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(
            text(value)
                .font(rune::tok::type_::LABEL)
                .color(theme.text),
        )
}
