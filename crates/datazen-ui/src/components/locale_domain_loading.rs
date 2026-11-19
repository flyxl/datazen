//! `components/LocaleDomainLoading.tsx` (T140): the loading skeleton
//! shown while a lazy i18n domain is being imported.
//!
//! It renders before the feature body can safely call `t()` for that
//! domain, so it must NOT depend on any (possibly not-yet-loaded) lazy
//! translation key: the only text is the hardcoded local fallback
//! (`Loading…`), exactly as in React.
//!
//! ## Key convention
//!
//! | part      | key              |
//! |-----------|------------------|
//! | root      | `<testId>` (the caller's prop) |
//! | spinner   | `<testId>-spinner` (the Spinner base appends `-indicator`) |
//! | fallback  | `<testId>-fallback` |
//!
//! ## Deliberate boundaries
//!
//! * `aria-busy`/`aria-label="Loading"` become the root's `role="status"`
//!   with the `Loading` accessible name (`label`); the busy shape itself
//!   is the skeleton.
//! * `variant="page"` is `h-screen` → `h_full` of the window; `section`
//!   is `min-h-[8rem]` → a bounded 128pt region that never blanks the
//!   whole page.
//! * React's `Spinner size="xl"` is the 20pt tier of the ported sizes
//!   (`Sm 12 / Md 16 / Lg 20`), so the Rust call is `SpinnerSize::Lg`.

use rune::{row, text, El};

use crate::ui::spinner::{Spinner, SpinnerSize};

/// The two fill modes (React's `variant`): the whole window, or a
/// bounded in-page region.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LocaleDomainLoadingVariant {
    /// `page` (default): fill the whole window height — the full-screen
    /// feature windows (Data Sync / Transfer / Schema Diff / Workflow).
    #[default]
    Page,
    /// `section`: a bounded-height region — in-page panels (Settings
    /// MCP, Dashboard), so it never blanks the whole page.
    Section,
}

/// The loading skeleton.
pub(crate) struct LocaleDomainLoading {
    key: &'static str,
    variant: LocaleDomainLoadingVariant,
}

impl LocaleDomainLoading {
    /// `testId` becomes the root key.
    pub(crate) fn new(key: &'static str) -> Self {
        Self {
            key,
            variant: LocaleDomainLoadingVariant::Page,
        }
    }

    /// `variant` (`page` default).
    pub(crate) fn variant(mut self, variant: LocaleDomainLoadingVariant) -> Self {
        self.variant = variant;
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let mut root = row()
            .key(self.key)
            .role("status")
            .label("Loading")
            .w_full()
            .items_center()
            .justify_center()
            .gap(8.0);
        root = match self.variant {
            LocaleDomainLoadingVariant::Page => root.h_full(),
            LocaleDomainLoadingVariant::Section => root.min_h(128.0),
        };
        root.child(
            Spinner::new(format!("{}-spinner", self.key))
                .size(SpinnerSize::Lg)
                .build(dark),
        )
        .child(
            text("Loading…")
                .key(format!("{}-fallback", self.key))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        )
    }
}
