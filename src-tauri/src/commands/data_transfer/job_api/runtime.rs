//! Admit and drive one Data Transfer Job through the P5 Job runtime.
//!
//! A run has two steps. [`admit`] mints the Job id, stores the Job and records
//! the idempotency receipt: that moment is what makes the id *addressable*, and
//! it happens while the Job is still `queued`, before any write. [`drive`]
//! then runs the handler to a terminal state. The split is the whole point of
//! this module — a Job whose id is only published after the run finished
//! cannot be cancelled while it is running and reports no progress while it
//! runs, so the caller can only ever learn the outcome after the fact.
//!
//! Contracts enforced here:
//!
//! * prepare and apply are distinct Jobs; apply carries only `planId` + plan digest +
//!   selection revision + reviewed selection + confirmation — never the plan body;
//! * one `planId` is consumable by exactly one apply Job: the repository rejects a
//!   second accept with `PlanAlreadyConsumed`, and the host marks the plan consumed
//!   only after a terminal success;
//! * a repeated idempotency key replays the *recorded* Job instead of creating a
//!   second one, so an unknown commit never becomes a second write. That is why
//!   the apply path asks [`receipt_for`] *before* it claims the plan: once the
//!   plan is consumed a retry would be refused as "already consumed" and could
//!   never recover the result of the attempt that consumed it.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{CommitBoundary, JobDefinition, JobProgress, JobState};
use datazen_platform_api::id::{
    ClientInstanceId, IdempotencyKey, JobId, OrganizationId, PrincipalId, RequestId, Timestamp,
    WorkerId,
};
use datazen_platform_api::{JobRepository, PortError, RequestContext};
use datazen_runtime::budget::{BudgetConfig, BudgetLedger};
use datazen_runtime::job::{
    EndpointRef, EndpointRole, HandlerRegistry, InMemoryJobRepository, JobClock, JobHandler,
    JobRuntime, RecoveryVerdict, SharedClock,
};

use super::super::plans;
use super::endpoint_identity::EndpointIdentity;
use crate::commands::error::CommandError;
use crate::data_transfer::job::{derive_evidence, DataTransferHandler};

pub(crate) const PREPARE_KIND: &str = "dataTransferPrepare";
pub(crate) const APPLY_KIND: &str = "dataTransferApply";
const CLAIM_TTL_SECS: i64 = 300;
const LOCAL_CLIENT_INSTANCE: &str = "datazen-local-client";
pub(crate) const RECOVERY_RESUME_AFTER_VERIFY: &str = "resumeAfterVerify";
pub(crate) const RECOVERY_REJECT: &str = "reject";
pub(crate) const RECOVERY_REQUIRE_MANUAL_REVIEW: &str = "requireManualReview";

/// The continuation an admitted Job still owes its caller, boxed so it can
/// outlive the call that admitted it and be driven from a detached task.
pub(crate) type BoxedJobRun =
    Pin<Box<dyn Future<Output = Result<JobOutcome, CommandError>> + Send + 'static>>;

/// Everything the two commands need from one Job run.
pub(crate) struct JobOutcome {
    pub(crate) job_id: String,
    pub(crate) kind: String,
    pub(crate) state: JobState,
    pub(crate) effect_outcome: EffectOutcome,
    pub(crate) progress: JobProgress,
    pub(crate) commit_boundaries: Vec<CommitBoundary>,
    pub(crate) artifact_ids: Vec<String>,
    pub(crate) error: Option<String>,
    /// Verdict as decided by the handler over the recorded checkpoint.
    pub(crate) recovery: RecoveryReport,
    /// True when an idempotency key replayed an already accepted Job.
    pub(crate) replayed: bool,
}

/// What `verify_recovery` decided for the recorded checkpoint, in a shape the
/// IPC view can carry. The runtime's own `Checkpoint::recovery_policy` is not
/// trustworthy here (it is hardcoded — see `recovery_report`), so the verdict is
/// always recomputed from the freeze's real policy.
#[derive(Debug, Clone)]
pub(crate) struct RecoveryReport {
    pub(crate) verdict: String,
    pub(crate) resume_through: Option<usize>,
    pub(crate) reason: Option<String>,
}

/// What a caller hands to the runtime seam.
pub(crate) struct JobRunRequest<'a> {
    pub(crate) kind: &'a str,
    pub(crate) payload: serde_json::Value,
    pub(crate) handler: Arc<DataTransferHandler>,
    pub(crate) endpoints: Vec<EndpointRef>,
    /// Host-computed artifact ids the Job view cannot expose (SQL file digest).
    pub(crate) artifact_ids: Vec<String>,
    pub(crate) idempotency_key: &'a str,
    pub(crate) plan_id: Option<&'a str>,
}

pub(super) struct TransferJobHost {
    pub(super) repo: Arc<InMemoryJobRepository>,
    ledger: Arc<Mutex<BudgetLedger>>,
    clock: Arc<SharedClock>,
    /// idempotency key → accepted Job id. A replay never mints a second Job:
    /// the receipt is written at admission, before the write attempt, so a
    /// crash during the attempt still leaves a way back to its recorded result.
    receipts: Mutex<HashMap<String, JobId>>,
    /// Job id → artifact ids the host computed itself. The Job view cannot
    /// surface the artifacts a stage emits (the emitted SQL file is one), so
    /// the host hashes them and keeps them here; an apply that returned at
    /// admission has no other place to publish them.
    artifacts: Mutex<HashMap<JobId, Vec<String>>>,
    /// Job id → the counters its run accumulated.
    ///
    /// The repository has no port for them: `accept` writes `JobProgress::default()`
    /// once and nothing in the `JobRepository` contract can change it, while the
    /// runtime accumulates the real counters in `dispatch` and hands them back in
    /// `JobResult`. A detached run has no caller left to read that result, so the
    /// host keeps it here — the same reason artifacts are kept — and `queries`
    /// folds it into the view it publishes.
    progress: Mutex<HashMap<JobId, JobProgress>>,
}

/// Process-wide Job host. State is in-memory like the existing connection
/// sessions: this host *is* the local desktop backend, so it carries no remote
/// principal and no session token, and a background service cannot reach it.
pub(super) fn host() -> &'static TransferJobHost {
    static HOST: OnceLock<TransferJobHost> = OnceLock::new();
    HOST.get_or_init(|| {
        let clock = Arc::new(SharedClock::at(now_timestamp().as_str()));
        let repo = Arc::new(InMemoryJobRepository::new(
            clock.clone() as Arc<dyn JobClock>,
            CLAIM_TTL_SECS,
        ));
        let ledger = Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default())));
        TransferJobHost {
            repo,
            ledger,
            clock,
            receipts: Mutex::new(HashMap::new()),
            artifacts: Mutex::new(HashMap::new()),
            progress: Mutex::new(HashMap::new()),
        }
    })
}

pub(crate) fn now_timestamp() -> Timestamp {
    Timestamp::new(
        chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
    )
}

fn now_millis() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Local desktop identity: a background service is not allowed to reach this
/// profile, so the context carries no remote principal and no session token.
pub(super) fn request_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("datazen-local"),
        PrincipalId::new("datazen-local-user"),
        None,
        ClientInstanceId::new(LOCAL_CLIENT_INSTANCE),
        RequestId::new(format!("transfer-{}", uuid::Uuid::new_v4())),
        None,
    )
}

/// Endpoint refs for a frozen run: one reader, and a writer only when the run
/// actually writes a database. A SQL-file destination has no target connection
/// to identify, so it contributes no writer and `target_identity` is `None` —
/// one condition decides both, so the two can never drift apart.
///
/// Identity comes from the user's real connection configs (see
/// [`super::endpoint_identity`]). Both endpoints used to be given the single
/// constant `TRANSFER_SERVICE_KEY`, which made every transfer look like one
/// service reading and writing `public.users` on itself.
pub(crate) fn endpoint_refs(
    source_objects: Vec<String>,
    target_objects: Vec<String>,
    source_identity: &EndpointIdentity,
    target_identity: Option<&EndpointIdentity>,
) -> Vec<EndpointRef> {
    let mut refs = vec![EndpointRef {
        connection_id: source_identity.connection_id.clone(),
        service_key: source_identity.service_key.clone(),
        objects: source_objects,
        role: EndpointRole::SourceReader,
    }];
    if let Some(target) = target_identity {
        refs.push(EndpointRef {
            connection_id: target.connection_id.clone(),
            service_key: target.service_key.clone(),
            objects: target_objects,
            role: EndpointRole::TargetWriter,
        });
    }
    refs
}

/// The budget ledger only admits a claim for a service it already knows, so an
/// endpoint has to be registered before its first Job. Registration is keyed by
/// the endpoint's own connection id — the same id the overlap detector compares —
/// so a later Job over the same connection re-registers nothing (`ensure_service`
/// is idempotent) while each genuinely distinct connection gets its own entry.
///
/// These are no longer a fixed pair of placeholder ids: every endpoint is
/// registered under the user's own persisted `connectionId`, so the ledger
/// tracks the connections this client actually transfers over, and the
/// all-or-nothing reservation covers each of them separately.
fn ensure_endpoint_services(
    host: &TransferJobHost,
    endpoints: &[EndpointRef],
) -> Result<(), CommandError> {
    let mut ledger = host
        .ledger
        .lock()
        .map_err(|_| CommandError::Internal("transfer job budget ledger is poisoned".into()))?;
    for endpoint in endpoints {
        ledger.ensure_service(&endpoint.connection_id);
    }
    Ok(())
}

/// A Job the repository now holds, together with the identity the worker needs.
///
/// The Job exists and is `queued`: it is addressable by
/// [`AcceptedJob::job_id`] and cancellable, but nothing has been written yet.
pub(crate) struct AcceptedJob {
    /// The addressable Job id, fixed at admission.
    pub(crate) job_id: JobId,
    /// The kind the Job was accepted under; echoed back in the caller's view.
    pub(crate) kind: String,
    /// The admitting request's own context, so driving does not need a second one.
    ctx: RequestContext,
}

/// What admitting a run produced.
pub(crate) enum Admission {
    /// A fresh queued Job with work still owed to it.
    Accepted(AcceptedJob),
    /// The recorded outcome of a Job this idempotency key already ran. No second
    /// Job was minted and nothing is left to drive.
    Replayed(JobOutcome),
}

/// Everything driving an accepted Job needs, owned rather than borrowed so the
/// continuation can outlive the call that admitted it.
pub(crate) struct DrivePlan {
    pub(crate) handler: Arc<DataTransferHandler>,
    pub(crate) endpoints: Vec<EndpointRef>,
    pub(crate) artifact_ids: Vec<String>,
    pub(crate) plan_id: Option<String>,
}

impl DrivePlan {
    pub(crate) fn from_request(request: &JobRunRequest<'_>) -> Self {
        Self {
            handler: request.handler.clone(),
            endpoints: request.endpoints.clone(),
            artifact_ids: request.artifact_ids.clone(),
            plan_id: request.plan_id.map(str::to_string),
        }
    }
}

/// The Job this idempotency key already named, if this host has one.
///
/// Read-only, and that is the point: the apply path consults it *before* it
/// claims the plan. A retry whose first attempt already consumed the plan would
/// otherwise be refused as "already consumed" forever and could never recover
/// the result that attempt produced.
pub(crate) fn receipt_for(idempotency_key: &str) -> Option<JobId> {
    lookup_receipt(host(), idempotency_key)
}

/// The recorded outcome of an already-run Job, replayed under `key`'s receipt.
pub(crate) async fn replayed_outcome(
    job_id: &JobId,
    artifact_ids: Vec<String>,
    handler: &DataTransferHandler,
) -> Result<JobOutcome, CommandError> {
    let host = host();
    let ctx = request_context();
    replay(host, &ctx, job_id, artifact_ids, handler).await
}

/// Artifact ids the host computed for a Job, which the Job record cannot carry.
pub(super) fn host_artifacts(job_id: &str) -> Vec<String> {
    host()
        .artifacts
        .lock()
        .ok()
        .map_or_else(Vec::new, |artifacts| {
            artifacts.get(job_id).cloned().unwrap_or_default()
        })
}

/// Record host-computed artifact ids so a Job that returned at admission can
/// still publish them through the Job query commands.
pub(super) fn remember_artifacts(job_id: &str, artifact_ids: &[String]) {
    let Ok(mut artifacts) = host().artifacts.lock() else {
        return;
    };
    let entry = artifacts.entry(JobId::new(job_id.to_string())).or_default();
    for artifact in artifact_ids {
        if !entry.contains(artifact) {
            entry.push(artifact.clone());
        }
    }
}

/// The counters a Job's run accumulated, if this host recorded them.
///
/// `None` means the run has not reported yet — the Job is still queued or still
/// executing, and the repository's own view (all zeros) is the honest answer for
/// that window.
pub(super) fn host_progress(job_id: &str) -> Option<JobProgress> {
    host().progress.lock().ok()?.get(job_id).cloned()
}

/// Keep a run's counters under its Job id. Best effort: losing them costs the UI
/// a zeroed progress bar, which is not worth failing a finished migration over.
fn remember_progress(job_id: &JobId, progress: &JobProgress) {
    if let Ok(mut recorded) = host().progress.lock() {
        recorded.insert(job_id.clone(), *progress);
    }
}

/// Accept one Job into the repository, without running it.
///
/// This is the moment the Job id becomes addressable: the Job is stored and its
/// idempotency receipt is written, both before the handler runs. Everything above
/// is a pure setup step, so a caller can hand the id out and cancel or poll the
/// Job while it is still queued.
pub(crate) async fn admit(request: &JobRunRequest<'_>) -> Result<Admission, CommandError> {
    let host = host();
    host.clock.set(now_timestamp().as_str());
    let ctx = request_context();
    if let Some(receipt) = lookup_receipt(host, request.idempotency_key) {
        return Ok(Admission::Replayed(
            replay(
                host,
                &ctx,
                &receipt,
                request.artifact_ids.clone(),
                request.handler.as_ref(),
            )
            .await?,
        ));
    }
    ensure_endpoint_services(host, &request.endpoints)?;
    let job_id = JobId::new(format!(
        "transfer-{}-{}",
        request.kind,
        uuid::Uuid::new_v4()
    ));
    let definition = JobDefinition {
        job_id: job_id.clone(),
        kind: request.kind.to_string(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new(LOCAL_CLIENT_INSTANCE),
            purpose: request.kind.to_string(),
        },
        payload: request.payload.clone(),
        created_at: host.clock.now(),
    };
    let record = host
        .repo
        .accept(
            &ctx,
            definition,
            &IdempotencyKey::new(request.idempotency_key.to_string()),
        )
        .await
        .map_err(admit_error)?;
    remember_receipt(host, request.idempotency_key, &job_id)?;
    Ok(Admission::Accepted(AcceptedJob {
        job_id,
        kind: record.view.kind.clone(),
        ctx,
    }))
}

/// Closed writes, keyed by plan id. See [`hold_write_closed`].
#[cfg(test)]
fn write_gates() -> &'static Mutex<HashMap<String, Arc<WriteGate>>> {
    static GATES: OnceLock<Mutex<HashMap<String, Arc<WriteGate>>>> = OnceLock::new();
    GATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Hold one plan's write closed, so a test can stand on the far side of it.
///
/// [`drive`] parks on this gate immediately before it dispatches the handler, and
/// that is the only place a gate can answer the question the Job split exists for:
/// *when* does the command hand back the id. Everything above the gate is
/// admission bookkeeping and has written nothing; everything below it has already
/// dispatched the handler. So a caller that answers while this is held is
/// answering before its write started, and a caller that cannot answer while this
/// is held is blocking its IPC on the write — regardless of what view it eventually
/// hands back, which is the part an assertion on the returned state cannot see:
/// `admit_apply` computes that view before the write, and awaiting does not change
/// a value already in hand.
///
/// Keyed by plan id rather than global on purpose. The Job host is process-wide and
/// shared by every test in the binary, and a gate that held *all* writes would park
/// the unrelated writes of whichever tests happened to run concurrently.
#[cfg(test)]
pub(crate) fn hold_write_closed(plan_id: &str) -> WriteGateGuard {
    let gate = Arc::new(WriteGate::default());
    write_gates()
        .lock()
        .expect("the write-gate registry only ever holds short, panic-free locks")
        .insert(plan_id.to_string(), Arc::clone(&gate));
    WriteGateGuard {
        plan_id: plan_id.to_string(),
        gate,
    }
}

/// Park until this plan's write is released, if a test is holding it closed.
#[cfg(test)]
async fn park_on_write_gate(plan_id: Option<&str>) {
    let Some(plan_id) = plan_id else {
        return;
    };
    let Some(gate) = write_gates()
        .lock()
        .ok()
        .and_then(|gates| gates.get(plan_id).cloned())
    else {
        return;
    };
    // Announced before the park, so a test can wait on this instead of assuming
    // the write has not started yet. `Notify` holds a permit for a waiter that
    // has not arrived, so the two cannot miss each other in either order.
    gate.reached.notify_one();
    // Never closed, so this cannot fail; the permit is dropped at once — what is
    // awaited is the permit being handed over, not any count of them.
    if gate.release.acquire().await.is_ok() {}
}

/// One plan's closed write: the writer announces itself, then waits to be let in.
///
/// Hand-written rather than derived: a `Semaphore` has no `Default`, because a
/// closed-write gate that defaulted to an open write would let the write it is
/// supposed to hold escape.
#[cfg(test)]
struct WriteGate {
    reached: tokio::sync::Notify,
    release: tokio::sync::Semaphore,
}

#[cfg(test)]
impl Default for WriteGate {
    fn default() -> Self {
        Self {
            reached: tokio::sync::Notify::new(),
            release: tokio::sync::Semaphore::new(0),
        }
    }
}

/// Owns one closed write.
///
/// Dropping the guard opens the write. A failing assertion therefore cannot leave a
/// detached task parked on a gate nobody holds any more, still holding the plan
/// claim the gate was blocking.
#[cfg(test)]
pub(crate) struct WriteGateGuard {
    plan_id: String,
    gate: Arc<WriteGate>,
}

#[cfg(test)]
impl WriteGateGuard {
    /// Wait until a Job is really sitting on the gate, so that "the write has not
    /// started" is an observation the test made rather than one it assumed.
    pub(crate) async fn wait_until_write_is_blocked(&self) {
        self.gate.reached.notified().await;
    }

    /// Let the blocked write proceed.
    pub(crate) fn release(&self) {
        self.gate.release.add_permits(1);
    }
}

#[cfg(test)]
impl Drop for WriteGateGuard {
    fn drop(&mut self) {
        if let Ok(mut gates) = write_gates().lock() {
            gates.remove(&self.plan_id);
        }
        self.gate.release.add_permits(1);
    }
}

/// Drive an admitted Job to a terminal state.
///
/// This is the only step that writes. It runs on its own task when the caller
/// wants the id back first, so nothing here may assume a caller is still waiting.
pub(crate) async fn drive(
    accepted: AcceptedJob,
    plan: DrivePlan,
) -> Result<JobOutcome, CommandError> {
    let host = host();
    host.clock.set(now_timestamp().as_str());
    let AcceptedJob { job_id, kind, ctx } = accepted;
    let mut registry = HandlerRegistry::new();
    registry.register(plan.handler.clone());
    let handlers = Arc::new(registry);
    let runtime = JobRuntime::new(
        host.repo.clone(),
        handlers,
        host.ledger.clone(),
        host.clock.clone() as Arc<dyn JobClock>,
        now_millis(),
    );
    let worker = WorkerId::new(format!("transfer-{kind}"));
    // The release of the write, and the only seam a test has on it: below this
    // line the handler runs, above it nothing has been written yet.
    #[cfg(test)]
    park_on_write_gate(plan.plan_id.as_deref()).await;
    let result = runtime
        .run(&ctx, &job_id, &worker, &plan.endpoints)
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "data transfer job {} could not be dispatched: {error}",
                job_id.as_str()
            ))
        })?;
    let mut artifact_ids = host
        .repo
        .get(&ctx, job_id.clone())
        .await
        .map(|record| {
            record
                .view
                .artifact_ids
                .iter()
                .map(|artifact| artifact.as_str().to_string())
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    for artifact in plan.artifact_ids {
        if !artifact_ids.contains(&artifact) {
            artifact_ids.push(artifact);
        }
    }
    remember_artifacts(job_id.as_str(), &artifact_ids);
    remember_progress(&job_id, &result.progress);
    if result.state == JobState::Succeeded {
        if let Some(plan_id) = plan.plan_id.as_deref() {
            plans::mark_plan_consumed(plan_id).map_err(CommandError::from)?;
        }
    }
    let commit_boundaries = host.repo.committed_boundaries(&job_id);
    Ok(JobOutcome {
        job_id: job_id.as_str().to_string(),
        kind,
        state: result.state,
        effect_outcome: result.effect_outcome,
        progress: result.progress,
        commit_boundaries,
        artifact_ids,
        error: result.error,
        recovery: recovery_report(
            host,
            &ctx,
            &job_id,
            plan.handler.as_ref(),
            result.progress.unknown.get(),
        ),
        replayed: false,
    })
}

/// Admit and drive in one call, for callers that genuinely wait for the verdict.
pub(crate) async fn run(request: JobRunRequest<'_>) -> Result<JobOutcome, CommandError> {
    match admit(&request).await? {
        Admission::Replayed(outcome) => Ok(outcome),
        Admission::Accepted(accepted) => drive(accepted, DrivePlan::from_request(&request)).await,
    }
}

fn lookup_receipt(host: &TransferJobHost, key: &str) -> Option<JobId> {
    host.receipts.lock().ok()?.get(key).cloned()
}

fn remember_receipt(host: &TransferJobHost, key: &str, job_id: &JobId) -> Result<(), CommandError> {
    let mut receipts = host.receipts.lock().map_err(|_| {
        CommandError::Internal("transfer job receipt store is unavailable".to_string())
    })?;
    receipts.insert(key.to_string(), job_id.clone());
    Ok(())
}

/// Replay the recorded Job for a repeated idempotency key: never a second Job.
async fn replay(
    host: &TransferJobHost,
    ctx: &RequestContext,
    job_id: &JobId,
    artifact_ids: Vec<String>,
    handler: &DataTransferHandler,
) -> Result<JobOutcome, CommandError> {
    let record = host.repo.get(ctx, job_id.clone()).await.map_err(|_| {
        CommandError::Validation(
            "the recorded idempotency receipt is no longer available; re-prepare the migration"
                .to_string(),
        )
    })?;
    let mut artifact_ids = artifact_ids;
    for artifact in host_artifacts(job_id.as_str()) {
        if !artifact_ids.contains(&artifact) {
            artifact_ids.push(artifact);
        }
    }
    // The recorded counters, not the repository's zeroed view: a replay exists
    // to hand back what the first attempt really did, and zeros would report a
    // migration that moved nothing.
    let progress = host_progress(job_id.as_str()).unwrap_or_else(|| record.view.progress.clone());
    let unknown_commits = progress.unknown.get();
    Ok(JobOutcome {
        job_id: job_id.as_str().to_string(),
        kind: record.view.kind.clone(),
        state: record.view.state,
        effect_outcome: record
            .view
            .effect_outcome
            .unwrap_or(EffectOutcome::NotStarted),
        progress,
        commit_boundaries: host.repo.committed_boundaries(job_id),
        artifact_ids,
        error: None,
        recovery: recovery_report(host, ctx, job_id, handler, unknown_commits),
        replayed: true,
    })
}

/// Ask the handler to judge the recorded checkpoint — the handler, not the host,
/// is the authority on whether a checkpoint may be resumed.
///
/// The runtime stamps every checkpoint it writes with a hardcoded
/// `recovery_policy = "resumeAfterVerify"`
/// (`packages/runtime/src/job/runtime.rs:214`), so a checkpoint that carries no
/// evidence at all would otherwise look resumable. The host therefore rebuilds
/// the evidence markers from the facts only the freeze owns — the agreed
/// policy, the snapshot proof, the unknown-commit count and the confirmed
/// boundaries — and hands that checkpoint to `verify_recovery`.
fn recovery_report(
    host: &TransferJobHost,
    ctx: &RequestContext,
    job_id: &JobId,
    handler: &DataTransferHandler,
    unknown_commits: u64,
) -> RecoveryReport {
    let boundaries = host.repo.committed_boundaries(job_id);
    let Some(mut checkpoint) = host.repo.latest_checkpoint(ctx, job_id) else {
        return RecoveryReport {
            verdict: RECOVERY_REQUIRE_MANUAL_REVIEW.to_string(),
            resume_through: None,
            reason: Some("no checkpoint was recorded, so no resume is safe".to_string()),
        };
    };
    checkpoint.verification_evidence = derive_evidence(
        handler.recovery_policy(),
        handler.snapshot_proven(),
        unknown_commits,
        &boundaries,
    );
    match handler.verify_recovery(&checkpoint) {
        RecoveryVerdict::ResumeAfterVerify { resume_through } => RecoveryReport {
            verdict: RECOVERY_RESUME_AFTER_VERIFY.to_string(),
            resume_through: Some(resume_through),
            reason: None,
        },
        RecoveryVerdict::Reject { reason } => RecoveryReport {
            verdict: RECOVERY_REJECT.to_string(),
            resume_through: None,
            reason: Some(reason),
        },
        RecoveryVerdict::RequireManualReview { reason } => RecoveryReport {
            verdict: RECOVERY_REQUIRE_MANUAL_REVIEW.to_string(),
            resume_through: None,
            reason: Some(reason),
        },
    }
}

/// Map admission failures onto the command error surface. A consumed plan or a
/// conflicting payload is a refusal, not an infrastructure fault.
pub(super) fn admit_error(error: PortError) -> CommandError {
    let message = match error {
        PortError::PlanAlreadyConsumed(plan_id) => format!(
            "plan '{plan_id}' was already consumed by an earlier apply job; re-review the migration to mint a new plan"
        ),
        PortError::IdempotencyConflict => "this idempotency key was already used with a different payload; re-prepare the migration".to_string(),
        PortError::QuotaExceeded(reason) => {
            format!("data transfer job was denied by the local budget: {reason}")
        }
        PortError::UnsupportedVersion(reason) => {
            format!("data transfer job contract is not supported: {reason}")
        }
        PortError::StaleClaim => {
            "another worker holds this data transfer job claim; retry once it settles".to_string()
        }
        PortError::NotFound(reason) => format!("data transfer job was not found: {reason}"),
        PortError::CasConflict { entity, id } => {
            format!("data transfer job {entity} '{id}' changed concurrently; retry")
        }
        PortError::TokenInvalid => "data transfer job session is no longer valid".to_string(),
        PortError::ProviderTimeout(reason) => {
            format!("data transfer job timed out before a verdict: {reason}")
        }
        PortError::ArtifactExpired => {
            "data transfer job artifact expired; re-prepare the migration".to_string()
        }
        PortError::BackendUnavailable(reason) => {
            format!("data transfer job backend is unavailable: {reason}")
        }
    };
    CommandError::Validation(message)
}
