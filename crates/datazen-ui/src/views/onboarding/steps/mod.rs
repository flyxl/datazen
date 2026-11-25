//! Rune ports of `packages/frontend/src/windows/onboarding/steps/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `AiProviderStep.tsx` | `ai_provider_step` | T148 |
//! | `WelcomeStep.tsx`'s `StepTag` export | `step_tag` | T148 (hoisted; T247 reuses) |
//! | `WelcomeStep.tsx` | `welcome_step` | T247 |
//! | `ManualConnectionStep.tsx` | `manual_connection_step` | T203 |
//! | `SampleStep.tsx` | `sample_step` | T226 |
//! | `ImportStep.tsx` | `import_step` | T177 |
//! | `DoneStep.tsx` | `done_step` | T177 |
//!
//! `StepTag` is defined inside `WelcomeStep.tsx` and exported for the other
//! steps (`AiProviderStep.tsx` imports it from there); the port hoists it
//! into its own module so the first step port (T148) could mount it before
//! `WelcomeStep` (T247) landed — the `onboarding_layout` absorb pattern.
//! T247 reuses [`step_tag`] instead of re-defining it.

pub mod ai_provider_step;
pub mod done_step;
pub mod import_step;
pub mod manual_connection_step;
pub mod sample_step;
pub mod step_tag;
pub mod welcome_step;

// The wizard host wiring (the remaining step tasks T177–T247) is the
// production consumer.
#[allow(unused_imports)] // the onboarding wizard host (T114's ai seat) is the production consumer
pub use ai_provider_step::AiProviderStep;
#[allow(unused_imports)] // the WelcomeStep port (T247) is the production consumer
pub use step_tag::step_tag;
#[allow(unused_imports)] // the onboarding wizard host (T114's welcome seat) is the production consumer
pub use welcome_step::WelcomeStep;
#[allow(unused_imports)] // the onboarding wizard host (T114's `-step-s1-manual` seat) is the production consumer
pub use manual_connection_step::ManualConnectionStep;
#[allow(unused_imports)] // the onboarding wizard host (T114's `-step-s1-sample` seat) is the production consumer
pub use sample_step::SampleStep;
#[allow(unused_imports)] // the onboarding wizard host (T114's `-step-s1-import` seat) is the production consumer
pub use import_step::ImportStep;
#[allow(unused_imports)] // the onboarding wizard host (T114's `-step-s3` seat) is the production consumer
pub use done_step::DoneStep;
