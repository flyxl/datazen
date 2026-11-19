//! Port of `packages/frontend/src/components/chart/LogScaleTooltip.tsx` (T136).
//!
//! The React component is a recharts `<Tooltip>` configured for the manual
//! log-scale pipeline: `mapToLogScale` (lib/chart/transform.ts) replaces each
//! series value with its log10 and stashes the pre-transform number under
//! `__orig__${key}`, and this component's custom `formatter` shows that
//! number back so users see real magnitudes instead of log10 values. The
//! `contentStyle` half is the floating content's chrome: the surface-alt box,
//! a 1pt edge, the 6pt radius, the fg ink and the 12px face.
//!
//! The app's own tooltip counterpart (`ui/tooltip.rs`, the kit `Tooltip` for
//! hints anchored to a trigger) is not this surface: recharts' `<Tooltip>` is
//! a content panel the chart floats over the plot, so what this port ships is
//! the content half — the formatter plus the chrome constructor — and the
//! five chart-engine tasks (the recharts `<Tooltip>`'s mount, triggering and
//! overlay anchoring) mount the box `build` returns.
//!
//! ## What this module owns, and the seams
//!
//! | concern | owner | shape here |
//! |---------|-------|-----------|
//! | the formatter + `contentStyle` chrome (this file's 30 lines) | T136 | [`format_entry`] + [`LogScaleTooltip::build`] |
//! | `formatCompact` (`lib/chart/format.ts`) | the lib/chart track | the injected `FormatCompact` parameter |
//! | writing the `__orig__${key}` slots (`mapToLogScale`, transform.ts) | the transform port (T182) | read-only: [`chart_payload`] surfaces `Some(n)` only when the payload slot holds a JS number |
//! | mounting/triggering the recharts `<Tooltip>` | the five chart-engine tasks | the engine formats each payload entry and mounts the built box in its own overlay |
//!
//! The transform's semantics are **not** ported here: this module reads one
//! slot per entry and never touches the log10 write side.
//!
//! ## Numeric semantics (React, arm by arm)
//!
//! * `logValue = typeof value === 'number' ? value : Number(value ?? 0)`:
//!   the family's `ChartValue` already carries the `string | number | null`
//!   space. `Number(str)` trims whitespace, maps `''` (and an all-space
//!   string) to 0, and parses the decimal grammar — `'12'`→12, `'1e2'`→100,
//!   `'12abc'`→NaN. Two JS literal forms deviate: hex (`'0x10'`→16 in JS)
//!   is outside Rust's decimal grammar, and Rust accepts case-insensitive
//!   `inf`/`nan` spellings JS rejects; the values crossing this seam are the
//!   chart pipeline's number-ish text, never JS literal forms.
//! * `nameStr = String(name ?? '')`: null/undefined → `''`, a string stays
//!   itself, a number takes JS number-to-string — `42`→`'42'`,
//!   `0.5`→`'0.5'`, `-0`→`'0'`, `NaN`→`'NaN'`, ±Infinity spelled out. JS
//!   switches to exponential spelling at 1e21/1e-7; Rust's shortest
//!   round-trip `Display` stays positional — the same round-tripped value,
//!   a different spelling only at the extremes.
//! * `orig` counts only when `typeof orig === 'number'`: the payload view's
//!   `Some(n)` is exactly that test, so a string number under
//!   `__orig__${name}` takes the fallback arm.
//! * The orig arm: `orig <= 0 ? '0' : formatCompact(orig)` — the literal
//!   `'0'` for 0, -0 and the negatives; NaN fails `<= 0` in JS and Rust
//!   alike, so a NaN orig reaches the seam as-is.
//! * The fallback arm: `Math.pow(10, logValue)` is `10f64::powf(log_value)`
//!   (`powi` cannot take fractional log values, and point-wise numeric
//!   fidelity is the directive); a NaN log value hands `10^NaN = NaN` to
//!   the seam unchanged — React's `formatCompact(NaN)` yields `'NaN'` via
//!   `NaN.toFixed(2)`, which the real seam port must reproduce.
//!
//! ## The chrome (CSS variables → kit tokens)
//!
//! | React `contentStyle` | the CSS variable it reads | kit token (precedent) |
//! |----------------------|---------------------------|------------------------|
//! | `background: var(--c-surface-alt, #111827)` | `--c-surface-alt` = Tailwind `bg-surface-alt` | `c.layer_01` (the chart toolbar's mapping) |
//! | `border: 1px solid var(--c-edge, #333)` | `--c-edge` = `border-edge` | `c.border_base` (the toolbar separator's mapping) |
//! | `borderRadius: 6` | — | `k.radius.md` (6pt in the built-in themes) |
//! | `color: var(--c-fg, #eee)` | `--c-fg` = `text-fg` | `c.text_strong` (ChartView's overlay-title mapping) |
//! | `fontSize: 12` | — | `k.ty.text_xsmall` (React `text-xs` = 12px, the toolbar/empty-state precedent) |
//!
//! The hex fallbacks (`#111827`/`#333`/`#eee`) are what the variables resolve
//! to when a theme pack omits them; the kit already carries the app theme, so
//! the port reads tokens and never spells a raw colour.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | the `contentStyle` box | `chart-log-scale-tooltip` |
//! | entry line *i* | `chart-log-scale-tooltip-item-{i}` |
//!
//! ## Deliberate deviations
//!
//! * recharts' default content renders each entry as separate
//!   name/separator/value spans (the entry's name, its default `' : '`
//!   separator, then the value); the port joins them into one text node per
//!   entry — the same a11y text, the same `contentStyle` ink. recharts also
//!   tints the item by the series colour; that tint is series data the
//!   formatter never sees, so the engine task applies it when it owns real
//!   series.
//! * `contentStyle` sets no padding; recharts' own default-content padding
//!   (and the axis-label row above the items) is the engine mount's concern,
//!   not this component's 30 lines — so the box adds none.
//! * A nameless entry renders the value alone (recharts' default content
//!   skips the name and its separator when the name is empty).
//!
//! The five chart-engine tasks are this module's production caller; until
//! one lands, only the tests exercise it, so the module carries the family's
//! dead-code allow (the T008/T059 pattern before ChartView landed).

#![allow(dead_code)]

use rune::{col, text, El};

use super::chart_canvas::{ChartDataPoint, ChartValue};
use crate::ui::kit::kit;

/// The `contentStyle` box recharts' `<Tooltip>` floats (this port's root).
pub(crate) const CONTENT_KEY: &str = "chart-log-scale-tooltip";
/// The slot prefix `mapToLogScale` (transform.ts) stashes pre-transform
/// values under. Shared with the transform port (T182), which writes it;
/// this module only reads one slot at a time.
pub(crate) const ORIG_PREFIX: &str = "__orig__";
/// React `border: '1px solid …'` — the ring's width.
const BORDER_WIDTH: f32 = 1.0;
/// recharts' default tooltip item separator.
const ITEM_SEPARATOR: &str = " : ";

/// The minimal payload view the formatter needs: recharts hands the entry's
/// whole data point as `props.payload`, and the formatter reads exactly one
/// slot — `__orig__${name}`. `Some(n)` means precisely
/// `typeof payload[key] === 'number'` (a string number is not a number);
/// `None` is every other slot, and a missing one.
pub(crate) type PayloadView<'a> = &'a dyn Fn(&str) -> Option<f64>;

/// The `formatCompact` seam (`lib/chart/format.ts`): the lib/chart track
/// owns the real port; the engine injects it here.
pub(crate) type FormatCompact<'a> = &'a dyn Fn(f64) -> String;

/// React's `<Tooltip formatter>` return: the `[display, name]` pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LogScaleTooltipEntry {
    /// The first tuple slot: the formatted display value.
    pub(crate) value: String,
    /// The second tuple slot: the name the entry is announced by
    /// (`String(name ?? '')`).
    pub(crate) name: String,
}

/// The minimal payload view over the family's `ChartDataPoint` — the adapter
/// the chart-engine track hands [`format_entry`] (recharts' `props.payload`
/// is the row's data point). Writing the `__orig__${key}` slots is
/// `mapToLogScale`'s job (the transform port); this reads one slot and
/// surfaces JS numbers only.
pub(crate) fn chart_payload(point: &ChartDataPoint) -> impl Fn(&str) -> Option<f64> + '_ {
    move |key| match point.get(key) {
        Some(ChartValue::Number(n)) => Some(*n),
        _ => None, // Text, Null or absent: not `typeof 'number'`
    }
}

/// React's `formatter`, arm by arm (the module docs spell out the JS
/// coercion rules). `payload` is the entry's data point viewed as one
/// `__orig__` read; `format_compact` is the injected lib/chart seam.
pub(crate) fn format_entry(
    value: &ChartValue,
    name: &ChartValue,
    payload: PayloadView<'_>,
    format_compact: FormatCompact<'_>,
) -> LogScaleTooltipEntry {
    let log_value = match value {
        ChartValue::Number(n) => *n, // typeof value === 'number'
        ChartValue::Null => 0.0,     // Number(value ?? 0) with value absent
        ChartValue::Text(s) => js_number(s),
    };
    let name_str = js_name_string(name);
    // props.payload?.[`__orig__${nameStr}`] — read one slot, numbers only.
    let value = match payload(&format!("{ORIG_PREFIX}{name_str}")) {
        // typeof orig === 'number': orig <= 0 renders the literal '0'
        // (0, -0 and the negatives); NaN fails the test in JS and Rust
        // alike and reaches the seam.
        Some(orig) if orig <= 0.0 => "0".to_string(),
        Some(orig) => format_compact(orig),
        // Not a number (or no slot): show 10^logValue instead.
        None => format_compact(10.0_f64.powf(log_value)),
    };
    LogScaleTooltipEntry {
        value,
        name: name_str,
    }
}

/// The tooltip content: [`format_entry`]'s results in the `contentStyle`
/// chrome. The five chart-engine tasks mount it — recharts' `<Tooltip>`
/// mount, triggering and overlay anchoring are theirs; this box is the
/// content they float.
pub(crate) struct LogScaleTooltip<'a> {
    entries: &'a [LogScaleTooltipEntry],
}

impl<'a> LogScaleTooltip<'a> {
    /// One box for the already-formatted entries (payload order).
    pub(crate) fn new(entries: &'a [LogScaleTooltipEntry]) -> Self {
        Self { entries }
    }

    /// Builds the content box. `dark` picks the kit theme the tokens read.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let c = &k.c;
        col()
            .key(CONTENT_KEY)
            .fill(c.layer_01) // background: var(--c-surface-alt, #111827)
            .border(BORDER_WIDTH, c.border_base) // border: 1px solid var(--c-edge, #333)
            .radius(k.radius.md) // borderRadius: 6
            .children(self.entries.iter().enumerate().map(|(index, entry)| {
                text(item_text(entry))
                    .key(format!("{CONTENT_KEY}-item-{index}"))
                    .font(k.ty.text_xsmall) // fontSize: 12 (React `text-xs` = 12px)
                    .color(c.text_strong) // color: var(--c-fg, #eee)
            }))
    }
}

/// One entry's line: recharts' default item is the name, its `' : '`
/// separator and the value; a nameless entry drops the name and separator.
fn item_text(entry: &LogScaleTooltipEntry) -> String {
    if entry.name.is_empty() {
        entry.value.clone()
    } else {
        format!("{}{ITEM_SEPARATOR}{}", entry.name, entry.value)
    }
}

/// JS `Number(str)`: whitespace is trimmed, `''` is 0, the decimal grammar
/// parses, and everything else is NaN. The JS-only literal forms (hex,
/// case-insensitive inf spellings) deviate — see the module docs.
fn js_number(s: &str) -> f64 {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0.0; // Number('') === Number('   ') === 0
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

/// JS `String(name ?? '')` over the family's value space.
fn js_name_string(name: &ChartValue) -> String {
    match name {
        ChartValue::Null => String::new(), // name ?? '' → String('')
        ChartValue::Text(s) => s.clone(),
        ChartValue::Number(n) => js_number_to_string(*n),
    }
}

/// JS `String(n)` for the representable forms. Rust's `Display` is the
/// shortest round-trip decimal; JS switches to exponential spelling at
/// 1e21/1e-7 — the same value, a different spelling only at the extremes.
fn js_number_to_string(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n.is_infinite() {
        if n < 0.0 {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        }
    } else if n == 0.0 {
        // String(0) and String(-0) are both '0'.
        "0".to_string()
    } else {
        n.to_string()
    }
}
