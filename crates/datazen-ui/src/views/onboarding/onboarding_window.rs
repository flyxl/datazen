//! Port of `packages/frontend/src/windows/onboarding/OnboardingWindow.tsx`
//! (T072): the standalone onboarding window's shell.
//!
//! React renders this inside a dedicated Tauri window (`?window=onboarding`,
//! selected by `App.tsx`'s window-kind switch and created by the desktop
//! bootstrap when onboarding has not yet been completed). The file's whole
//! body is one completion handler — `await invoke('onboarding_complete')`,
//! the Rust command that creates the main window and closes the wizard
//! window — wrapped around the wizard:
//! `<OnboardingWizard onComplete={handleComplete} />`.
//!
//! ## What maps where
//!
//! - `invoke('onboarding_complete')` is a host command, not UI-crate
//!   business: it arrives as the [`OnboardingWindow::on_complete`] action
//!   slot (React's `handleComplete`). Nothing in the shell itself fires it
//!   — React's wizard calls `onComplete` from its done-step open handler
//!   after settings have been persisted — so the window retains it for the
//!   wizard seam: [`OnboardingWindow::completion_action`] is the getter the
//!   future wizard host wiring (T114) reads to hand the ported wizard its
//!   `onComplete` prop.
//! - The wizard itself (`OnboardingWizard.tsx`, T114) is NOT ported here.
//!   The seam: React's wizard owns the root row, the `BrandSidebar` and
//!   the `<main>` column; this shell hoists exactly that chrome so the
//!   standalone window exists before T114 lands, and the wizard arrives
//!   as the host-fed [`OnboardingWindow::wizard`] slot — the children of
//!   React's `<main>` (the padded step area plus the `WizardFooter`) as
//!   one host-built `El`. Until the host supplies it the main column
//!   renders empty (the sidebar-only window) — the slot paints nothing,
//!   the T069 convention.
//! - `BrandSidebar` (T021) is composed verbatim; the host-delivered app
//!   version rides [`OnboardingWindow::version`] (React's sidebar fetches
//!   it through the platform port's `hostSettings.getAppVersion()`;
//!   `None` keeps BrandSidebar's `useState('…')` em-dash default).
//! - React's root classes `flex h-screen bg-surface text-fg` → a full-size
//!   row (`w_full` + `h_full`, the window root) filled with the kit's
//!   `bg_base` (`bg-surface`); `text-fg` is inherited ink — Rune texts
//!   carry their own colours, so no counterpart. The `<main>` classes
//!   `flex flex-1 flex-col overflow-hidden` → `grow(1.0)` + `min_h(0.0)` /
//!   `min_w(0.0)` (overflow-hidden's automatic-minimum-size rule; the
//!   T024 mapping — no hard clip, so kit popups the slot mounts are not
//!   cut, exactly as React's own popups portal out).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; this subtree's derived keys
//! sit under the one `onboarding-window-` prefix:
//!
//! | part | key |
//! |------|-----|
//! | root row (React: the wizard root's testid, absorbed by this shell) | `onboarding-wizard` |
//! | the `<main>` column (no React testid) | `onboarding-window-main` |
//! | brand sidebar family | T021's `onboarding-sidebar-*` keys (see `brand_sidebar.rs`) |
//!
//! The wizard slot's contents carry T114's own keys and are not re-keyed
//! here. The window is not yet wired into any `Section` dispatch — React
//! roots it through `App.tsx`'s `?window=onboarding` switch, so the Rune
//! host will root it directly once the wizard track (T114) lands.

use std::rc::Rc;

use rune::{col, row, El};

use super::brand_sidebar::BrandSidebar;
use crate::ui::kit::kit;

/// The window's action slots (`Rc<dyn Fn(&mut S)>`, the crate convention).
type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// The standalone onboarding window shell: the wizard chrome (root row +
/// brand sidebar + main column) around a host-fed wizard slot, carrying
/// the completion action React wires to `invoke('onboarding_complete')`.
#[allow(dead_code)] // the wizard host wiring (T114) is the production consumer
pub(crate) struct OnboardingWindow<S: 'static> {
    /// The host-delivered app version for the brand sidebar's footer.
    version: Option<String>,
    /// The T114 seam: the wizard — the children of React's `<main>`.
    wizard: Option<El>,
    /// React `handleComplete` (`invoke('onboarding_complete')`), retained
    /// for the wizard seam (nothing in the shell fires it).
    on_complete: Option<ActionFn<S>>,
}

impl<S: 'static> Default for OnboardingWindow<S> {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)] // the wizard host wiring (T114) is the production consumer
impl<S: 'static> OnboardingWindow<S> {
    /// React's start: no wizard slot yet (it paints nothing) and the
    /// sidebar's em-dash version placeholder.
    #[allow(dead_code)] // the wizard host wiring (T114) is the production consumer
    pub(crate) fn new() -> Self {
        Self { version: None, wizard: None, on_complete: None }
    }

    /// The sidebar's app version (React: the platform port's
    /// `getAppVersion()`; `None` keeps BrandSidebar's `'…'` default).
    #[allow(dead_code)] // the wizard host wiring (T114) is the production consumer
    pub(crate) fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// The wizard slot (T114): the children of React's `<main>` — the
    /// padded step area plus the `WizardFooter` — as one host-built `El`.
    #[allow(dead_code)] // the wizard host wiring (T114) is the production consumer
    pub(crate) fn wizard(mut self, wizard: El) -> Self {
        self.wizard = Some(wizard);
        self
    }

    /// React `handleComplete`: the completion IPC action the wizard calls
    /// after settings have been persisted (the desktop host then creates
    /// the main window and closes the wizard window).
    #[allow(dead_code)] // the wizard host wiring (T114) is the production consumer
    pub(crate) fn on_complete(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_complete = Some(Rc::new(f));
        self
    }

    /// The completion action for the wizard seam: what the T114 host hands
    /// the ported wizard as its `onComplete` prop.
    #[allow(dead_code)] // the wizard host wiring (T114) is the production consumer
    pub(crate) fn completion_action(&self) -> Option<ActionFn<S>> {
        self.on_complete.clone()
    }

    /// Paint the window. `dark` drives the kit tokens (the sidebar's
    /// gradient anchor stays fixed-dark, as in React).
    #[allow(dead_code)] // the wizard host wiring (T114) is the production consumer
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);

        let mut sidebar = BrandSidebar::new();
        if let Some(version) = &self.version {
            sidebar = sidebar.version(version.clone());
        }

        // React <main className="flex flex-1 flex-col overflow-hidden">:
        // the wizard slot's column. The slot paints nothing until the
        // host supplies it.
        let mut main = col()
            .key("onboarding-window-main")
            .grow(1.0) // flex-1
            .min_h(0.0) // overflow-hidden: the column yields before it clips
            .min_w(0.0);
        if let Some(wizard) = self.wizard {
            main = main.child(wizard);
        }

        // React's wizard root: `flex h-screen bg-surface text-fg` with the
        // `onboarding-wizard` testid (verbatim — this shell absorbs the
        // wizard's root row until T114 fills the slot).
        row()
            .key("onboarding-wizard")
            .w_full()
            .h_full() // h-screen: the window root fills the OS window
            .fill(k.c.bg_base) // bg-surface
            .child(sidebar.build(dark))
            .child(main)
    }
}
