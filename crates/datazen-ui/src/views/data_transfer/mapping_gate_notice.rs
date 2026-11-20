//! T176: `windows/data-transfer/MappingGateNotice.tsx` ported to Rune.
//!
//! The one sentence that explains a Next button the same predicate
//! disarmed. A create-new row nobody has named keeps the user on the
//! mapping step with a reason they can act on, instead of a button that
//! is simply off — and a prepare that came back admitted over rows that
//! no longer clear the gate still refuses to advance into a preview
//! built for other rows.
//!
//! ## The message chain (verbatim from React)
//!
//! 1. `visible` false renders nothing (the notice only exists on the
//!    mapping step).
//! 2. `gateLost` wins first: a prepare receipt that came back with a
//!    reason is the reason shown, whatever the rows say now.
//! 3. Otherwise `mappingGateBlockReason(rows)` supplies the i18n key
//!    (only the unnamed-create-new case: the key
//!    `transfer.mapping.targetNameRequired`), and `t(reason)` renders it.
//! 4. An empty message renders nothing — a mapping step that clears the
//!    gate shows no banner, and a `gateLost` of `''` is falsy exactly
//!    like the React `||` chain.
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | the banner (React test id) | `data-transfer-mapping-gate-error` |
//! | the banner's message text (internal) | `…-text` |
//!
//! ## Documented deviations from React
//!
//! * **`border-danger/40` and `bg-danger/10`** become the kit danger
//!   foreground at alphas 0.4 and 0.1 (the `text-danger` foreground is the
//!   same token at full alpha) — the crate convention for semantic danger
//!   banners, with no new color literal.
//! * **`rounded-lg`** is `k.radius.lg`; `px-4 py-3` are 16/12pt;
//!   `text-sm` is `k.ty.text_small`.
//! * **The message wraps** (a CSS block's default) — long prepare errors
//!   fold at the width instead of overflowing the window.

// The production consumer is the Data Transfer window (T147's render
// tree mounts it at the mapping step); until the wiring lands, the
// component tests are the only callers, so the dead-code lint is silenced
// here rather than crate-wide.
#![allow(dead_code)] // reusable component; wired by the Data Transfer window track

use rune::{col, el, text, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::data_transfer::{mapping_gate_block_reason, TransferTableVm};

/// The gate notice. `visible` mirrors the React prop (the mapping step);
/// `rows` are the mapping-step tables the reason is derived from;
/// `gate_lost` is the prepare receipt's failure text (`''` = none).
pub(crate) struct MappingGateNotice {
    visible: bool,
    rows: Vec<TransferTableVm>,
    gate_lost: String,
}

impl MappingGateNotice {
    pub(crate) fn new(visible: bool, rows: Vec<TransferTableVm>, gate_lost: String) -> Self {
        Self {
            visible,
            rows,
            gate_lost,
        }
    }

    /// The notice, or nothing (`el()` = React's `null`).
    pub(crate) fn build(self, dark: bool) -> El {
        if !self.visible {
            return el();
        }
        // React: `const message = gateLost || (reason ? t(reason) : '')`.
        let message = if self.gate_lost.is_empty() {
            match mapping_gate_block_reason(&self.rows) {
                Some(key) => t(key).to_owned(),
                None => String::new(),
            }
        } else {
            self.gate_lost
        };
        if message.is_empty() {
            return el();
        }
        let k = kit(dark);
        col()
            .key("data-transfer-mapping-gate-error")
            .role("alert")
            .w_full()
            .radius(k.radius.lg)
            .border(1.0, k.alpha(k.c.danger.fg, 0.4))
            .fill(k.alpha(k.c.danger.fg, 0.1))
            .px(16.0)
            .py(12.0)
            .child(
                text(message)
                    .key("data-transfer-mapping-gate-error-text")
                    .font(k.ty.text_small)
                    .color(k.c.danger.fg)
                    .wrap(),
            )
    }
}
