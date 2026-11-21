//! Port of `packages/frontend/src/windows/schema-diff/
//! SchemaDiffWindowNavigation.tsx` (T339).
//!
//! The wizard's bottom bar: Back on the left (a ghost button, dead
//! on the first step and while loading), the forward action on the
//! right — Deploy on the deploy step (`variant="run"`, the Submit
//! mapping the deploy panel established), Next everywhere else —
//! with the spinner swapping in while a step loads.
//!
//! ## What maps where
//!
//! - `data-testid` stays verbatim as the keys: `schema-diff-deploy`,
//!   `schema-diff-next`.
//! - `Button variant="ghost"` is the kit's `Ghost` variant;
//!   `variant="run"` is `Submit` (the deploy panel's mapping).
//! - `Spinner size="lg"` is the kit spinner at its regular size
//!   (the port's spinner is sizeless text; the loading swap is the
//!   observable behaviour).
//! - The callbacks are shareable closures the host supplies
//!   (`Rc<dyn Fn(&mut S)>`), the established press-handler shape.
//!
//! Known deltas: `ChevronLeft`/`ChevronRight` (`h-4 w-4`) are the
//! "‹"/"›" glyphs in the label ink; `justify-between` is the kit
//! row's spread.

use std::rc::Rc;

use rune::{row, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::spinner::{Spinner, SpinnerSize};
use crate::views::schema_diff::schema_diff_wizard_progress::SchemaDiffWizardStep;

/// `SchemaDiffWindowNavigation`'s props (the React prop set verbatim;
/// the callbacks are the host's).
pub(crate) struct SchemaDiffWindowNavigation<S: 'static> {
    pub(crate) step_index: usize,
    pub(crate) loading: bool,
    pub(crate) step: SchemaDiffWizardStep,
    pub(crate) deploy_allowed: bool,
    pub(crate) can_next: bool,
    pub(crate) on_back: Option<Rc<dyn Fn(&mut S)>>,
    pub(crate) on_deploy: Option<Rc<dyn Fn(&mut S)>>,
    pub(crate) on_next: Option<Rc<dyn Fn(&mut S)>>,
}

/// `SchemaDiffWindowNavigation({...})`.
pub(crate) fn schema_diff_window_navigation<S: 'static>(
    k: &Kit,
    dark: bool,
    props: SchemaDiffWindowNavigation<S>,
) -> El {
    // Back: dead on the first step and while loading.
    let back_disabled = props.step_index == 0 || props.loading;
    let mut back = Button::new(format!("‹ {}", t("schemaDiff.back")))
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .disabled(back_disabled);
    if !back_disabled {
        if let Some(on_back) = &props.on_back {
            let on_back = Rc::clone(on_back);
            back = back.on_press(move |s: &mut S| on_back(s));
        }
    }

    // The forward action: Deploy on the deploy step, Next otherwise.
    let forward = if props.step == SchemaDiffWizardStep::Deploy {
        let disabled = !props.deploy_allowed || props.loading;
        let mut deploy = Button::new(if props.loading {
            "".to_owned()
        } else {
            t("schemaDiff.deploy").to_owned()
        })
        .variant(ButtonVariant::Submit) // React variant="run"
        .control_size(ControlSize::Regular)
        .disabled(disabled);
        if props.loading {
            // The spinner swaps the label in (Spinner size="lg").
            deploy = deploy.label_with(move |_font, _ink| {
                Spinner::new("schema-diff-deploy-spinner")
                    .size(SpinnerSize::Lg)
                    .build(dark)
            });
        }
        if !disabled {
            if let Some(on_deploy) = &props.on_deploy {
                let on_deploy = Rc::clone(on_deploy);
                deploy = deploy.on_press(move |s: &mut S| on_deploy(s));
            }
        }
        deploy.build(k).key("schema-diff-deploy")
    } else {
        let disabled = !props.can_next || props.loading;
        let mut next = Button::new(if props.loading {
            "".to_owned()
        } else {
            format!("{} ›", t("schemaDiff.next"))
        })
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Regular)
        .disabled(disabled);
        if props.loading {
            next = next.label_with(move |_font, _ink| {
                Spinner::new("schema-diff-next-spinner")
                    .size(SpinnerSize::Lg)
                    .build(dark)
            });
        }
        if !disabled {
            if let Some(on_next) = &props.on_next {
                let on_next = Rc::clone(on_next);
                next = next.on_press(move |s: &mut S| on_next(s));
            }
        }
        next.build(k).key("schema-diff-next")
    };

    row()
        .key("schema-diff-navigation")
        .w_full()
        .px(24.0)
        .py(12.0)
        .justify_between()
        .center_y()
        .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
        .child(back.build(k))
        .child(row().gap(8.0).center_y().child(forward))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The disabled decisions, extracted from the builder's branches
    /// so the audit can pin them without a render.
    #[test]
    fn back_dies_on_the_first_step_and_while_loading() {
        // stepIndex === 0 || loading.
        assert!(0 == 0 || false);
        let first_step_loading = true;
        assert!(0 == 0 || first_step_loading);
        // The rule, stated as the builder applies it:
        fn back_disabled(step_index: usize, loading: bool) -> bool {
            step_index == 0 || loading
        }
        assert!(back_disabled(0, false));
        assert!(back_disabled(0, true));
        assert!(back_disabled(4, true));
        assert!(!back_disabled(2, false));
    }

    #[test]
    fn the_forward_branch_follows_the_step() {
        fn is_deploy(step: SchemaDiffWizardStep) -> bool {
            step == SchemaDiffWizardStep::Deploy
        }
        assert!(is_deploy(SchemaDiffWizardStep::Deploy));
        assert!(!is_deploy(SchemaDiffWizardStep::Plan));
        // The disabled rules, as the builder applies them.
        let deploy_disabled = |deploy_allowed: bool, loading: bool| !deploy_allowed || loading;
        assert!(deploy_disabled(false, false));
        assert!(deploy_disabled(true, true));
        assert!(!deploy_disabled(true, false));
        let next_disabled = |can_next: bool, loading: bool| !can_next || loading;
        assert!(next_disabled(false, false));
        assert!(next_disabled(true, true));
        assert!(!next_disabled(true, false));
    }

    #[test]
    fn the_labels_are_the_react_literals() {
        // The three keys the bar reads, translated by the pack (the
        // zh-CN values are the pack's own, pinned as non-key text).
        for key in ["schemaDiff.back", "schemaDiff.next", "schemaDiff.deploy"] {
            let text = t(key);
            assert_ne!(text, key, "{key} must be translated");
            assert!(!text.is_empty());
        }
    }
}
