//! Port of `packages/frontend/src/windows/connection/er/ErRelationLegend.tsx`.
//!
//! The diagram's key to its own lines plus the switch that adds inferred ones.
//! React shares `relationStyle.ts` with the canvas so the swatch is literally the
//! stroke drawn on an edge. Rune has no SVG here, so the swatches are drawn with
//! kit-token fills and an explicit 4-on / 3-off dash (the React
//! `ER_PREDICTED_DASH`) as 4pt segments; the canvas will read the same constants
//! from this module when it is ported, so a second copy cannot drift.
//!
//! ## Keys
//!
//! React `data-testid`s are preserved: `er-diagram-legend`,
//! `er-legend-declared`, `er-legend-predicted`, `er-predicted-count`,
//! `er-diagram-toggle-prediction`. The title, labels and swatches have no React
//! test id; keys named after them are Rune-side observation handles only.
//!
//! No new colors: every surface/ink/swatch comes from `kit(dark)` tokens. The
//! `Sparkles` glyph comes from the icon registry.

use std::rc::Rc;

use rune::{col, icon, row, text, El};

use crate::i18n::t;
use crate::icons::ICON_SPARKLES;
use crate::ui::kit::kit;
use rune_kit::{Button, ButtonVariant, ControlSize};

pub(crate) const ROOT_KEY: &str = "er-diagram-legend";
pub(crate) const DECLARED_KEY: &str = "er-legend-declared";
pub(crate) const PREDICTED_KEY: &str = "er-legend-predicted";
pub(crate) const PREDICTED_COUNT_KEY: &str = "er-predicted-count";
pub(crate) const TOGGLE_KEY: &str = "er-diagram-toggle-prediction";

/// React panel width `w-56` (14rem) and the 26×8 SVG swatch with a 2pt stroke.
pub(crate) const PANEL_WIDTH: f32 = 224.0;
const SWATCH_WIDTH: f32 = 24.0;
const SWATCH_HEIGHT: f32 = 2.0;
/// `ER_PREDICTED_DASH = '4 3'` as `(on, off)` points, derived from the
/// relation-style constant the canvas edges are painted with (T568): the
/// legend must not keep a second copy that can drift.
pub(crate) const PREDICTED_DASH: (f32, f32) = dash_points(super::relation_style::ER_PREDICTED_DASH);

/// Parse a `'on off'` dash pattern into points. A malformed pattern falls
/// back to the canvas default rather than collapsing the swatch to nothing.
const fn dash_points(pattern: &str) -> (f32, f32) {
    let bytes = pattern.as_bytes();
    let mut on: f32 = 4.0;
    let mut off: f32 = 3.0;
    let mut index = 0;
    let mut seen_on = false;
    let mut value: f32 = 0.0;
    let mut any_digit = false;
    // Two space-separated integers in a const fn: hand-rolled, no allocation.
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_digit() {
            value = value * 10.0 + (byte - b'0') as f32;
            any_digit = true;
        } else if byte == b' ' && any_digit {
            if !seen_on {
                on = value;
                seen_on = true;
            } else {
                off = value;
            }
            value = 0.0;
            any_digit = false;
        }
        index += 1;
    }
    if any_digit {
        if !seen_on {
            on = value;
        } else {
            off = value;
        }
    }
    (on, off)
}

/// React `ErRelationLegendProps` minus the host callback.
#[allow(dead_code)] // view VM; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ErRelationLegendVm {
    pub declared_count: usize,
    pub predicted_count: usize,
    pub prediction_enabled: bool,
}

/// Solid swatch: one line in the canvas's declared stroke.
fn declared_swatch(dark: bool) -> El {
    let k = kit(dark);
    row()
        .key("er-legend-declared-swatch")
        .w(SWATCH_WIDTH)
        .h(SWATCH_HEIGHT)
        .radius(1.0)
        .fill(k.c.text_accent)
        .child(
            row()
                .key("er-legend-declared-swatch-line")
                .w(SWATCH_WIDTH)
                .h(SWATCH_HEIGHT)
                .fill(k.c.text_accent),
        )
}

/// Dashed swatch: 4pt warning strokes separated by 3pt gaps, matching the
/// canvas's `strokeDasharray="4 3"`.
fn predicted_swatch(dark: bool) -> El {
    let k = kit(dark);
    let mut swatch = row()
        .key("er-legend-predicted-swatch")
        .w(SWATCH_WIDTH)
        .h(SWATCH_HEIGHT)
        .gap(PREDICTED_DASH.1);
    // 24 = 4 + (3 + 4) * 3 - 3, four 4pt dashes with three 3pt gaps.
    for index in 0..4 {
        swatch = swatch.child(
            row()
                .key(format!("er-legend-predicted-dash-{index}"))
                .w(PREDICTED_DASH.0)
                .h(SWATCH_HEIGHT)
                .radius(1.0)
                .fill(k.c.warning.fg),
        );
    }
    swatch
}

/// Build the legend. `on_toggle` reports the requested new setting state; the
/// host owns the persisted "smart foreign key prediction" flag.
#[allow(dead_code)] // view entry point; consumer: the ER canvas host (not wired).
pub(crate) fn er_relation_legend<S: 'static>(
    vm: &ErRelationLegendVm,
    dark: bool,
    on_toggle: Option<Rc<dyn Fn(&mut S, bool) + 'static>>,
) -> El {
    let k = kit(dark);

    let declared = row()
        .key(DECLARED_KEY)
        .w_full()
        .gap(8.0)
        .center_y()
        .child(declared_swatch(dark))
        .child(
            text(t("erDiagram.legendDeclared"))
                .key("er-legend-declared-label")
                .font(k.ty.text_xsmall)
                .color(k.c.text_base)
                .wrap(),
        )
        .child(
            text(vm.declared_count.to_string())
                .key("er-legend-declared-count")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );

    let mut predicted = row()
        .key(PREDICTED_KEY)
        .w_full()
        .gap(8.0)
        .center_y()
        // React puts `opacity-60` on the whole predicted row, not just its
        // count, so the swatch, label and count all dim together.
        .opacity(if vm.prediction_enabled { 1.0 } else { 0.6 })
        .child(predicted_swatch(dark))
        .child(
            text(t("erDiagram.legendPredicted"))
                .key("er-legend-predicted-label")
                .font(k.ty.text_xsmall)
                .color(if vm.prediction_enabled {
                    k.c.text_base
                } else {
                    k.c.text_muted
                })
                .wrap(),
        );
    // The count inherits the row's opacity; no count-only dim here (that would
    // double-apply it and leave the swatch/label bright).
    predicted = predicted.child(
        text(vm.predicted_count.to_string())
            .key(PREDICTED_COUNT_KEY)
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted),
    );

    let label = if vm.prediction_enabled {
        t("erDiagram.predictionOn")
    } else {
        t("erDiagram.predictionOff")
    };
    let mut button = Button::new(label)
        .variant(if vm.prediction_enabled {
            ButtonVariant::Neutral
        } else {
            ButtonVariant::Ghost
        })
        .control_size(ControlSize::Small)
        .icon(&ICON_SPARKLES)
        .help(t("erDiagram.predictionToggleHint").to_owned());
    if let Some(on_toggle) = on_toggle {
        let requested = !vm.prediction_enabled;
        button = button.on_press(move |s: &mut S| on_toggle(s, requested));
    }
    let toggle = button
        .build(k)
        .key(TOGGLE_KEY)
        .w_full()
        .checked(vm.prediction_enabled);

    col()
        .key(ROOT_KEY)
        .w(PANEL_WIDTH)
        .radius(8.0)
        .border(1.0, k.c.border_base)
        .fill(k.c.surface_base)
        .child(
            col()
                .key("er-legend-title")
                .w_full()
                .child(
                    text(t("erDiagram.legendTitle"))
                        .key("er-legend-title-text")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                ),
        )
        .child(declared)
        .child(predicted)
        .child(toggle)
}

// Leftover L1 (removal stage): mount this legend from the ER canvas host when
// that view lands, binding `on_toggle` to the persisted prediction setting;
// share the dash/color constants with the canvas renderer and drop the allows.
