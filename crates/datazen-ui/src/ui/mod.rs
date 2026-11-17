//! Reusable UI primitives for DataZen's Rune interface.
//!
//! Thin wrappers over `rune` primitives and shared theme/tokens, so feature
//! views (for example the connection navigator) compose consistent controls
//! instead of ad-hoc styling.

pub(crate) mod dialog;
pub(crate) mod field;
pub(crate) mod kit;
pub(crate) mod nav;
pub(crate) mod select;
pub(crate) mod theme;
pub(crate) mod toggle;
