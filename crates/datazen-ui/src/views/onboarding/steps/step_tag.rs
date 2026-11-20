//! The onboarding steps' shared chrome: the `StepTag` export of
//! `packages/frontend/src/windows/onboarding/steps/WelcomeStep.tsx`
//! (T148 — hoisted into its own module because `AiProviderStep.tsx`
//! imports it from `WelcomeStep.tsx` while `WelcomeStep` itself is the
//! later task T247; see `steps/mod.rs`).
//!
//! React renders the tag as a pill-labelled eyebrow above every step's
//! heading (`AiProviderStep.tsx` and, later, the other steps' own
//! `<StepTag label={t('onboarding.sN.stepLabel')} />` calls).
//!
//! ## What maps where
//!
//! - `<StepTag label={…} />` is one presentational function: the label
//!   arrives as a parameter (the caller's `t("…")` string), so no state and
//!   no effects live here.
//! - The `uppercase` Tailwind class becomes `str::to_uppercase` on the
//!   label (the `column_mapping_editor.rs` ruling — a no-op for the zh-CN
//!   values, the same rendering React's class gives them).
//!
//! ## Keys
//!
//! React renders no `data-testid`; the derived keys sit under the one
//! `onboarding-step-tag-` prefix:
//!
//! | part | key |
//! |------|-----|
//! | the `mb-4 flex items-center gap-2` row | `onboarding-step-tag` |
//! | the pill `span` | `onboarding-step-tag-pill` |
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75); `rounded-full` is a 999pt radius (the paint pipeline clamps to
//! half the shorter side); `bg-accent/12` / `border-accent/20` are the kit
//! accent at those alphas (the `brand_sidebar.rs` ruling); `inline-block`
//! inside the flex row is `shrink`.

use rune::{row, text, El, TextStyle};
use rune_kit::Kit;

/// The pill label's type (React `text-[11px] font-semibold uppercase
/// tracking-wider`): 11px → 8.25pt, weight 600, +0.05em tracking.
fn tag_text_style() -> TextStyle {
    TextStyle::new(8.25, 600.0).tracking(8.25 * 0.05)
}

/// One `StepTag`: the accent pill label above a step's heading. Purely
/// presentational — the label is the caller's translated step label.
pub(crate) fn step_tag(k: &Kit, label: &str) -> El {
    row()
        .key("onboarding-step-tag")
        .mb(12.0) // React mb-4 (16px)
        .gap(6.0) // React gap-2 (8px)
        .center_y() // items-center
        .child(
            // The pill span: the `badge_pill` pattern (a shrinking row with
            // padding around one text), rounded-full via the 999pt radius.
            row()
                .key("onboarding-step-tag-pill")
                .shrink() // inline-block inside the row
                .radius(999.0) // rounded-full (clamped to half the height)
                .px(7.5) // React px-2.5 (10px)
                .py(3.0) // React py-1 (4px)
                .center_y()
                .fill(k.c.bg_accent.alpha(0.12)) // bg-accent/12
                .border(0.75, k.c.bg_accent.alpha(0.20)) // border border-accent/20
                .child(
                    // `uppercase` at render time (see the mapping above).
                    text(label.to_uppercase())
                        .font(tag_text_style())
                        .color(k.c.text_accent), // text-accent
                ),
        )
}
