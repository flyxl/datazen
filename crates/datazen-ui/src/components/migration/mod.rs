//! Migration UI components (fail-closed refusal notices).
//!
//! Port of `packages/frontend/src/components/migration/`. The components in
//! this area have no production consumer in the Rune app yet; they are intended
//! to be wired by the Schema Diff / Data Sync migration tracks. Reusable API
//! items are kept with per-item `#[allow(dead_code)]` until wired.

pub(crate) mod migration_endpoints_bar;
pub(crate) mod migration_job_failure_notice;
