//! Port of `packages/frontend/src/windows/onboarding/onboardingGate.ts`
//! (T268): the first-run journey's gate — the pure decision whether the
//! onboarding wizard should run, plus the versioned settings payload its
//! exit writes. A logic port: no elements, no keys, no i18n.
//!
//! The React file sits between two hosts. The backend owns the
//! fresh-install-vs-upgrade decision (`Store::load_all`): a data dir that
//! has never persisted `settings.json` is a fresh install and materializes
//! `onboarding = { completed: false }` → journey shown; an installation
//! whose `settings.json` predates the journey is an upgrade and resolves to
//! `{ completed: true }` → journey hidden. The desktop bootstrap turns that
//! state into the window pair (the `?window=onboarding` wizard window vs
//! the main window — the two never coexist, the T072 seam). So the UI side
//! only has to render the journey when an explicitly unfinished state
//! arrives; a missing state (`undefined`/`null`) means "upgrading user,
//! never onboarded before" and must NOT show the journey — gating on
//! `completed` alone would re-onboard every existing user once.
//!
//! ## What maps where
//!
//! - `ONBOARDING_VERSION` stays the exported const ([`ONBOARDING_VERSION`],
//!   the `state` submodule) — bump it (and the backend's mirror in
//!   `store/settings.rs`) only to re-onboard every user.
//! - The `OnboardingState` interface is [`state::OnboardingState`] — a
//!   serde mirror of the shared settings contract's `onboarding` key (the
//!   backend domain type `crates/datazen/src/settings/domain.rs`'s
//!   camelCase `{ "completed", "version" }`), so the host can hand the
//!   loaded settings' state straight to the gate and serialize the exit
//!   payload back out verbatim. The UI crate keeps its own copy (it does
//!   not depend on the backend crate); `state`'s tests pin the JSON shape.
//! - `shouldShowOnboarding(state?)` is [`gate::should_show_onboarding`] —
//!   React's `undefined` and `null` both arrive as `None`.
//! - `completedOnboardingState()` is
//!   [`state::completed_onboarding_state`]: the payload the journey's
//!   single exit (the done step's open handler — finishing AND skipping
//!   both leave through S3's one button) merges via
//!   `updateSettings({ onboarding: … })` before the window's completion
//!   action fires. That write is the host's persistence seam (no IPC in
//!   the UI crate): the T114 wizard documents it as the `on_open`
//!   feed-in — the host persists this payload, then fires
//!   `invoke('onboarding_complete')` (the stash → persist → complete
//!   order).
//!
//! ## Consumers / feeds
//!
//! - `wizard_state.rs` (T323) documents that the versioned
//!   `{ completed, version }` payload is THIS file's — the done screen's
//!   summary reads the wizard state, never this one.
//! - `onboarding_wizard.rs` (T114) / `onboarding_window.rs` (T072) hold
//!   the window open/close seams: the desktop bootstrap decides which
//!   window to create, and the wizard's exit persists through here.
//!
//! ## Known deltas (documented, not silent)
//!
//! - `version: number` → `i32` (the shared settings contract's type).
//! - React's `undefined` vs `null` distinction collapses into `None` (the
//!   gate treats both identically — `!state`).
//! - `state.completed !== true` is `!state.completed`: Rust's `bool` is
//!   already strict, so the JS "not strictly `true`" edge (a truthy
//!   non-boolean) has no counterpart to preserve.
//! - `Default` derives `{ completed: false, version: 0 }` (the backend
//!   domain type's default, for serde-driven loads); React has no default
//!   shape.

// The onboarding window host (the gate feed and the exit-persistence seam
// wire there) is the production consumer; until then this crate's tests
// are the only caller, so the dead-code lint is silenced here rather than
// crate-wide (the onboarding_wizard.rs precedent).
#![allow(dead_code)]

// The gate decision (React `shouldShowOnboarding`).
pub(crate) mod gate;
// The versioned payload + revision const (React `OnboardingState` /
// `ONBOARDING_VERSION` / `completedOnboardingState`).
pub(crate) mod state;

#[allow(unused_imports)] // the onboarding window host (the exit-persistence seam) is the production consumer
pub(crate) use gate::should_show_onboarding;
#[allow(unused_imports)] // the onboarding window host (the exit-persistence seam) is the production consumer
pub(crate) use state::{OnboardingState, ONBOARDING_VERSION, completed_onboarding_state};
