//! T286: `windows/data-transfer/TransferPairingNote.tsx` ported to Rune.
//!
//! Shown on the Transfer endpoints step only when the selected pair is
//! unsupported: the reason text when one exists, the generic
//! "unsupported pair" word otherwise — a bordered inline note.
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (React test id) | `data-transfer-path` |
//!
//! ## Deviations
//!
//! * `mt-4 inline-block` — the note sits in the step's column with a 16pt
//!   top margin, sized to its content (`inline-block`).
//! * `bg-surface` is the kit background (`k.c.bg_base`) — the crate's
//!   surface mapping.

// The production consumers are the Transfer endpoints step and the Data
// Sync window track; until they wire it in, the component tests are the
// only callers.
#![allow(dead_code)] // reusable component; wired by the transfer step

use rune::{col, text, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::kit::kit;

/// The note builder: the reason when present, the generic word otherwise.
pub(crate) struct TransferPairingNote {
    reason: Option<String>,
}

impl TransferPairingNote {
    pub(crate) fn new(reason: Option<String>) -> Self {
        Self { reason }
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let body = match self.reason.as_deref() {
            // A present-but-empty reason reads as absent, like React's
            // truthiness on the string.
            Some(reason) if !reason.is_empty() => reason.to_owned(),
            _ => t("common.unsupportedPair").to_owned(),
        };
        col()
            .key("data-transfer-path")
            .mt(16.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.bg_base)
            .px(8.0)
            .py(4.0)
            .child(
                text(body)
                    .key("data-transfer-path-text")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
    }
}
