//! The value editor of a range operator (`between` / `notBetween`): two bounds
//! with an `AND` between them.
//!
//! Ported 1:1 from `packages/frontend/src/components/filter/FilterBetweenInput.tsx`.
//!
//! **The bounds are never swapped.** A user who typed the bounds in the wrong
//! order gets a warning, not a silent correction: swapping them would turn a
//! typo into a query with a different (and plausible looking) result.
//!
//! ## Key convention
//!
//! | part     | key                  |
//! |----------|----------------------|
//! | root     | `<key>`              |
//! | lower    | `<key>-lower`        |
//! | connector| `<key>-connector`    |
//! | upper    | `<key>-upper`        |
//! | warning  | `<key>-inverted`     |
//!
//! React gives both inputs the same `data-testid="filter-value"`; Rune keys
//! must be unique in a window, so they are suffixed by bound.

// The filter panel view (T024) is the production consumer; until it lands the
// component tests are the only callers, so the dead-code lint is silenced here
// rather than in a crate-wide allow.
#![allow(dead_code)]

use rune::{row, text, El, TextEdit};

use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// A pair-bound editor over two text drafts.
///
/// The component edits the drafts directly (React's `onCommit` fires on every
/// keystroke); the parent reads the two `TextEdit`s when it builds a query.
pub(crate) struct FilterBetweenInput<'a, S> {
    key: String,
    lower: &'a TextEdit,
    upper: &'a TextEdit,
    on_lower: fn(&mut S) -> &mut TextEdit,
    on_upper: fn(&mut S) -> &mut TextEdit,
    placeholder: String,
    connector_label: String,
    inverted_label: String,
}

impl<'a, S: 'static> FilterBetweenInput<'a, S> {
    /// The editor of `lower` / `upper`, 28 points tall, 80 points wide, with
    /// the caller's two draft accessors (a text field without `on_edit` cannot
    /// take focus).
    pub(crate) fn new(
        key: impl Into<String>,
        lower: &'a TextEdit,
        upper: &'a TextEdit,
        on_lower: fn(&mut S) -> &mut TextEdit,
        on_upper: fn(&mut S) -> &mut TextEdit,
    ) -> FilterBetweenInput<'a, S> {
        FilterBetweenInput {
            key: key.into(),
            lower,
            upper,
            on_lower,
            on_upper,
            placeholder: String::new(),
            connector_label: crate::i18n::t("filter.and").to_owned(),
            inverted_label: crate::i18n::t("filter.betweenInverted").to_owned(),
        }
    }

    /// The hint shown inside both bounds (React `placeholder`).
    pub(crate) fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// The text between the two fields, e.g. `AND` (React `connectorLabel`).
    pub(crate) fn connector_label(mut self, label: impl Into<String>) -> Self {
        self.connector_label = label.into();
        self
    }

    /// The warning shown when the lower bound is above the upper bound.
    pub(crate) fn inverted_label(mut self, label: impl Into<String>) -> Self {
        self.inverted_label = label.into();
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let inverted = is_inverted(&self.lower.text, &self.upper.text);

        let mut line = row().key(self.key.clone()).gap(4.0).center_y().min_w(0.0);

        line = line.child(
            TextField::new(format!("{}-lower", self.key), self.lower)
                .height(28.0)
                .width(80.0)
                .pad_x(8.0)
                .placeholder(self.placeholder.clone())
                .on_edit(self.on_lower)
                .build(dark),
        );
        line = line.child(
            text(self.connector_label.clone())
                .key(format!("{}-connector", self.key))
                .font(k.ty.small)
                .color(k.c.text_muted),
        );
        line = line.child(
            TextField::new(format!("{}-upper", self.key), self.upper)
                .height(28.0)
                .width(80.0)
                .pad_x(8.0)
                .placeholder(self.placeholder)
                .on_edit(self.on_upper)
                .build(dark),
        );
        if inverted {
            line = line.child(
                text(self.inverted_label.clone())
                    .key(format!("{}-inverted", self.key))
                    .font(k.ty.small)
                    .color(k.c.warning.fg)
                    .description(self.inverted_label),
            );
        }
        line
    }
}

/// Whether the lower bound is above the upper bound.
///
/// Only comparable values count: two numbers, or two timestamps the date
/// parser understands. Everything else is left alone — a warning that shows up
/// for a value pair that is not comparable is noise.
pub(crate) fn is_inverted(lower: &str, upper: &str) -> bool {
    let low = lower.trim();
    let high = upper.trim();
    if low.is_empty() || high.is_empty() {
        return false;
    }

    if let (Ok(a), Ok(b)) = (low.parse::<f64>(), high.parse::<f64>()) {
        return a > b;
    }

    match (parse_instant(low), parse_instant(high)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Seconds since the Unix epoch for the date/timestamp forms a filter bound
/// usually holds. `Date.parse` in React accepts far more; a value the browser
/// would date-parse but this parser leaves alone simply gets no warning.
fn parse_instant(s: &str) -> Option<i64> {
    let (date, time) = match s.split_once(['T', ' ']) {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    let mut parts = date.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (mut hour, mut minute, mut second) = (0, 0, 0);
    if let Some(time) = time {
        let mut hms = time.split(':');
        hour = hms.next()?.parse::<i64>().ok()?;
        minute = hms.next()?.parse::<i64>().ok()?;
        if let Some(sec) = hms.next() {
            second = sec.parse::<i64>().ok()?;
        }
        if hms.next().is_some()
            || !(0..24).contains(&hour)
            || !(0..60).contains(&minute)
            || !(0..60).contains(&second)
        {
            return None;
        }
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Howard Hinnant's `days_from_civil`, the proleptic Gregorian day count.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
