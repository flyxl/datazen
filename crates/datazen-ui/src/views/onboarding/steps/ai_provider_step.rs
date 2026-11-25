//! Port of `packages/frontend/src/windows/onboarding/steps/AiProviderStep.tsx`
//! (T148): S2 — step 2 of 2 of the first-run journey, always the second
//! step whichever entry S0 was started from.
//!
//! The React file is deliberately thin: the form itself is the settings'
//! `ModelProfileForm` (`windows/settings/ai/ModelProfileForm.tsx`, T107)
//! shared with Settings → AI → Add model — same provider/protocol/key/
//! endpoint/model fields, same validate, same safety gate — mounted in its
//! `inline` variant. Only the heading, the security note and the primary
//! action are journey-specific, and the primary action (the journey
//! footer's Finish) is the wizard's, which is exactly why the form runs
//! `inline` (no Cancel/Save row of its own).
//!
//! ## What maps where
//!
//! - React's `draft` prop is the wizard's own `useModelProfileDraft(...)`
//!   instance (`OnboardingWizard.tsx:50` — `profile: null`, `onSave` marks
//!   the saved profile `isDefault: true`, `onSaved` dispatches
//!   `FINISH { aiLabel: profile.name }`). The port's seam is the SAME
//!   [`ModelProfileFormSlots`] the settings form uses (the draft half plus
//!   the `useAiStore` half), so the host wires one draft machine
//!   (`use_model_profile_draft.rs`, T479) into both consumers and the
//!   journey can never drift from Settings → AI — the T107/T479 contract.
//!   `aiDraft.hasCredentials` / `aiDraft.savingState` feed the wizard's
//!   own seams (`onboarding_wizard.rs`'s `has_credentials` / `busy`, and
//!   its `finish_path` branch), not this step.
//! - `useEffect(() => { void loadProviders(); }, [loadProviders])` is the
//!   host's provider load when the journey reaches step 2 (no IPC in the
//!   UI crate): the loaded list arrives through the `providers` slot and
//!   the draft's `fill_provider_defaults` runs host-side on its arrival
//!   (T479's resync rule).
//! - `<ModelProfileForm draft={draft} variant="inline" />` is
//!   [`ModelProfileForm::variant`] with [`FormVariant::Inline`] — the T107
//!   inline variant with its reserved `onboarding-ai-form` root key, built
//!   over the same slots; the high-risk egress confirmation pushes into
//!   the caller's `layer`, exactly as in the settings dialog.
//! - `<StepTag label={t('onboarding.s2.stepLabel')} />` is the shared
//!   chrome ([`super::step_tag`], the hoisted `WelcomeStep.tsx` export).
//! - The four i18n reads (`onboarding.s2.title` / `subtitle` /
//!   `securityNote` / `optionalHint`) are `crate::i18n::t` with the same
//!   keys.
//!
//! ## Keys
//!
//! React's one `data-testid` stays verbatim; this step's other parts derive
//! under the one `onboarding-step-s2-ai-` prefix (the shared tag carries
//! its own `onboarding-step-tag-` prefix, the form its T107 table):
//!
//! | part | key |
//! |------|-----|
//! | root (`data-testid="onboarding-step-s2-ai"`) | `onboarding-step-s2-ai` |
//! | heading `h2` | `onboarding-step-s2-ai-title` |
//! | subtitle `p` | `onboarding-step-s2-ai-subtitle` |
//! | the form card (`rounded-xl border border-edge bg-surface p-5`) | `onboarding-step-s2-ai-card` |
//! | the inline form (T107's reserved root) | `onboarding-ai-form` |
//! | security note / its Info glyph / its text | `onboarding-step-s2-ai-security-note`, `-icon`, `-text` |
//! | the optional hint `p` | `onboarding-step-s2-ai-optional-hint` |
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75); `max-w-[56ch]` on the subtitle becomes 283.5pt (the `ch` unit
//! is the "0" advance, ≈ 0.5em at React's 13.5px → 6.75px, so 56ch ≈
//! 378px — approximate by construction); `rounded-xl` is the kit's `lg`
//! token (the `workspace_default_cards.rs` mapping); `text-fg-secondary`
//! and `text-fg-muted` both map to the kit's one `text_muted` token (the
//! T015/T029 ruling); `items-start` on the security note is the row's
//! default top alignment.

// The onboarding wizard host (T114's `ai` seat) is the production consumer;
// until the window track wires it, this crate's audit is the only caller.
#![allow(dead_code)]

use rune::{col, icon, row, text, El, TextStyle};

use crate::i18n::t;
use crate::icons::ICON_INFO;
use crate::ui::kit::kit;
use crate::views::settings::ai::model_profile_form::{
    FormVariant, ModelProfileForm, ModelProfileFormSlots,
};

use super::step_tag::step_tag;

/// React `max-w-[680px]` (680px → 510pt): the step column's cap.
pub const STEP_MAX_W: f32 = 510.0;

/// React `max-w-[56ch]` on the subtitle: 56 × the "0" advance (≈ 0.5em at
/// 13.5px → 6.75px) ≈ 378px → 283.5pt. Approximate by construction — the
/// `ch` unit has no Rune counterpart (see the deltas above).
const SUBTITLE_MAX_W: f32 = 283.5;

/// The journey's second step: the shared model-profile form (inline
/// variant) inside the step's own first-run framing. Built per frame over
/// the host's `S` — the same slot list the settings form reads, so the
/// wizard's one draft machine drives both.
pub struct AiProviderStep<S: 'static> {
    slots: ModelProfileFormSlots<S>,
}

impl<S: 'static> AiProviderStep<S> {
    /// The step over the wizard's draft-machine slots (React's `draft`
    /// prop — the `useModelProfileDraft` return value the wizard owns).
    pub fn new(slots: ModelProfileFormSlots<S>) -> Self {
        Self { slots }
    }

    /// One appearance of the step: the tag, the heading, the subtitle, the
    /// bordered card carrying the inline form, the security note and the
    /// optional hint. The high-risk egress confirmation (while the draft's
    /// `show_high_risk_modal` is set) pushes into `layer`, as in the
    /// settings dialog.
    pub fn build(&self, s: &S, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let form = ModelProfileForm::new(self.slots)
            .variant(FormVariant::Inline)
            .build(s, dark, layer);

        col()
            .key("onboarding-step-s2-ai") // React data-testid, verbatim
            .w_full()
            .max_w(STEP_MAX_W)
            // <StepTag label={t('onboarding.s2.stepLabel')} />
            .child(step_tag(&k, t("onboarding.s2.stepLabel")))
            // The heading: mb-2.5 text-[25px] font-bold tracking-tight
            // leading-tight text-fg.
            .child(
                text(t("onboarding.s2.title").to_owned())
                    .key("onboarding-step-s2-ai-title")
                    .font(TextStyle::new(18.75, 700.0).tracking(-0.46875).lh(23.4375))
                    .mb(7.5)
                    .color(k.c.text_base),
            )
            // The subtitle: mb-6 max-w-[56ch] text-[13.5px] leading-[1.65]
            // text-fg-secondary.
            .child(
                text(t("onboarding.s2.subtitle").to_owned())
                    .key("onboarding-step-s2-ai-subtitle")
                    .font(TextStyle::new(10.125, 400.0).lh(16.7))
                    .max_w(SUBTITLE_MAX_W)
                    .mb(18.0)
                    .color(k.c.text_muted),
            )
            // The form card: rounded-xl border border-edge bg-surface p-5,
            // carrying the shared form in its inline variant (the journey
            // footer's Finish owns the primary action).
            .child(
                col()
                    .key("onboarding-step-s2-ai-card")
                    .w_full()
                    .radius(k.radius.lg)
                    .border(0.75, k.c.border_base)
                    .fill(k.c.surface_base)
                    .px(15.0)
                    .py(15.0)
                    .child(form),
            )
            // The security note: mt-5 flex items-start gap-2.5
            // rounded-[10px] border border-accent/16 bg-accent/5 p-3.5
            // text-[12.5px] leading-[1.6] text-fg-secondary.
            .child(
                row()
                    .key("onboarding-step-s2-ai-security-note")
                    .w_full()
                    .mt(15.0)
                    .gap(7.5)
                    .radius(7.5)
                    .border(0.75, k.c.bg_accent.alpha(0.16))
                    .fill(k.c.bg_accent.alpha(0.05))
                    .px(10.5)
                    .py(10.5)
                    .child(
                        // <Info className="mt-0.5 h-[15px] w-[15px]
                        // shrink-0 text-accent" /> — items-start keeps the
                        // glyph on the note's first line; `shrink-0` is a
                        // FIXED box (the flex `shrink()` here once let the
                        // text squeeze it to 0 width).
                        icon(&ICON_INFO, 11.25)
                            .key("onboarding-step-s2-ai-security-note-icon")
                            .w(11.25)
                            .mt(1.5)
                            .color(k.c.text_accent),
                    )
                    .child(
                        text(t("onboarding.s2.securityNote").to_owned())
                            .key("onboarding-step-s2-ai-security-note-text")
                            .font(TextStyle::new(9.375, 400.0).lh(15.0))
                            .color(k.c.text_muted),
                    ),
            )
            // The optional hint: mt-3 text-[12.5px] text-fg-muted.
            .child(
                text(t("onboarding.s2.optionalHint").to_owned())
                    .key("onboarding-step-s2-ai-optional-hint")
                    .font(TextStyle::new(9.375, 400.0))
                    .mt(9.0)
                    .color(k.c.text_muted),
            )
    }
}
