//! The data grid's keyboard-shortcut description, ported 1:1 from
//! `packages/frontend/src/components/DataTable/GridKeyboardHints.tsx`.
//!
//! React renders one `div#<id>.sr-only` holding a `<p>` and a `<ul>` of seven
//! `<li>`; a grid reads it through `aria-describedby`. The tree here is the
//! same (root, hint, list, seven items), the roles are the same (`list`,
//! `listitem`) and the strings come from the same i18n keys.
//!
//! ## Rendering the `sr-only` half
//!
//! `sr-only` is a 1 point box, clipped to nothing, that remains in the
//! accessibility tree. Rune has no visually-hidden primitive and no child
//! clipping on a plain box (`El::a11y_hidden` removes the subtree from AT,
//! which is the opposite of `sr-only`). The port therefore keeps the box at
//! **1 point tall** and gives it `opacity(0.0)`: it occupies one point of
//! layout, paints nothing, and its text and list roles stay in the
//! accessibility tree for `aria-describedby`-style reading. The measured rect
//! is `[x, y, width, 1.0]`; nothing here is focusable, so it adds no Tab stop.
//!
//! ## Key convention
//!
//! | part       | key              |
//! |------------|------------------|
//! | root       | `<id>`           |
//! | hint       | `<id>-hint`      |
//! | list       | `<id>-list`      |
//! | item `<n>` | `<id>-item-<n>`  |

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

    /// Builds the one-point `sr-only` box: `list`/`listitem` for a reader, no
    /// paint, no focus. Nothing here is focusable.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let id = self.id;
        let mut list = col().key(format!("{id}-list")).role("list").w_full();
        for (index, hint_key) in HINT_KEYS.iter().enumerate() {
            list = list.child(
                text(t(hint_key))
                    .key(format!("{id}-item-{index}"))
                    .role("listitem")
                    .font(k.ty.small)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        }
        // `sr-only`: one point of layout, no paint, still in the a11y tree.
        col()
            .key(id.clone())
            .w_full()
            .h(1.0)
            .opacity(0.0)
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
