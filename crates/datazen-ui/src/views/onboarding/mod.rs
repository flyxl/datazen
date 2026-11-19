//! Rune ports of `packages/frontend/src/windows/onboarding/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `BrandSidebar.tsx` | `brand_sidebar` | T021 |
//! | `OnboardingWindow.tsx` | `onboarding_window` | T072 |
//! | `OnboardingWizard.tsx` | `onboarding_wizard` | T114 |
//! | `onboardingLayout.ts` | `onboarding_layout` | T287 |

pub(crate) mod brand_sidebar;
pub(crate) mod onboarding_layout;
pub(crate) mod onboarding_window;
pub(crate) mod onboarding_wizard;

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
