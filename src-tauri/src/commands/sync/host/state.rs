//! Runtime-only Data Sync artifacts and selections.
//!
//! Durable Job definitions, state, progress, commit boundaries, and recovery
//! evidence live in `DesktopJobHost`. This module keeps the row-bearing
//! ChangeSet and reviewed selection only in process memory; neither contains a
//! live session id in durable storage. Cancel intent belongs to the Job record.

use std::collections::HashMap;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use std::sync::Arc;
use std::sync::{LazyLock, Mutex, MutexGuard};

#[cfg(test)]
use datazen_platform_api::context::RequestContext;
#[cfg(test)]
use datazen_platform_api::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
#[cfg(test)]
use datazen_runtime::budget::{BudgetConfig, BudgetLedger};
#[cfg(test)]
use datazen_runtime::job::{InMemoryJobRepository, SharedClock};

use crate::data_sync::job::ChangeSetArtifact;

use super::super::plans::SyncRunSelection;
use crate::data_sync::SyncOptions;

/// Deterministic clock origin: Job bookkeeping must not depend on wall time.
#[cfg(test)]
const CLOCK_ORIGIN: &str = "2026-01-01T00:00:00.000Z";
/// Claim TTL for the in-process Job repository (seconds).
#[cfg(test)]
const CLAIM_TTL_SECS: i64 = 300;

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
static CLOCK: LazyLock<Arc<SharedClock>> =
    LazyLock::new(|| Arc::new(SharedClock::at(CLOCK_ORIGIN)));

#[cfg(test)]
static REPOSITORY: LazyLock<Arc<InMemoryJobRepository>> =
    LazyLock::new(|| Arc::new(InMemoryJobRepository::new(CLOCK.clone(), CLAIM_TTL_SECS)));

#[cfg(test)]
static LEDGER: LazyLock<Arc<Mutex<BudgetLedger>>> = LazyLock::new(|| {
    Arc::new(Mutex::new(BudgetLedger::new(
        BudgetConfig::desktop_default(),
    )))
});

/// Frozen ChangeSet artifacts, keyed by planId. They are process-local and
/// must be rebuilt through a fresh compare after application restart.
static ARTIFACTS: LazyLock<Mutex<HashMap<String, ChangeSetArtifact>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Reviewed selection per planId, installed after durable apply acceptance.
/// The client can only narrow it — never add rows or rewrite values (§5.1).
static SELECTIONS: LazyLock<Mutex<HashMap<String, StoredSelection>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(test)]
static CONFIRMED_SELECTIONS: LazyLock<Mutex<HashMap<String, StoredSelection>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Last host-observed failure per Job for in-process diagnostics. Durable
/// details contain only stable safe reason codes.
static FAILURES: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
pub(crate) struct StoredSelection {
    pub selection: SyncRunSelection,
    pub options: SyncOptions,
}

/// Desktop Jobs are single-tenant and run in-process, so the context only
/// has to identify the client session and the accepting request.
#[cfg(test)]
pub(crate) fn desktop_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("datazen-desktop"),
        PrincipalId::new("datazen-desktop"),
        None,
        ClientInstanceId::new("datazen-desktop"),
        RequestId::new("datazen-desktop"),
        None,
    )
}

#[cfg(test)]
pub(crate) fn clock() -> Arc<SharedClock> {
    CLOCK.clone()
}

#[cfg(test)]
pub(crate) fn repository() -> Arc<InMemoryJobRepository> {
    REPOSITORY.clone()
}

#[cfg(test)]
pub(crate) fn ledger() -> Arc<Mutex<BudgetLedger>> {
    LEDGER.clone()
}

/// Monotonic millisecond stamp for budget claims/releases. It is a counter,
/// not wall time, so permit evidence is reproducible across runs.
#[cfg(test)]
static CLOCK_MS: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(CLOCK_ORIGIN_MS));

#[cfg(test)]
const CLOCK_ORIGIN_MS: u64 = 1_767_225_600_000;

#[cfg(test)]
pub(crate) fn now_ms() -> u64 {
    CLOCK_MS.fetch_add(1, Ordering::Relaxed)
}

/// Drop test-only in-memory job state.
#[cfg(test)]
pub(crate) fn forget(job_id: &str) {
    lock(&FAILURES).remove(job_id);
}

pub(crate) fn store_artifact(artifact: ChangeSetArtifact) {
    lock(&ARTIFACTS).insert(artifact.plan_id.clone(), artifact);
}

pub(crate) fn load_artifact(plan_id: &str) -> Option<ChangeSetArtifact> {
    lock(&ARTIFACTS).get(plan_id).cloned()
}

/// Artifact id of a stored ChangeSet. The apply handler reports the same id
/// in `StageOutcome::artifact_ids`, so the receipt and the store agree.
pub(crate) fn artifact_id(plan_id: &str) -> String {
    format!("changeset-{plan_id}")
}

pub(crate) fn store_selection(plan_id: &str, selection: StoredSelection) {
    lock(&SELECTIONS).insert(plan_id.to_string(), selection);
}

/// Consume the reviewed selection. This is the one-time invariant behind §2.1 /
/// CM-41: the first apply Job that reaches this point owns the planId, and every
/// later claim — from this path or any other — finds nothing to execute.
pub(crate) fn take_selection(plan_id: &str) -> Option<StoredSelection> {
    lock(&SELECTIONS).remove(plan_id)
}

#[cfg(test)]
pub(crate) fn store_confirmed_selection(plan_id: &str, selection: StoredSelection) {
    lock(&CONFIRMED_SELECTIONS).insert(plan_id.to_string(), selection);
}

#[cfg(test)]
pub(crate) fn confirmed_selection(plan_id: &str) -> Option<StoredSelection> {
    lock(&CONFIRMED_SELECTIONS).get(plan_id).cloned()
}

pub(crate) fn record_failure(job_id: &str, message: impl Into<String>) {
    lock(&FAILURES).insert(job_id.to_string(), message.into());
}

#[cfg(test)]
pub(crate) fn failure_of(job_id: &str) -> Option<String> {
    lock(&FAILURES).get(job_id).cloned()
}

/// Allowlisted guidance only. Never project row values or raw driver diagnostics
/// into a Job view (including the live in-process view).
pub(crate) fn safe_failure_of(job_id: &str) -> Option<String> {
    failure_of(job_id)
        .as_deref()
        .and_then(safe_failure_message)
        .map(str::to_owned)
}

fn safe_failure_message(message: &str) -> Option<&'static str> {
    if message.contains("tupleRange") {
        Some("Invalid recordset: tupleRange must match the complete primary key in declared order.")
    } else {
        None
    }
}

#[cfg(test)]
mod failure_projection_tests {
    use super::safe_failure_message;

    #[test]
    fn recordset_guidance_never_echoes_row_values_or_driver_text() {
        let message = "tupleRange columns must match primary key; password=secret; row=private";
        assert_eq!(safe_failure_message(message), Some(
            "Invalid recordset: tupleRange must match the complete primary key in declared order."
        ));
        assert_eq!(safe_failure_message("driver password=secret"), None);
    }
}
