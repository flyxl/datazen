//! The connector of one condition: how it joins the condition before it.
//!
//! Ported 1:1 from `packages/frontend/src/components/filter/FilterConnectorToggle.tsx`.
//!
//! Two states of the same control:
//!
//! - **inherited** — the row has no connector of its own and follows the
//!   request wide logic. The badge shows that value and says so in its title.
//! - **explicit** — the row carries its own connector. Clicking a value makes
//!   the row explicit, which is what takes the request wide logic out of the
//!   picture.
//!
//! ## Key convention
//!
//! | part | key            |
//! |------|----------------|
//! | root | `<key>`        |
//! | AND  | `<key>-and`    |
//! | OR   | `<key>-or`     |
//!
//! ## Deviation
//!
//! React takes one `onSelect(connector)` callback; the Rune builder takes
//! `.on_and(..)` / `.on_or(..)` because a Rune press handler receives only the
//! state, not a per-button payload. The two callbacks are the same API surface
//! with the payload baked in.

// The filter panel view (T024) is the production consumer; until it lands the
// component tests are the only callers, so the dead-code lint is silenced here
// rather than in a crate-wide allow.
#![allow(dead_code)]

use rune::{row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::kit::kit;

/// The connector in effect for one row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FilterConnector {
    And,
    Or,
}

impl FilterConnector {
    pub(crate) fn label(self) -> &'static str {
        match self {
            FilterConnector::And => t("filter.and"),
            FilterConnector::Or => t("filter.or"),
        }
    }
}

pub(crate) struct FilterConnectorToggle {
    key: String,
    index: usize,
    value: FilterConnector,
    explicit: bool,
    differs_from_global: bool,
    disabled: bool,
    on_and: Option<Box<dyn FnOnce(El) -> El>>,
    on_or: Option<Box<dyn FnOnce(El) -> El>>,
}

impl FilterConnectorToggle {
    /// `index` is the row's `data-filter-index` in React; it is kept for the
    /// caller's bookkeeping and does not change the drawing.
    pub(crate) fn new(
        key: impl Into<String>,
        index: usize,
        value: FilterConnector,
    ) -> FilterConnectorToggle {
        FilterConnectorToggle {
            key: key.into(),
            index,
            value,
            explicit: false,
            differs_from_global: false,
            disabled: false,
            on_and: None,
            on_or: None,
        }
    }

    /// Whether the row carries its own connector (React `explicit`).
    pub(crate) fn explicit(mut self, explicit: bool) -> Self {
        self.explicit = explicit;
        self
    }

    /// Whether the effective connector differs from the request wide logic.
    pub(crate) fn differs_from_global(mut self, differs: bool) -> Self {
        self.differs_from_global = differs;
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Called when `AND` is pressed.
    pub(crate) fn on_and<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_and = Some(Box::new(move |e: El| e.on_press(f)));
        self
    }

    /// Called when `OR` is pressed.
    pub(crate) fn on_or<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_or = Some(Box::new(move |e: El| e.on_press(f)));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let accent = k.c.text_accent;
        let muted = k.c.text_muted;
        let inherited_help = (!self.explicit).then(|| t("filter.connectorInherited").to_owned());

        let mut root = row()
            .key(self.key.clone())
            .center_y()
            .radius(k.radius.sm)
            .border(
                1.0,
                if self.differs_from_global {
                    k.alpha(accent, 0.60)
                } else {
                    k.c.border_base
                },
            )
            .fill(if self.differs_from_global {
                k.alpha(k.c.bg_accent, 0.10)
            } else {
                rune::Color::TRANSPARENT
            });
        if !self.explicit {
            root = root.opacity(0.7);
        }
        if let Some(help) = inherited_help {
            root = root.help(help);
        }

        let and_active = self.value == FilterConnector::And;
        let or_active = self.value == FilterConnector::Or;
        let mut and = Button::new(t("filter.and"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(self.disabled)
            .label_with(move |font, _ink| {
                text(t("filter.and"))
                    .font(font)
                    .color(if and_active { accent } else { muted })
            })
            .build(k)
            .key(format!("{}-and", self.key))
            .fill(if and_active {
                k.alpha(accent, 0.15)
            } else {
                rune::Color::TRANSPARENT
            });
        if let Some(act) = self.on_and {
            if !self.disabled {
                and = act(and);
            }
        }

        let mut or = Button::new(t("filter.or"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(self.disabled)
            .label_with(move |font, _ink| {
                text(t("filter.or"))
                    .font(font)
                    .color(if or_active { accent } else { muted })
            })
            .build(k)
            .key(format!("{}-or", self.key))
            .fill(if or_active {
                k.alpha(accent, 0.15)
            } else {
                rune::Color::TRANSPARENT
            });
        if let Some(act) = self.on_or {
            if !self.disabled {
                or = act(or);
            }
        }

        // `index` is part of React's data attributes only; keeping the field
        // read avoids a dead-code annotation while preserving the API.
        let _ = self.index;
        root.child(and).child(or)
    }
}
