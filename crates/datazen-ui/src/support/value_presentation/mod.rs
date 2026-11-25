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

pub mod temporal_math;
pub mod timestamp_format; // T272: the three output formats
pub mod timestamp_config; // T230: persisted options + full validation
pub mod value_presentation_rules; // T291: rules, restricted regex, matching
// The extension-points valuePresentation slice (contracts + registry) —
// ported beside their first in-tree consumer (T251).
pub mod contracts;
pub mod registry;
pub mod timestamp_contribution; // T251: the datazen.timestamp contribution
pub mod presentation_plan; // T155: column plans, override keys, cache keys
pub mod column_presentation; // T085: the column panel's pure logic
pub mod column_presentation_session; // T123: session overrides + the merge point
// T037 brings the shared persistence shapes; their own logic lands with
// T182/T310 (documented two-step split in each file's header).
pub mod settings;
pub mod value_presentation_save;
pub mod column_overrides; // T037: override persistence
pub mod register_bundled_contributions; // T182: startup registration
