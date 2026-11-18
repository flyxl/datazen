//! A collapsed filter condition: “name = alice” with an edit and a remove
//! button.
//!
//! Ported 1:1 from `packages/frontend/src/components/filter/FilterConditionChip.tsx`.
//! The chip is where the user reads the filter most of the time, so it carries
//! the summary text and the per-condition connector badge lives next to it
//! (`FilterConnectorToggle`).
//!
//! ## Key convention
//!
//! | part  | key            |
//! |-------|----------------|
//! | root  | `<key>`        |
//! | edit  | `<key>-edit`   |
//! | remove| `<key>-remove` |
//!
//! A condition longer than the chip truncates (React `max-w-full truncate`).

// The filter panel view (T024) is the production consumer; until it lands the
// component tests are the only callers, so the dead-code lint is silenced here
// rather than in a crate-wide allow.
#![allow(dead_code)]

use rune::{icon, row, text, Color, El, Key, KeyEvent};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::ICON_X;
use crate::ui::kit::kit;

use super::filter_connector_toggle::FilterConnector;

/// The value of a filter condition, shaped like the React operator's value
/// control: none, one literal, a pair (`between`), or a token list (`in`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FilterValue {
    None,
    Single(String),
    Pair(String, String),
    Tokens(Vec<String>),
}

/// A filter condition in the shape the chip renders. `operator` is the React
/// operator id (`eq`, `between`, …); its label is `filter.<operator>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FilterCondition {
    pub column: String,
    pub operator: &'static str,
    pub value: FilterValue,
    pub connector: Option<FilterConnector>,
}

impl FilterCondition {
    pub(crate) fn new(
        column: impl Into<String>,
        operator: &'static str,
        value: FilterValue,
    ) -> Self {
        FilterCondition {
            column: column.into(),
            operator,
            value,
            connector: None,
        }
    }

    pub(crate) fn connector(mut self, connector: Option<FilterConnector>) -> Self {
        self.connector = connector;
        self
    }
}

/// The text of one condition, e.g. `name = alice` (React `conditionSummary`).
pub(crate) fn condition_summary(filter: &FilterCondition) -> String {
    let key = format!("filter.{}", filter.operator);
    let op = t(&key);
    match &filter.value {
        FilterValue::None => format!("{} {}", filter.column, op),
        FilterValue::Single(value) => {
            // React only elides an empty value; the literal text `NULL` is a value
            // the user typed and stays visible.
            let shown = if value.is_empty() {
                "…".to_owned()
            } else {
                value.clone()
            };
            format!("{} {} {}", filter.column, op, shown)
        }
        FilterValue::Pair(lower, upper) => {
            format!("{} {} {}, {}", filter.column, op, lower, upper)
        }
        FilterValue::Tokens(tokens) => {
            format!("{} {} {}", filter.column, op, tokens.join(", "))
        }
    }
}

pub(crate) struct FilterConditionChip {
    key: String,
    condition: FilterCondition,
    disabled: bool,
    edit: Option<Box<dyn FnOnce(El) -> El>>,
    remove: Option<Box<dyn FnOnce(El) -> El>>,
}

impl FilterConditionChip {
    pub(crate) fn new(key: impl Into<String>, condition: FilterCondition) -> FilterConditionChip {
        FilterConditionChip {
            key: key.into(),
            condition,
            disabled: false,
            edit: None,
            remove: None,
        }
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Called when the summary is pressed (React `onEdit`).
    pub(crate) fn on_edit<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.edit = Some(Box::new(move |e: El| e.on_press(f)));
        self
    }

    /// Called when the remove button is pressed (React `onRemove`).
    ///
    /// React's native `<button>` also fires on Enter; Rune presses a focused
    /// button with Space only (Return passes buttons by), so the row carries an
    /// explicit Enter handler to keep the two activations at parity.
    pub(crate) fn on_remove<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.remove = Some(Box::new(move |e: El| {
            let key_f = f.clone();
            e.on_press(f)
                .on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut rune::Tx| {
                    if ev.key == Key::Enter {
                        key_f(s);
                        true
                    } else {
                        false
                    }
                })
        }));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let label = condition_summary(&self.condition);
        let accent = k.c.text_accent;
        let edit_label = label.clone();
        let remove_label = t("filter.remove");

        let mut edit = Button::new(label)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(self.disabled)
            .help(t("filter.editCondition"))
            .label_with(move |font, _ink| {
                text(edit_label.clone()).font(font).color(accent).truncate()
            })
            .build(k)
            .key(format!("{}-edit", self.key))
            .shrink()
            // Rune's El has no fit-content or percent max-width, so the chip cannot
            // clamp itself to the filter column; cap the summary so the pill still
            // fits the 360pt narrow column (React truncates at the column edge).
            .max_w(300.0);
        if let Some(act) = self.edit {
            if !self.disabled {
                edit = act(edit);
            }
        }

        // React paints the X in `text-accent/80`; the kit's GhostMuted variant would
        // use the muted icon ink, and IconButton has no ink override in this Rune
        // version, so the row carries the accent ink on its icon child.
        let mut remove = row()
            .role("button")
            .size(16.0, 16.0)
            .center()
            .radius(k.radius.sm)
            .label(remove_label)
            .help(remove_label)
            .child(
                icon(&ICON_X, 12.0)
                    .key("icon")
                    .color(k.alpha(k.c.text_accent, 0.8))
                    .a11y_hidden(),
            )
            .focusable(!self.disabled)
            .key(format!("{}-remove", self.key));
        if self.disabled {
            remove = remove.disabled(true).layer_opacity(0.5);
        }
        if let Some(act) = self.remove {
            if !self.disabled {
                remove = act(remove);
            }
        }

        let mut chip = row()
            .key(self.key)
            .self_start()
            .min_w(0.0)
            .gap(2.0)
            .center_y()
            .radius(999.0)
            .pl(8.0)
            .pr(2.0)
            .py(2.0)
            .max_w(560.0)
            .disabled(self.disabled)
            .fill(Color::TRANSPARENT)
            .child(edit);
        chip = chip.child(remove);
        // The accent wash and hairline sit behind the two buttons.
        chip.fill(k.alpha(k.c.bg_accent, 0.10))
            .border(1.0, k.alpha(k.c.bg_accent, 0.30))
    }
}
