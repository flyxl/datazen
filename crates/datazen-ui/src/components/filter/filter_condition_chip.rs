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

use rune::{row, text, Color, El};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant};

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
            let shown = if value.is_empty() || value == "NULL" {
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
    pub(crate) fn on_remove<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.remove = Some(Box::new(move |e: El| e.on_press(f)));
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
            .max_w(480.0);
        if let Some(act) = self.edit {
            if !self.disabled {
                edit = act(edit);
            }
        }

        let mut remove = IconButton::new(&ICON_X)
            .variant(IconButtonVariant::GhostMuted)
            .control_size(ControlSize::Mini)
            .disabled(self.disabled)
            .label(remove_label)
            .build(k)
            .key(format!("{}-remove", self.key));
        if let Some(act) = self.remove {
            if !self.disabled {
                remove = act(remove);
            }
        }

        let mut chip = row()
            .key(self.key)
            .gap(2.0)
            .center_y()
            .radius(999.0)
            .pl(8.0)
            .pr(2.0)
            .py(2.0)
            .max_w(560.0)
            .fill(Color::TRANSPARENT)
            .child(edit);
        chip = chip.child(remove);
        // The accent wash and hairline sit behind the two buttons.
        chip.fill(k.alpha(k.c.bg_accent, 0.10))
            .border(1.0, k.alpha(k.c.bg_accent, 0.30))
    }
}
