//! Rune ports of `packages/frontend/src/windows/onboarding/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `BrandSidebar.tsx` | `brand_sidebar` | T021 |
//! | `OnboardingWindow.tsx` | `onboarding_window` | T072 |
//! | `onboardingLayout.ts` | `onboarding_layout` | T287 |

pub(crate) mod brand_sidebar;
pub(crate) mod onboarding_layout;
pub(crate) mod onboarding_window;

#[allow(unused_imports)] // the onboarding wizard track (T114) is the production consumer
pub(crate) use brand_sidebar::{BrandSidebar, version_label};
#[allow(unused_imports)] // the onboarding wizard track (T114) is the production consumer
pub(crate) use onboarding_layout::{
    WIZARD_ASIDE_PX, WIZARD_FOOTER_HEIGHT, WIZARD_FOOTER_PX, chrome_text,
};
#[allow(unused_imports)] // the onboarding wizard track (T114) is the production consumer
pub(crate) use onboarding_window::OnboardingWindow;
