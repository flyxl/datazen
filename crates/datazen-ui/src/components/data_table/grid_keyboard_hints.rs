//! The data grid's keyboard-shortcut description, ported 1:1 from
//! `packages/frontend/src/components/DataTable/GridKeyboardHints.tsx`.
//!
//! React renders one `div#<id>.sr-only` holding a `<p>` and a `<ul>` of seven
//! `<li>`; a grid reads it through `aria-describedby`. The tree here is the
//! same (root, hint, list, seven items) and the strings come from the same i18n
//! keys.
//!
//! ## Rendering the `sr-only` half
//!
//! Rune has no visually-hidden-but-accessible primitive (no `sr-only`
//! equivalent in `vendor/rune/crates/rune/src`), and `El::description` is
//! per-element text, not an element that a sibling can reference by id. The
//! port therefore renders the same text as a visible, muted hint strip: text
//! only, never focusable, so it adds no Tab stop. It wraps instead of running
//! out of the window. A later grid call site that wants React's
//! `aria-describedby` behaviour attaches the same strings with
//! `El::description`; that wiring belongs to the grid, not to this component.
//!
//! ## Key convention
//!
//! | part      | key               |
//! |-----------|-------------------|
//! | root      | `<id>`            |
//! | hint      | `<id>-hint`       |
//! | list      | `<id>-list`       |
//! | item `<n>`| `<id>-item-<n>`   |

use rune::{col, text, El};

use crate::i18n::t;
use crate::ui::kit::kit;

/// The seven shortcut descriptions, in React's order.
const HINT_KEYS: [&str; 7] = [
    "tableView.shortcut.enterEdit",
    "tableView.shortcut.extendSelection",
    "tableView.shortcut.moveByPage",
    "tableView.shortcut.moveToEdge",
    "tableView.shortcut.selectAllRows",
    "tableView.shortcut.quickLook",
    "tableView.shortcut.tabLeavesGrid",
];

/// The grid's keyboard hints, labelled with the id the grid owns.
#[allow(dead_code)] // ported base; remove when the data grid call site lands
pub(crate) struct GridKeyboardHints {
    id: String,
}

#[allow(dead_code)] // ported base; remove when the data grid call site lands
impl GridKeyboardHints {
    pub(crate) fn new(id: impl Into<String>) -> GridKeyboardHints {
        GridKeyboardHints { id: id.into() }
    }

    /// Builds the hint strip. Nothing here is focusable.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let id = self.id;
        let mut list = col().key(format!("{id}-list")).w_full().gap(2.0);
        for (index, hint_key) in HINT_KEYS.iter().enumerate() {
            list = list.child(
                text(t(hint_key))
                    .key(format!("{id}-item-{index}"))
                    .font(k.ty.small)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        }
        col()
            .key(id.clone())
            .w_full()
            .gap(2.0)
            .child(
                text(t("tableView.shortcut.hint"))
                    .key(format!("{id}-hint"))
                    .font(k.ty.small)
                    .color(k.c.text_faint)
                    .wrap(),
            )
            .child(list)
    }
}
