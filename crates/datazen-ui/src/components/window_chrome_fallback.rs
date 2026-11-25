//! `components/WindowChromeFallback.tsx` (T301): the chrome overlay
//! fallback.
//!
//! Overlay title-bar windows have no native drag area until the app
//! mounts. Use as Suspense/error chrome so the window can still be
//! moved and closed.
//!
//! ## Input boundary
//!
//! Form-split F2: React renders the **registered chrome slot** (the
//! desktop app registers its `TitleBar` at assembly time through
//! `useWindowChrome` from `@datazen/extension-points`) instead of
//! importing a host component — shared code never names a chrome entity
//! (ruling #7). The Rust port keeps the slot shape: the caller hands
//! the registered chrome as a built [`El`] ([`WindowChromeFallback::chrome`]);
//! an unregistered slot (or a web build without one) passes `None` and
//! renders no bar.
//!
//! ## Key convention
//!
//! | part | key |
//! |---|---|
//! | root | `window-chrome-fallback` |
//! | the spinner | `chrome-fallback-spinner` (the Spinner base appends `-indicator`/`-label`) |
//!
//! ## Deliberate boundaries
//!
//! * React's `Spinner variant="ring" size="2xl" tone="accent"` — the
//!   Rune Spinner base ends at the 20pt tier (`Lg`, React's `xl`), so
//!   this port takes the largest tier as `2xl`'s counterpart (React's
//!   `2xl` is 24pt); the size ladder extension lands with the spinner
//!   base's own follow-up, not here.
//! * A bare spinner is the only thing on screen here, so it is the one
//!   place in the app that has to announce itself: the label is the
//!   literal `Loading`, not a `t()` call, for the same reason as
//!   `LocaleDomainLoading` — this renders before the i18n domain can be
//!   trusted.

use rune::{col, row, El};

use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone, SpinnerVariant};

/// The fallback chrome.
pub struct WindowChromeFallback {
    chrome: Option<El>,
}

impl WindowChromeFallback {
    pub fn new() -> Self {
        Self { chrome: None }
    }

    /// The registered chrome slot (React's `useWindowChrome()` — the
    /// desktop TitleBar, registered at assembly time). `None` renders no
    /// bar.
    pub fn chrome(mut self, chrome: Option<El>) -> Self {
        self.chrome = chrome;
        self
    }

    pub fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let mut root = col()
            .key("window-chrome-fallback")
            .w_full()
            .h_full()
            .min_h(0.0)
            .fill(k.c.surface_base)
            .items_stretch();
        if let Some(chrome) = self.chrome {
            root = root.child(chrome);
        }
        root.child(
            row()
                .key("chrome-fallback-center")
                .grow(1.0)
                .min_h(0.0)
                .items_center()
                .justify_center()
                .child(
                    Spinner::new("chrome-fallback-spinner")
                        .variant(SpinnerVariant::Ring)
                        .size(SpinnerSize::Lg)
                        .tone(SpinnerTone::Accent)
                        .label("Loading")
                        .build(dark),
                ),
        )
    }
}
