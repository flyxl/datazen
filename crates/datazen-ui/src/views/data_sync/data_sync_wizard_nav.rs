//! T287: `windows/data-sync/DataSyncWizardNav.tsx` + `DataSyncWizardStepper.tsx`
//! ported to Rune.
//!
//! Back / next navigation of the Data Sync wizard (the back button with
//! its chevron, the next button with its optional busy spinner — hidden
//! entirely on the steps that navigate with the execute bar instead), and
//! the wizard's progress stepper (the six step chips with their chevron
//! separators, the current step accented, past steps at 80%, future steps
//! muted).
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | nav root (internal) | `data-sync-wizard-nav` |
//! | back button (React test id) | `data-sync-back` |
//! | next button (React test id) | `data-sync-next` |
//! | busy spinner (internal) | `data-sync-next-busy` |
//! | stepper root (internal) | `data-sync-wizard-stepper` |
//! | the centered chip row (internal) | `…-row` |
//! | one step chip (React test id) | `data-sync-step-<step>` |
//! | the chip's number bubble (internal) | `data-sync-step-<step>-num` |
//! | the chevron before chip i>0 (internal) | `data-sync-step-sep-<i>` |
//!
//! ## Deviations
//!
//! * `max-w-4xl mx-auto` — the stepper row centers with an 896pt cap (the
//!   960pt minimum still fits).
//! * `bg-accent text-on-accent` / `bg-accent/20` / `bg-surface-raised` —
//!   the kit accent fill with the crate's `palette::on_accent` text, the
//!   accent at 20% alpha, and the raised surface — the established
//!   mappings, no new literals.
//! * React puts the chevrons inside the buttons' children; the kit button
//!   carries them through its own icon slot.

// The production consumer is the Data Sync window track (T268); until it
// wires the pair in, the component tests are the only callers.
#![allow(dead_code)] // reusable components; wired by the Data Sync wizard (T268)

use std::rc::Rc;

use rune::{col, icon, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT};
use crate::ui::kit::kit;
use crate::ui::palette;
use crate::ui::spinner::{Spinner, SpinnerSize};
use crate::views::data_sync::use_data_sync_wizard_state::{WizardStep, WIZARD_STEPS};

/// The back/next navigation.
pub(crate) struct DataSyncWizardNav<S: 'static> {
    step_index: usize,
    busy: bool,
    can_next: bool,
    show_next: bool,
    on_back: Option<Rc<dyn Fn(&mut S)>>,
    on_next: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> DataSyncWizardNav<S> {
    pub(crate) fn new(step_index: usize, busy: bool, can_next: bool, show_next: bool) -> Self {
        Self {
            step_index,
            busy,
            can_next,
            show_next,
            on_back: None,
            on_next: None,
        }
    }

    /// `onBack`.
    pub(crate) fn on_back(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_back = Some(Rc::new(f));
        self
    }

    /// `onNext`.
    pub(crate) fn on_next(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_next = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut nav = row()
            .key("data-sync-wizard-nav")
            .w_full()
            .items_center()
            .justify_between()
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            .px(24.0)
            .py(12.0);

        // Back: disabled on the first step or while busy.
        let mut back = Button::new(t("sync.back"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .icon(&ICON_CHEVRON_LEFT)
            .disabled(self.step_index == 0 || self.busy);
        if let Some(callback) = self.on_back.clone() {
            back = back.on_press(move |s: &mut S| callback(s));
        }
        nav = nav.child(back.build(&k).key("data-sync-back"));

        // Next (hidden on the steps that navigate with the execute bar):
        // the busy spinner rides inside the row beside the button.
        if self.show_next {
            let mut next = Button::new(t("sync.next"))
                .control_size(ControlSize::Regular)
                .icon(&ICON_CHEVRON_RIGHT)
                .disabled(!self.can_next || self.busy);
            if let Some(callback) = self.on_next.clone() {
                next = next.on_press(move |s: &mut S| callback(s));
            }
            let mut next_row = row()
                .key("data-sync-next-row")
                .items_center()
                .gap(4.0);
            if self.busy {
                next_row = next_row.child(
                    Spinner::new("data-sync-next-busy")
                        .size(SpinnerSize::Lg)
                        .build(dark),
                );
            }
            next_row = next_row.child(next.build(&k).key("data-sync-next"));
            nav = nav.child(next_row);
        }
        nav
    }
}

/// The progress stepper: one chip per wizard step.
pub(crate) struct DataSyncWizardStepper {
    step_index: usize,
}

impl DataSyncWizardStepper {
    pub(crate) fn new(step_index: usize) -> Self {
        Self { step_index }
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut chips = Vec::new();
        for (index, step) in WIZARD_STEPS.iter().enumerate() {
            if index > 0 {
                chips.push(
                    icon(&ICON_CHEVRON_RIGHT, 12.0)
                        .key(format!("data-sync-step-sep-{index}"))
                        .color(k.c.text_muted),
                );
            }
            chips.push(self.chip(&k, dark, index, step));
        }
        col()
            .key("data-sync-wizard-stepper")
            .w_full()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .px(24.0)
            .py(12.0)
            .child(
                row()
                    .key("data-sync-wizard-stepper-row")
                    .w_full()
                    .max_w(896.0)
                    .items_center()
                    .justify_center()
                    .gap(4.0)
                    .children(chips),
            )
    }

    /// One step chip: the number bubble and the word, in the
    /// current/past/future color triple.
    fn chip(&self, k: &Kit, dark: bool, index: usize, step: &WizardStep) -> El {
        let name = step.as_str();
        // i === stepIndex → accent; i < stepIndex → 80% accent; else muted.
        let (word_color, bubble_fill, bubble_text) = if index == self.step_index {
            (k.c.text_accent, k.c.text_accent, palette::on_accent(dark))
        } else if index < self.step_index {
            (
                k.alpha(k.c.text_accent, 0.8),
                k.alpha(k.c.text_accent, 0.2),
                k.c.text_accent,
            )
        } else {
            (k.c.text_muted, k.c.surface_weak, k.c.text_muted)
        };
        let key = format!("data-sync-step-{name}");
        row()
            .key(key.clone())
            .items_center()
            .gap(6.0)
            .radius(999.0)
            .px(8.0)
            .py(2.0)
            .child(
                row()
                    .key(format!("{key}-num"))
                    .w(20.0)
                    .h(20.0)
                    .items_center()
                    .justify_center()
                    .radius(999.0)
                    .fill(bubble_fill)
                    .child(
                        text((index + 1).to_string())
                            .key(format!("{key}-num-text"))
                            .font(k.ty.text_xsmall)
                            .color(bubble_text),
                    ),
            )
            .child(
                text(t(&format!("sync.step.{name}")))
                    .key(format!("{key}-label"))
                    .font(k.ty.text_xsmall)
                    .color(word_color),
            )
    }
}
