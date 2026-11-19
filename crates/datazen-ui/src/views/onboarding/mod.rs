//! Rune ports of `packages/frontend/src/windows/onboarding/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `BrandSidebar.tsx` | `brand_sidebar` | T021 |
//! | `OnboardingWindow.tsx` | `onboarding_window` | T072 |
//! | `OnboardingWizard.tsx` | `onboarding_wizard` | T114 |
//! | `onboardingLayout.ts` | `onboarding_layout` | T287 |
//! | `wizardState.ts` | `wizard_state` | T323 |
//! | `steps/AiProviderStep.tsx` (+ `WelcomeStep.tsx`'s `StepTag` export) | `steps` | T148 |
//! | `WizardFooter.tsx` | `wizard_footer` | T247 |
//! | `steps/WelcomeStep.tsx` | `steps::welcome_step` | T247 |
//! | `useDetectedImportSource.ts` | `use_detected_import_source` | T306 |

pub(crate) mod brand_sidebar;
pub(crate) mod onboarding_layout;
pub(crate) mod onboarding_window;
pub(crate) mod onboarding_wizard;
// T148/T247: the journey's step components and the global footer (the
// steps/ directory and WizardFooter.tsx).
pub(crate) mod steps;
pub(crate) mod wizard_footer;
// T306: the detected-import-source hook (the import entry's config
// pre-check — the probe order/hits, the enabled effect, the cancelled
// round guard; absorbs the ImportApp seam type).
pub(crate) mod use_detected_import_source;
// T323: the wizard's state machine (wizardState.ts) — the step/entry seam
// types and summary cores absorb here.
pub(crate) mod wizard_state;

#[allow(unused_imports)] // the wizard host wiring (the step/footer tasks) is the production consumer
pub(crate) use brand_sidebar::{BrandSidebar, version_label};
#[allow(unused_imports)] // the wizard host wiring (the step/footer tasks) is the production consumer
pub(crate) use onboarding_layout::{
    WIZARD_ASIDE_PX, WIZARD_FOOTER_HEIGHT, WIZARD_FOOTER_PX, chrome_text,
};
#[allow(unused_imports)] // the wizard host wiring (the step/footer tasks) is the production consumer
pub(crate) use onboarding_window::OnboardingWindow;
#[allow(unused_imports)] // the onboarding step/footer/state-machine tasks (T148–T323) are the consumers
pub(crate) use onboarding_wizard::{
    Entry, FinishPath, ImportApp, ImportSource, OnboardingWizard, PostOnboardingSampleVm,
    SAMPLE_PRESET_SQL, Step, visible_step_number,
};
#[allow(unused_imports)] // the onboarding wizard host (T114's footer seat) is the production consumer
pub(crate) use wizard_footer::WizardFooter;
#[allow(unused_imports)] // the onboarding window host (the wizard's detection wiring) is the production consumer
pub(crate) use use_detected_import_source::{
    DetectedImportSource, IMPORT_APP_ORDER, ImportProbe,
};
