//! In-process Data Transfer cancel flags keyed by job id.
//!
//! The registry is only consulted by the legacy (pre-P5) run-path shims:
//! `exec.rs` / `exec/execution.rs` arm a cancel flag when a run request carries
//! a `job_id` and clear it again on completion, and the dedicated cancel
//! command always goes through the durable Job path instead.

pub(crate) use crate::services::job_registry::{ensure_job, remove_job};
