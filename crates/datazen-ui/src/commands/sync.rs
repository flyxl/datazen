//! Port of `packages/frontend/src/commands/sync.ts` (T406, split) —
//! the Data Sync commands (W8 刀3e web 收口, contract §6.16).
//!
//! The types and the 22-route domain methods live on the
//! backend-client's `dataSync` domain — both transports (desktop IPC
//! / server HTTP) share the wire vocabulary (camelCase) — and this
//! port keeps exactly the UI-side orchestration the React file keeps:
//! the receipt-driven polling, the comparison plan's module-level
//! staging, and the selection's client-side folding.
//!
//! Two legacy commands deliberately stay (each one's domain ruling in
//! its doc): `apply_data_sync` (the desktop-permanently-refused old
//! apply shape; the domain methods `startDataSyncApplyJob` /
//! `executeDataSync` are the admitted faces) and
//! `get_data_sync_comparison_page` (the plan_store row-level paged
//! read; contract §6.16's 22 routes have no method for it).
//!
//! The split mirrors `commands/schema_diff`: [`wire`] (the MethodMap
//! slice, the payload shapers, the option/selection VMs) and
//! [`runner`] (the selection folding, the submit→poll→resolve
//! machines, the staging).

pub mod runner;
pub mod wire;
