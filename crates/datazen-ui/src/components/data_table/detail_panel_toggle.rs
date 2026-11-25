//! T257: the detail-panel toggle, ported 1:1 from
//! `packages/frontend/src/components/DataTable/DetailPanelToggle.tsx`.
//!
//! ```text
//! <button type="button" aria-pressed={open} title={…}
//!   class="inline-flex h-7 w-7 items-center justify-center rounded-md
//!          transition-colors …">
//!   {open ? <PanelRightClose class="h-4 w-4"/> : <PanelRightOpen class="h-4 w-4"/>}
//! </button>
//! ```
//!
//! The 7-point square beside the grid header that hides and shows the
//! row-detail drawer (T236's `detail_panel.rs`): open, it paints the
//! accent wash (`bg-accent/15 text-accent`) and offers the collapse
//! chevron; closed, it is the quiet muted button that offers the
//! expand one. The icon pair is `panel-right-close` / `panel-right-open`
//! (the divider moves the way the chevron points), newly registered in
//! `icons.rs` from the lucide sources.
//!
//! ## Key convention
//!
//! | part   | key        |
//! |--------|------------|
//! | button | `<key>`    |
//!
//! ## Deliberate boundaries
//!
//! * React's `title` is the button's only text, so it is the accessible
//!   name here ([`El::label`]) — the tooltip and the name are one string,
//!   `detail.show` / `detail.hide`, as the locale pack carries them.
//! * `aria-pressed` is [`El::checked`] (the toggle-button state channel).
//! * `transition-colors` has no Rune counterpart in a static frame; the
//!   open/closed colours are their own settled values, as React's are
//!   after the 150 ms fade.
//! * The closed state's `hover:text-fg` is a text colour Rune's hover
//!   does not carry ([`El::hover`] is a fill); the affordance is the
//!   `surface-raised` hover fill itself, and the muted icon keeps its
//!   colour. Documented rather than approximated.

use rune::{icon, row, El};

use crate::icons::{ICON_PANEL_RIGHT_CLOSE, ICON_PANEL_RIGHT_OPEN};
use crate::i18n::t;
use crate::ui::kit::kit;

/// The 7-point (`h-7 w-7`) toggle button.
#[allow(dead_code)] // ported base; remove when the grid header call site lands
pub struct DetailPanelToggle<S: 'static> {
    key: String,
    open: bool,
    on_toggle: Option<Box<dyn Fn(&mut S)>>,
}

#[allow(dead_code)] // ported base; remove when the grid header call site lands
impl<S: 'static> DetailPanelToggle<S> {
    /// `DetailPanelToggle::new("detail-panel-toggle", open)` — the key is
    /// this port's own (React has no `data-testid` on the button).
    pub fn new(key: impl Into<String>, open: bool) -> Self {
        Self {
            key: key.into(),
            open,
            on_toggle: None,
        }
    }

    /// React `onToggle` — the whole contract: one press, one flip; the
    /// open state itself lives in the host.
    pub fn on_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle = Some(Box::new(f));
        self
    }

    /// Paint the button. The two states are React's two class lists:
    /// the open one carries the accent wash, the closed one stays
    /// surface-quiet until hover.
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        // React 17: `title={open ? t('detail.hide') : t('detail.show')}`.
        let title = if self.open {
            t("detail.hide")
        } else {
            t("detail.show")
        };

        let mut button = row()
            .key(self.key)
            .role("button")
            .checked(self.open)
            .label(title)
            .w(28.0)
            .h(28.0)
            .items_center()
            .justify_center()
            // rounded-md
            .radius(6.0);
        // React 20-22: the open wash vs. the quiet surface.
        if self.open {
            button = button
                .fill(k.alpha(k.c.text_accent, 0.15))
                .hover(k.alpha(k.c.text_accent, 0.25));
        } else {
            button = button.hover(k.c.surface_raised);
        }

        // React 25: the divider's motion direction, at `h-4 w-4`.
        let glyph = if self.open {
            &ICON_PANEL_RIGHT_CLOSE
        } else {
            &ICON_PANEL_RIGHT_OPEN
        };
        button = button.child(
            icon(glyph, 16.0).color(if self.open {
                k.c.text_accent
            } else {
                k.c.text_muted
            }),
        );

        if let Some(on_toggle) = self.on_toggle {
            button = button.on_press(move |state: &mut S| on_toggle(state));
        }
        button
    }
}
