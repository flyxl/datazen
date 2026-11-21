//! The binary cell editor, ported from React
//! `packages/frontend/src/components/DataTable/editors/BinaryCellEditor.tsx`
//! (T278).
//!
//! The binary editor — which does not edit. A binary column is **not**
//! edited in place: the old editor wrote the string `"[1,2,3]"` back
//! into a `BLOB`, which no driver can use. The value is shown with its
//! size and the reason it cannot be edited here; the dedicated entry
//! point belongs to the special-type chapter (DB-13). The size is the
//! only thing read from the value: the editor never serialises a byte
//! array to measure it (05 §4.5 #5) — and this port reads neither size
//! nor value: the React control itself renders only the reason.
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | the status row (React test id) | `cell-editor-binary` |
//! | the visible reason text (internal) | `cell-editor-binary-text` |
//! | the error's sr-only echo (internal) | `cell-editor-binary-sr` |
//!
//! ## Props read
//!
//! The React control takes the host's whole `CellEditorControlProps`
//! but reads exactly two fields: `error` (whether the sr-only echo
//! mounts) and `ariaLabel` (the status role's name). The port takes
//! those two and nothing else — the host (T297) owns the rest.

use rune::{row, text, El, TextStyle};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::kit::kit;

/// `BinaryCellEditor`: the read-only status row — `flex h-7 w-full
/// items-center truncate text-[10px] text-fg-muted`, `role="status"`.
///
/// The sr-only echo (React 25) mounts only when the host carries an
/// error: screen readers announce the reason twice — once for the
/// visible row, once for the live region the error belongs to. Rune's
/// a11y hidden channel is the sr-only semantics.
pub(crate) struct BinaryCellEditor {
    aria_label: String,
    error: bool,
}

impl BinaryCellEditor {
    /// The control over the fields it reads (`ariaLabel`, `error`).
    pub(crate) fn new(aria_label: impl Into<String>, error: bool) -> Self {
        Self {
            aria_label: aria_label.into(),
            error,
        }
    }

    /// The status row.
    pub(crate) fn build(self, k: &Kit) -> El {
        let reason = t("cellEditor.binaryNotEditable");
        let mut row = row()
            .key("cell-editor-binary")
            .w_full()
            .h(28.0)
            .center_y()
            .role("status")
            .label(self.aria_label)
            .child(
                text(reason)
                    .key("cell-editor-binary-text")
                    .font(TextStyle { size: 10.0, ..k.ty.small })
                    .color(k.c.text_muted)
                    .truncate(),
            );
        if self.error {
            row = row.child(
                text(t("cellEditor.binaryNotEditable"))
                    .key("cell-editor-binary-sr")
                    .font(TextStyle { size: 10.0, ..k.ty.small })
                    .a11y_hidden(),
            );
        }
        row
    }
}

