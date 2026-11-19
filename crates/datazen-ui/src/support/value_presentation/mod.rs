//! Ports of `packages/frontend/src/lib/valuePresentation/` — the issue-32
//! field-presentation layer (temporal math, the timestamp renderer's config
//! and contribution, presentation rules and plans, column overrides and
//! their persistence shape).
//!
//! The area lands file-by-file with its own ledger tasks; this module grows
//! as they do. `temporalMath` (T207) is first; `timestampFormat`,
//! `timestampConfig`, `valuePresentationRules`, `timestampContribution`,
//! `presentationPlan`, `columnPresentation`, `columnPresentationSession`,
//! `columnOverrides`, `settings` and `valuePresentationSave` follow in
//! dependency order.
//!
//! The extension-points vocabulary the area leans on
//! (`PresentationValue`, `ColumnSource`, `ColumnPresentationOverride`,
//! `ValuePresentationRegistry`, …) lives in
//! `packages/extension-points/src/valuePresentation/`; those contracts are
//! ported here alongside their first consumer rather than as a separate
//! crate — the Rust host has no extension sandbox, so a contract type plus a
//! registry keyed by contribution id is the whole surface.

pub(crate) mod temporal_math;
pub(crate) mod timestamp_format; // T272: the three output formats
pub(crate) mod timestamp_config; // T230: persisted options + full validation
pub(crate) mod value_presentation_rules; // T291: rules, restricted regex, matching
// The extension-points valuePresentation slice (contracts + registry) —
// ported beside their first in-tree consumer (T251).
pub(crate) mod contracts;
pub(crate) mod registry;
pub(crate) mod timestamp_contribution; // T251: the datazen.timestamp contribution
pub(crate) mod presentation_plan; // T155: column plans, override keys, cache keys
