//! Process-global, AppState-free kernel shared by every Data Sync Job.
//!
//! Only AppState-free items live here (artifact store, reviewed selection,
//! cancel intent, JobRepository/BudgetLedger/Clock). `ConnectionManager` and
//! `SyncAdapterRegistry` stay per-submission, because they are bound to a
//! specific `AppState` instance.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen_runtime::budget::{BudgetConfig, BudgetLedger};
use datazen_runtime::job::{InMemoryJobRepository, SharedClock};

use crate::data_sync::job::ChangeSetArtifact;

use super::super::plans::SyncRunSelection;
use crate::data_sync::SyncOptions;

/// Deterministic clock origin: Job bookkeeping must not depend on wall time.
const CLOCK_ORIGIN: &str = "2026-01-01T00:00:00.000Z";
/// Claim TTL for the in-process Job repository (seconds).
const CLAIM_TTL_SECS: i64 = 300;

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

static CLOCK: LazyLock<Arc<SharedClock>> =
    LazyLock::new(|| Arc::new(SharedClock::at(CLOCK_ORIGIN)));

static REPOSITORY: LazyLock<Arc<InMemoryJobRepository>> =
    LazyLock::new(|| Arc::new(InMemoryJobRepository::new(CLOCK.clone(), CLAIM_TTL_SECS)));

static LEDGER: LazyLock<Arc<Mutex<BudgetLedger>>> = LazyLock::new(|| {
    Arc::new(Mutex::new(BudgetLedger::new(
        BudgetConfig::desktop_default(),
    )))
});

/// Frozen ChangeSet Artifacts, keyed by planId (§2.1: the artifact is the
/// only thing that survives the prepare Job).
static ARTIFACTS: LazyLock<Mutex<HashMap<String, ChangeSetArtifact>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Reviewed selection per planId, captured before the apply Job is accepted.
/// The client can only narrow it — never add rows or rewrite values (§5.1).
static SELECTIONS: LazyLock<Mutex<HashMap<String, StoredSelection>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Host-owned cancel intent. The runtime `CancelToken` only fires at stage
/// start, so batch granularity comes from this flag (§5.3).
static CANCEL: LazyLock<Mutex<HashMap<String, CancelEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Last host-observed failure per Job. The runtime does not carry handler
/// error text, so the IPC message is reconstructed from this plus the Job
/// state / effect outcome (see `jobs::message_for`).
static FAILURES: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
pub(crate) struct StoredSelection {
    pub selection: SyncRunSelection,
    pub options: SyncOptions,
}

#[derive(Clone)]
pub(crate) struct CancelEntry {
    pub flag: Arc<AtomicBool>,
    pub ctx: RequestContext,
}

/// Desktop Jobs are single-tenant and run in-process, so the context only
/// has to identify the client session and the accepting request.
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

pub(crate) fn clock() -> Arc<SharedClock> {
    CLOCK.clone()
}

pub(crate) fn repository() -> Arc<InMemoryJobRepository> {
    REPOSITORY.clone()
}

pub(crate) fn ledger() -> Arc<Mutex<BudgetLedger>> {
    LEDGER.clone()
}

/// Monotonic millisecond stamp for budget claims/releases. It is a counter,
/// not wall time, so permit evidence is reproducible across runs.
static CLOCK_MS: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(CLOCK_ORIGIN_MS));

const CLOCK_ORIGIN_MS: u64 = 1_767_225_600_000;

pub(crate) fn now_ms() -> u64 {
    CLOCK_MS.fetch_add(1, Ordering::Relaxed)
}

/// Mint (or fetch) the cancel flag for a Job id. Insert-then-set so a cancel
/// for an unknown id still reports success (the UI must not lose the click).
pub(crate) fn cancel_flag(job_id: &str, ctx: RequestContext) -> Arc<AtomicBool> {
    let mut entries = lock(&CANCEL);
    let entry = entries
        .entry(job_id.to_string())
        .or_insert_with(|| CancelEntry {
            flag: Arc::new(AtomicBool::new(false)),
            ctx,
        });
    entry.flag.clone()
}

pub(crate) fn ctx_of(job_id: &str) -> Option<RequestContext> {
    lock(&CANCEL).get(job_id).map(|entry| entry.ctx.clone())
}

/// Set cancel intent and return whether this call flipped a live intent.
/// Unknown ids are reported as `true` so the caller keeps its UI contract.
pub(crate) fn request_cancel(job_id: &str, ctx: Option<RequestContext>) -> bool {
    let mut entries = lock(&CANCEL);
    match entries.get_mut(job_id) {
        Some(entry) => {
            entry.flag.store(true, Ordering::SeqCst);
            true
        }
        None => {
            entries.insert(
                job_id.to_string(),
                CancelEntry {
                    flag: Arc::new(AtomicBool::new(true)),
                    ctx: ctx.unwrap_or_else(desktop_context),
                },
            );
            true
        }
    }
}

pub(crate) fn is_cancelled(job_id: &str) -> bool {
    lock(&CANCEL)
        .get(job_id)
        .map(|entry| entry.flag.load(Ordering::SeqCst))
        .unwrap_or(false)
}

/// Drop the per-Job state once the Job reached a terminal state.
pub(crate) fn forget(job_id: &str) {
    lock(&CANCEL).remove(job_id);
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

pub(crate) fn record_failure(job_id: &str, message: impl Into<String>) {
    lock(&FAILURES).insert(job_id.to_string(), message.into());
}

pub(crate) fn failure_of(job_id: &str) -> Option<String> {
    lock(&FAILURES).get(job_id).cloned()
}
