//! Port of `packages/frontend/src/windows/connection/er/relationStyle.ts`.
//!
//! How the ER canvas tells a declared relationship from an inferred one. The
//! graph builder and the legend both read these, so the swatch in the legend
//! is literally the stroke drawn on the edge. A second copy in the legend
//! could drift from the canvas, and a legend that lies is worse than none.
//!
//! The strings are CSS values handed to xyflow's edge styling; the Rune
//! canvas host resolves the `--c-*` tokens against the active palette when it
//! mounts. `er_relation_legend.rs` derives its swatch dash pattern from
//! [`ER_PREDICTED_DASH`], and a test pins the derivation.

/// Stroke of a relationship a foreign key constraint declares.
pub(crate) const ER_DECLARED_COLOR: &str = "var(--c-accent, #3b82f6)";

/// Stroke of a relationship the engine inferred from structure and naming.
pub(crate) const ER_PREDICTED_COLOR: &str = "var(--c-warning, #e39a27)";

/// Dash pattern for an inferred relationship.
///
/// Dashed rather than only recoloured: a colour-only distinction disappears
/// in a monochrome export and for a colour-blind reader, while a dash
/// survives both.
pub(crate) const ER_PREDICTED_DASH: &str = "4 3";
