//! Green success panel shown after a single-table export completes.
//!
//! Ported from `packages/frontend/src/components/export/ExportSuccessPanel.tsx`
//! (22 lines). Both React callers pass `t('export.success')` as the message
//! (`components/DataTable/DataExportDialog.tsx:374`,
//! `windows/connection/ExportDialog.tsx:332`); this port takes the resolved
//! message string, so the caller owns the key.
//!
//! ## Key convention
//!
//! React gives this panel no `data-testid` at all, so the keys below are new
//! stable names under the caller's `<key>` prefix:
//!
//! | part | key |
//! |---|---|
//! | root | `<key>` |
//! | check circle | `<key>-icon` |
//! | message | `<key>-message` |
//!
//! ## Deliberate boundaries
//!
//! * The geometry follows the already-ported inline twin in
//!   `components/connection/batch_export_dialog.rs` (`{key}-success`):
//!   a 48pt circle, a 24pt check, `py(24)`, `gap(8)` — the same React rules
//!   (`h-12 w-12`, `py-6`, `gap-2`), with the success wash at
//!   `alpha(success.fg, 0.15)` (`bg-green-500/15`).
//! * The check comes from the `icons.rs` registry (`ICON_CHECK`, the exact
//!   React path `M20 6 9 17l-5-5`); the registry draws it at stroke-width 2
//!   while React's inline svg uses 2.5. The registry is the single icon
//!   source (T0-04), so the panel does not carry its own copy.
//! * The message is `body_medium` (14pt medium): React `text-sm font-medium`.
//!   The batch dialog's inline twin renders its own success line at
//!   `text_small` (13pt); this panel keeps React's 14pt.
//! * No production consumer yet: the DataTable / connection export-dialog
//!   wiring tasks own one and pass `t("export.success")`.

// The production consumer is the export-dialog wiring task; until then the
// component tests are the only callers, so the dead-code lint is silenced
// here rather than in a crate-wide allow.
#![allow(dead_code)]

use rune::{col, el, icon, text, El};

use crate::icons::ICON_CHECK;
use crate::ui::kit::kit;

/// The panel: a centred green check over the caller's message.
pub(crate) struct ExportSuccessPanel {
    key: String,
    message: String,
}

impl ExportSuccessPanel {
    pub(crate) fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
        }
    }

    /// React's column: `flex flex-col items-center gap-2 py-6 text-center`.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let key = self.key;
        col()
            .key(key.clone())
            .w_full()
            .py(24.0)
            .items_center()
            .gap(8.0)
            .child(
                el().key(format!("{key}-icon"))
                    .size(48.0, 48.0)
                    .radius(24.0)
                    .center()
                    .fill(k.alpha(k.c.success.fg, 0.15))
                    .child(
                        icon(&ICON_CHECK, 24.0)
                            .key("icon")
                            .color(k.c.success.fg)
                            .a11y_hidden(),
                    ),
            )
            .child(
                text(self.message)
                    .key(format!("{key}-message"))
                    .font(k.ty.body_medium)
                    .color(k.c.text_base),
            )
    }
}
