//! T262: the status bar, ported 1:1 from
//! `packages/frontend/src/components/StatusBar.tsx`.
//!
//! ```text
//! <footer class="flex h-10 min-h-[40px] shrink-0 items-center justify-between
//!                gap-3 border-t border-edge bg-surface-alt px-4 text-xs text-fg-secondary">
//!   <div class="min-w-0 truncate">{left}</div>
//!   <div class="shrink-0">{right}</div>
//! </footer>
//! ```
//!
//! The window's bottom bar: the left half may shrink to nothing (its
//! content is expected to truncate — React's `truncate` is the child
//! text's ellipsis, and the child carries it), the right half never
//! shrinks. `bg-surface-alt` is the kit's `layer_01` (the diagnosis
//! panel's precedent), and the type is the bar's own
//! `text-xs text-fg-secondary` — the slots' children set their own.
//!
//! ## Key convention
//!
//! | part | key |
//! |---|---|
//! | footer | `<key>` |
//! | left slot | `<key>-left` |
//! | right slot | `<key>-right` |
//!
//! The slots mount even when empty (React's two divs always render;
//! an empty slot just paints nothing).

use rune::{col, row, El};

use crate::ui::kit::kit;

/// The 40-point footer bar.
pub(crate) struct StatusBar {
    key: String,
    left: Option<El>,
    right: Option<El>,
}

impl StatusBar {
    /// `StatusBar::new("status-bar")` — the key is this port's own
    /// (React has no `data-testid` on the footer).
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            left: None,
            right: None,
        }
    }

    /// React's `left` — the shrinking half.
    pub(crate) fn left(mut self, el: El) -> Self {
        self.left = Some(el);
        self
    }

    /// React's `right` — the fixed half.
    pub(crate) fn right(mut self, el: El) -> Self {
        self.right = Some(el);
        self
    }

    /// Paint the footer.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let key = self.key;
        row()
            .key(key.clone())
            .w_full()
            // React: `h-10 min-h-[40px] shrink-0`.
            .h(40.0)
            .min_h(40.0)
            .items_center()
            .justify_between()
            .gap(12.0)
            // React: `border-t border-edge bg-surface-alt px-4`.
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
            .fill(k.c.layer_01)
            .px(16.0)
            .child(
                col()
                    .key(format!("{key}-left"))
                    // React: `min-w-0 truncate` — the slot may shrink to
                    // nothing; the ellipsis belongs to the child text.
                    .min_w(0.0)
                    .items_start()
                    .child(self.left.unwrap_or_else(rune::el)),
            )
            .child(
                // React: the plain `shrink-0` right div.
                col()
                    .key(format!("{key}-right"))
                    .items_start()
                    .child(self.right.unwrap_or_else(rune::el)),
            )
    }
}
