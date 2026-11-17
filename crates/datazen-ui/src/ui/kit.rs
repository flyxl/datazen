//! DataZen's entry point into `rune-kit`.
//!
//! Every control the parity port adds comes from `rune-kit` and reads only
//! `Kit` tokens, so restyling the app happens here and nowhere else.
//! `Kit::neutral` derives its palette from `rune::tok::theme`, the same base
//! the hand-written views already paint with, so adopting a kit component
//! keeps the look instead of introducing a second theme.

use std::sync::OnceLock;

use rune_kit::Kit;

static DARK: OnceLock<Kit> = OnceLock::new();
static LIGHT: OnceLock<Kit> = OnceLock::new();

/// The kit for one appearance, built once: `Kit::neutral` resolves ~70 tokens
/// and a few shadow layers, too much to repeat on every frame.
pub(crate) fn kit(dark: bool) -> &'static Kit {
    let slot = if dark { &DARK } else { &LIGHT };
    slot.get_or_init(|| Kit::neutral(dark))
}
