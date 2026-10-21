//! In-process Data Transfer cancel flags keyed by job id.
//!
//! The registry is only consulted by the legacy (pre-P5) run-path shims:
//! `exec.rs` / `exec/execution.rs` arm a cancel flag when a run request carries
//! a `job_id` and clear it again on completion. The cancel command checks the
//! durable Job host first, then this registry for a still-running legacy call.

pub(crate) use crate::services::job_registry::{cancel_registered_job, ensure_job, remove_job};
