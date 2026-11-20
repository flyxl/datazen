//! Port of `packages/frontend/src/windows/onboarding/WizardFooter.tsx`
//! (T247): the wizard's global footer — the step indicator (two step dots
//! plus the step label) on the left, skip/back/continue/finish on the right.
//!
//! React's own doc comment states the file's one layout rule: the footer
//! shares its fixed height with the brand sidebar's version row so both
//! lines of chrome text sit on the same bottom line — the height is
//! `WIZARD_FOOTER_HEIGHT_CLASS` from `onboardingLayout.ts` (T287's
//! [`WIZARD_FOOTER_HEIGHT`], never hard-coded here).
//!
//! ## What maps where
//!
//! - The component is stateless: every prop arrives per frame. `step` /
//!   `canContinue` / `showContinue` / `showFinish` / `busy` are the
//!   [`WizardFooter`] fields (fed by [`crate::views::onboarding::onboarding_wizard`]'s
//!   `footer_wiring()`, which documents the exact React prop list —
//!   [`WizardFooter::from_wiring`] reads it directly), and the four action
//!   props are `Rc` action slots (`onSkip`/`onBack`/`onContinue`/`onFinish`).
//! - `stepIndexFor` / `stepLabelKey` (`wizardState.ts`, absorbed by
//!   `wizard_state.rs` — T323) reduce
//!   here to [`visible_step_number`]: `showSteps` is "a visible step number
//!   exists" (S0 entry and S3 done hide the indicator — React's `invisible`
//!   keeps it laid out, so the port paints it at `opacity(0.0)` with
//!   `a11y_hidden`, matching `aria-hidden`), the dots compare against the
//!   same number, and the label key is `onboarding.s2.stepLabel` on S2 else
//!   `onboarding.s1.stepLabel` (the label only renders while visible).
//! - The buttons: React's `Button` (`@datazen/ui`) primary variant is the
//!   kit `Contrast` skin and secondary is `Outline` (the T018/T022 track
//!   convention), both at React's default md size (`h-9`), the kit's
//!   `ExtraLarge` control — the mapping the ported dialog footers use.
//!   `disabled={!canContinue}` / `disabled={busy}` are the kit's
//!   `disabled` (Continue/Finish); Back and Skip never disable.
//! - The skip button is NOT React's `Button` — it is a raw `<button>` with
//!   custom metrics (`px-1.5 py-2.5 text-[12px]`, the 3px X glyph, muted
//!   ink) — so the port hand-rolls it (`role("button")` + `focusable` +
//!   `on_press`, the `workspace_default_cards.rs` precedent for raw React
//!   buttons).
//! - React's own visibility guards are ported verbatim even though the
//!   wizard shell only mounts the footer at S0–S2: skip hides at S3, back
//!   hides at S0 and S3, and the step indicator hides whenever there is no
//!   visible step number.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; the footer's other parts derive
//! under the one `onboarding-footer-` prefix:
//!
//! | part | key |
//! |------|-----|
//! | footer root (`data-testid="onboarding-footer"`) | `onboarding-footer` |
//! | step indicator (`data-testid="onboarding-step-indicator"`) | `onboarding-step-indicator` |
//! | step dot *i* (React `key={i}`, i ∈ 1..=2) | `onboarding-footer-dot-{i}` |
//! | step label (`data-testid="onboarding-step-label"`) | `onboarding-step-label` |
//! | the `ml-auto` spacer | `onboarding-footer-spacer` |
//! | actions row | `onboarding-footer-actions` |
//! | skip (`data-testid="onboarding-skip"`) | `onboarding-skip` |
//! | skip's X glyph | `onboarding-footer-skip-icon` |
//! | back (`data-testid="onboarding-back"`) | `onboarding-back` |
//! | continue (`data-testid="onboarding-continue"`) | `onboarding-continue` |
//! | finish (`data-testid="onboarding-finish"`) | `onboarding-finish` |
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75); `transition-all` on the dots has no counterpart (the sizes are
//! static per step); the skip button's `hover:text-fg-secondary` ink hover
//! has no Rune counterpart (`El::hover` is a fill hover) — the glyph and
//! label keep their rest ink; `text-left` is a row default; the kit button
//! chrome (radius/padding) follows the kit's ExtraLarge metrics, not
//! Tailwind's `rounded-[9px] px-[18px]`.

// The onboarding wizard host (the T114 shell's `footer` seat) is the
// production consumer; until the window track wires it, this crate's audit
// is the only caller.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{icon, row, text, El, TextStyle};

use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::ICON_X;
use crate::ui::kit::kit;

use super::onboarding_layout::{chrome_text, WIZARD_FOOTER_HEIGHT, WIZARD_FOOTER_PX};
use super::onboarding_wizard::{visible_step_number, FooterWiring, Step};
// The T323 absorb: the label-key helper lives in `wizard_state.rs` now.
use super::wizard_state::step_label_key;

/// The footer's action slots (`Rc<dyn Fn(&mut S)>`, the crate convention).
type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// The wizard's global footer: step dots + label left, actions right.
/// Stateless — every prop arrives per frame (React's are per render).
pub(crate) struct WizardFooter<S: 'static> {
    /// `step`.
    step: Step,
    /// `canContinue` — step 1 completeness (the T323 machine's output).
    can_continue: bool,
    /// `showContinue` — step 1 of a non-manual entry.
    show_continue: bool,
    /// `showFinish` — S2.
    show_finish: bool,
    /// `busy` (React default `false`) — `aiDraft.savingState`.
    busy: bool,
    /// `onSkip`.
    on_skip: Option<ActionFn<S>>,
    /// `onBack`.
    on_back: Option<ActionFn<S>>,
    /// `onContinue`.
    on_continue: Option<ActionFn<S>>,
    /// `onFinish`.
    on_finish: Option<ActionFn<S>>,
}

impl<S: 'static> WizardFooter<S> {
    /// The footer at `step` with React's prop defaults: nothing shown, not
    /// busy, no actions wired.
    pub(crate) fn new(step: Step) -> Self {
        Self {
            step,
            can_continue: false,
            show_continue: false,
            show_finish: false,
            busy: false,
            on_skip: None,
            on_back: None,
            on_continue: None,
            on_finish: None,
        }
    }

    /// The footer over the wizard's computed prop list — exactly the props
    /// React's `OnboardingWizard` hands `<WizardFooter … />` (the T114
    /// shell's `footer_wiring()` output).
    pub(crate) fn from_wiring(wiring: FooterWiring<S>) -> Self {
        Self {
            step: wiring.step,
            can_continue: wiring.can_continue,
            show_continue: wiring.show_continue,
            show_finish: wiring.show_finish,
            busy: wiring.busy,
            on_skip: wiring.on_skip,
            on_back: wiring.on_back,
            on_continue: wiring.on_continue,
            on_finish: wiring.on_finish,
        }
    }

    /// `canContinue`.
    pub(crate) fn can_continue(mut self, can_continue: bool) -> Self {
        self.can_continue = can_continue;
        self
    }

    /// `showContinue`.
    pub(crate) fn show_continue(mut self, show_continue: bool) -> Self {
        self.show_continue = show_continue;
        self
    }

    /// `showFinish`.
    pub(crate) fn show_finish(mut self, show_finish: bool) -> Self {
        self.show_finish = show_finish;
        self
    }

    /// `busy` (React default `false`).
    pub(crate) fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    /// `onSkip`.
    pub(crate) fn on_skip(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_skip = Some(Rc::new(f));
        self
    }

    /// `onBack`.
    pub(crate) fn on_back(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_back = Some(Rc::new(f));
        self
    }

    /// `onContinue`.
    pub(crate) fn on_continue(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_continue = Some(Rc::new(f));
        self
    }

    /// `onFinish`.
    pub(crate) fn on_finish(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_finish = Some(Rc::new(f));
        self
    }

    /// Paint the footer. `dark` drives the kit tokens; the shared chrome
    /// height keeps the sidebar's version row and this step indicator on
    /// the same bottom line (T287's one rule).
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let WizardFooter {
            step,
            can_continue,
            show_continue,
            show_finish,
            busy,
            on_skip,
            on_back,
            on_continue,
            on_finish,
        } = self;

        // `stepIndexFor(step)` — 0 unless S1/S2; the indicator is hidden
        // (still laid out) whenever no step number is visible.
        let step_index = visible_step_number(step).unwrap_or(0);
        let show_steps = step_index > 0;

        // React: `flex items-center gap-2` + `invisible` — visibility:hidden
        // keeps the geometry, so the port paints the subtree at opacity 0
        // and marks it `aria-hidden` exactly when React does.
        let mut indicator = row()
            .key("onboarding-step-indicator") // React data-testid, verbatim
            .gap(6.0) // gap-2
            .center_y(); // items-center
        for i in [1u8, 2u8] {
            indicator = indicator.child(step_dot(&k, dark, i, step_index));
        }
        if !show_steps {
            indicator = indicator.opacity(0.0).a11y_hidden(); // invisible + aria-hidden
        }
        let indicator = indicator.child(
            // `ml-1` + WIZARD_CHROME_TEXT_CLASS: the shared small mono ink,
            // its muted half applied here (T287's split).
            text(t(step_label_key(step)).to_owned())
                .key("onboarding-step-label") // React data-testid, verbatim
                .font(chrome_text())
                .ml(3.0) // ml-1
                .color(k.c.text_muted),
        );

        // React `ml-auto` on the actions row: the free space between the
        // indicator and the buttons (the `data_table_selection_bar`
        // precedent — a growing spacer).
        let spacer = rune::el().key("onboarding-footer-spacer").grow(1.0);

        let mut actions = row().key("onboarding-footer-actions").gap(7.5).center_y(); // gap-2.5

        // Skip — every step except S3 (React's own guard; the wizard shell
        // additionally never mounts the footer at S3).
        if step != Step::S3 {
            actions = actions.child(skip_button(&k, on_skip));
        }
        // Back — S1 and S2 only (`step !== 's0' && step !== 's3'`).
        if step != Step::S0 && step != Step::S3 {
            let mut back = Button::new(t("onboarding.common.back").to_owned())
                .variant(ButtonVariant::Outline) // React secondary (T022)
                .control_size(ControlSize::ExtraLarge); // React md (h-9)
            if let Some(on_back) = on_back {
                back = back.on_press(move |s: &mut S| on_back(s));
            }
            actions = actions.child(back.build(&k).key("onboarding-back"));
        }
        // Continue — step 1 of a non-manual entry, gated by completeness.
        if show_continue {
            let mut continue_ = Button::new(t("onboarding.common.continue").to_owned())
                .variant(ButtonVariant::Contrast) // React primary (T018)
                .control_size(ControlSize::ExtraLarge) // React md (h-9)
                .disabled(!can_continue);
            if let Some(on_continue) = on_continue {
                continue_ = continue_.on_press(move |s: &mut S| on_continue(s));
            }
            actions = actions.child(continue_.build(&k).key("onboarding-continue"));
        }
        // Finish — S2, gated by the AI draft's saving state.
        if show_finish {
            let mut finish = Button::new(t("onboarding.common.finish").to_owned())
                .variant(ButtonVariant::Contrast) // React primary (T018)
                .control_size(ControlSize::ExtraLarge) // React md (h-9)
                .disabled(busy);
            if let Some(on_finish) = on_finish {
                finish = finish.on_press(move |s: &mut S| on_finish(s));
            }
            actions = actions.child(finish.build(&k).key("onboarding-finish"));
        }

        // React <footer>: `flex shrink-0 items-center gap-4 border-t
        // border-edge bg-surface` + the shared height and footer padding.
        row()
            .key("onboarding-footer") // React data-testid, verbatim
            .w_full()
            .shrink() // shrink-0
            .h(WIZARD_FOOTER_HEIGHT) // WIZARD_FOOTER_HEIGHT_CLASS (h-[56px])
            .center_y() // items-center
            .gap(16.0) // gap-4
            .px(WIZARD_FOOTER_PX) // WIZARD_FOOTER_PX_CLASS (px-[48px])
            .border_sides([0.75, 0.0, 0.0, 0.0], k.c.border_base) // border-t border-edge
            .fill(k.c.surface_base) // bg-surface
            .child(indicator)
            .child(spacer)
            .child(actions)
    }
}

/// One step dot (`[1, 2].map(...)`): the current step's dot is the wide
/// accent bar (`w-5 bg-accent`), a passed step's the faded accent pin
/// (`w-[7px] bg-accent/40`), a future step's the edge-coloured pin
/// (`w-[7px] bg-edge`) — every dot `h-[7px]` and `rounded-full`.
fn step_dot(k: &Kit, dark: bool, i: u8, step_index: u8) -> El {
    let (width, fill) = if step_index == i {
        (15.0, k.c.bg_accent) // w-5 bg-accent
    } else if step_index > i {
        (5.25, k.c.bg_accent.alpha(0.40)) // w-[7px] bg-accent/40
    } else {
        // React's `bg-edge` is the OPAQUE theme token, not the kit's
        // translucent border ink.
        (5.25, crate::ui::palette::edge_solid(dark)) // w-[7px] bg-edge
    };
    rune::el()
        .key(format!("onboarding-footer-dot-{i}"))
        .size(width, 5.25) // w-* h-[7px]
        .radius(999.0) // rounded-full
        .fill(fill)
}

/// React's raw skip `<button>`: `inline-flex items-center gap-1.5
/// border-none bg-transparent px-1.5 py-2.5 text-[12px] text-fg-muted` with
/// the 3px X glyph — not a `Button`, so the port hand-rolls it.
fn skip_button<S: 'static>(k: &Kit, on_skip: Option<ActionFn<S>>) -> El {
    let mut skip = row()
        .key("onboarding-skip") // React data-testid, verbatim
        .gap(4.5) // gap-1.5
        .center_y() // items-center
        .px(4.5) // px-1.5
        .py(7.5) // py-2.5
        .shrink() // the row never yields to the spacer
        // <X className="h-3 w-3" /> in the button's muted ink.
        .child(icon(&ICON_X, 9.0).key("onboarding-footer-skip-icon").color(k.c.text_muted))
        .child(
            text(t("onboarding.common.skip").to_owned())
                .font(TextStyle::new(9.0, 400.0)) // text-[12px]
                .color(k.c.text_muted), // text-fg-muted
        )
        // React: `<button type="button">` — a raw button.
        .role("button")
        .focusable(true);
    if let Some(on_skip) = on_skip {
        skip = skip.on_press(move |s: &mut S| on_skip(s));
    }
    skip
}
