//! Data Sync Job submission (data-migration-jobs.md §2.1, §3, CM-41).
//!
//! Every data-sync request leaves this module through exactly one of two calls:
//! [`submit_prepare`] (compare → planId + ChangeSet Artifact) or
//! [`submit_apply`] (planId + selectionRevision + confirmation). Both take the
//! same four steps, in this order:
//!
//! 1. adopt the Job id the caller already holds (the window mints it before the
//!    request, so a cancel can name a Job that does not exist yet) and register
//!    it in the pre-Job window registry, so a cancel that arrives before the Job
//!    is accepted is still reachable;
//! 2. `JobRepository::accept` — the one point that consumes a planId (CM-41)
//!    and holds the idempotency receipt, so a replayed submit never produces a
//!    second Job; a cancel that landed during `accept` is carried into the fresh
//!    record's `cancel_requested` (CM-44);
//! 3. `JobRuntime::run` with one `EndpointRef` per endpoint, after
//!    `BudgetLedger::ensure_service` registered both connections — without that
//!    the ledger denies every claim as `UnknownConnection` before work starts;
//! 4. project the terminal `JobResult` back to the caller.
//!
//! Cancel has exactly one owner: the Job record. [`cancel_job`] writes
//! `cancel_requested` there (plus the pre-Job window registry for the window
//! that has no Job yet), the runtime's per-stage watcher turns that into the
//! stage's own `CancelToken`, and the handler polls that token between keyset
//! pages and batch commits. The host keeps no second copy of the intent.
//!
//! The handler owns no Job state and the runtime carries no error text
//! (`JobResult::error` is always `None`), so the user-facing message is rebuilt
//! here from the Job state, the effect outcome and the host-recorded failure.
//! §7 forbids downgrading an `Unknown` commit to `NotStarted`, so the effect
//! outcome is carried verbatim and `NotStarted` is reported only when the Job
//! record itself proves nothing ran.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use datazen_platform_api::context::{OwnerRef, RequestContext};
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{JobDefinition, JobState};
use datazen_platform_api::id::{IdempotencyKey, JobId, WorkerId};
use datazen_platform_api::{JobRepository, PortError};
use datazen_runtime::job::{
    EndpointRef, EndpointRole, HandlerRegistry, JobClock, JobHandler, JobRuntime,
};

use crate::data_sync::job::body::{APPLY_KIND, PREPARE_KIND};
use crate::data_sync::job::host::DataSyncHost;
use crate::data_sync::job::{
    apply_payload, prepare_payload, ApplySpec, DataSyncHandler, PrepareSpec,
};
use crate::data_sync::{Endpoint, SyncSourceFilter};

use super::super::error::CommandError;
use super::super::AppState;
use super::host::recording::RecordingHost;
use super::host::{state, HostDataSync};

/// One worker identity for the whole desktop app: the Data Sync window owns the
/// Job and the Job never spawns another worker.
const WORKER: &str = "data-sync-window";

/// Terminal projection of one Data Sync Job, as the IPC layer needs it.
pub(crate) struct SyncJobOutcome {
    pub(crate) job_id: String,
    pub(crate) state: JobState,
    /// Commit effect exactly as the runtime recorded it (never downgraded).
    pub(crate) effect: EffectOutcome,
    pub(crate) committed: u64,
    /// Reconstructed user-facing reason; `None` on a clean success.
    pub(crate) message: Option<String>,
}

/// Submit the prepare Job that produces one ChangeSet Artifact.
///
/// `spec.plan_id` is honoured when the caller pre-minted it (the compare
/// command does, so its response carries the planId the apply Job will consume);
/// otherwise the handler mints one and the caller reads it back from
/// [`state::load_artifact`].
///
/// `window_job_id` is the id the caller already shows in the UI. It becomes the
/// kernel [`JobId`] itself, so [`cancel_job`] can name this Job before it exists
/// and after it is accepted, and it is registered in the pre-Job window
/// registry first so a cancel that lands before `accept` is not lost.
pub(crate) async fn submit_prepare(
    state: &AppState,
    spec: PrepareSpec,
    window_job_id: Option<String>,
) -> Result<SyncJobOutcome, CommandError> {
    let source = spec.source.clone();
    let target = spec.target.clone();
    let endpoints = endpoints_for(state, &source, &target).await?;
    let cancelled_before_job = register_window_job(window_job_id.as_deref()).await;
    let (job_id, ctx) = open_job(PREPARE_KIND, window_job_id.as_deref());
    let host = recording_host(state, source, target, spec.filters.clone(), job_id.as_str());
    let plan_id = spec.plan_id.clone().unwrap_or_default();
    let idempotency = if plan_id.is_empty() {
        IdempotencyKey::new(format!("data-sync-prepare:{job_id}"))
    } else {
        IdempotencyKey::new(format!("data-sync-prepare:{plan_id}"))
    };
    drive(
        job_id,
        ctx,
        PREPARE_KIND,
        prepare_payload(&spec),
        idempotency,
        Arc::new(DataSyncHandler::for_prepare(spec, host)),
        endpoints,
        cancelled_before_job,
    )
    .await
}

/// Submit the apply Job for one reviewed planId.
///
/// The Job carries the planId as `consumedPlanId`, so `accept` — not the legacy
/// plan registry — is what makes a second apply of the same planId impossible
/// (CM-41). Source/target come from the stored ChangeSet Artifact, which is the
/// only reviewed input; source filters are not replayed because apply executes
/// the reviewed blocks and never re-reads the source.
///
/// `window_job_id` is the id the caller already shows in the UI; see
/// [`submit_prepare`] for what adopting it buys.
pub(crate) async fn submit_apply(
    state: &AppState,
    spec: ApplySpec,
    window_job_id: Option<String>,
) -> Result<SyncJobOutcome, CommandError> {
    let artifact = state::load_artifact(&spec.plan_id).ok_or_else(|| {
        CommandError::Validation(format!(
            "plan {} does not match the stored ChangeSet; compare again",
            spec.plan_id
        ))
    })?;
    // An empty reviewed ChangeSet is refused **before** `accept`, so it costs
    // the user nothing: the planId stays unconsumed and a compare that finds
    // changes later can still apply it.
    if artifact.blocks.is_empty() {
        return Err(CommandError::Validation(format!(
            "plan {} change set is empty; compare again",
            spec.plan_id
        )));
    }
    let source = artifact.source.clone();
    let target = artifact.target.clone();
    let endpoints = endpoints_for(state, &source, &target).await?;
    let cancelled_before_job = register_window_job(window_job_id.as_deref()).await;
    let (job_id, ctx) = open_job(APPLY_KIND, window_job_id.as_deref());
    let host = recording_host(state, source, target, HashMap::new(), job_id.as_str());
    drive(
        job_id,
        ctx,
        APPLY_KIND,
        apply_payload(&spec),
        IdempotencyKey::new(format!("data-sync-apply:{}", spec.plan_id)),
        Arc::new(DataSyncHandler::for_apply(spec, host)),
        endpoints,
        cancelled_before_job,
    )
    .await
}

/// Record cancel intent for a Job id.
///
/// The Job record is the sole owner of cancel intent: `cancel_requested` here is
/// what the runtime's per-stage watcher reads while the stage is running
/// (`runtime.rs:362`), and what makes `JobRuntime::run` skip a Job cancelled
/// before it started (CM-44). The pre-Job window registry is written too,
/// because the window can cancel an id that has not been accepted yet —
/// `submit_*` carries that intent into the record at accept time.
///
/// An unknown id still reports success, because the window registry accepted
/// the click; the repository side simply has nothing left to mark.
pub(crate) async fn cancel_job(job_id: &str) -> bool {
    let window = crate::services::job_registry::cancel_job(job_id).await;
    let ctx = state::desktop_context();
    let kernel = match state::repository().request_cancel(&ctx, &JobId::new(job_id.to_string())) {
        Ok(record) => record.view.cancel_requested,
        Err(error) => {
            tracing::debug!(job_id, %error, "no live job record to mark as cancelled");
            false
        }
    };
    window || kernel
}

/// Register the caller's id in the pre-Job window registry and report whether
/// the user had already cancelled it by the time this submission arrived.
///
/// Reading the flag here — once, into a `bool` — is deliberate: the Job's cancel
/// channel is the kernel record, so no flag handle travels any further into the
/// submission path.
async fn register_window_job(window_job_id: Option<&str>) -> bool {
    match window_job_id {
        Some(id) => crate::services::job_registry::ensure_job(id)
            .await
            .load(Ordering::SeqCst),
        None => false,
    }
}

/// Adopt the Job id this submission runs under, and hand back the context the
/// repository calls will be made under.
///
/// A caller-supplied id is the id the user (or the window) cancels by, so the
/// Job has to *be* that id: minting a second one would leave the cancel and the
/// Job in different address spaces. A caller with no id (the internal apply
/// path) still gets a unique one.
fn open_job(kind: &'static str, window_job_id: Option<&str>) -> (JobId, RequestContext) {
    let ctx = state::desktop_context();
    let job_id = match window_job_id {
        Some(id) if !id.is_empty() => JobId::new(id.to_string()),
        _ => JobId::new(uuid::Uuid::new_v4().to_string()),
    };
    tracing::info!(%job_id, kind, "data sync job submitted");
    (job_id, ctx)
}

/// The desktop host wrapped in the failure recorder. `RecordingHost` is the
/// only production writer of `state::FAILURES`, which is what makes the IPC
/// message evidence-based instead of a reconstruction.
///
/// The host is constructed without any cancel handle:
/// `DataSyncHost::table_reader` and `::target_executor` take the running stage's
/// `CancelToken`, so the bit the keyset walk and the batch executor poll is the
/// kernel's own.
fn recording_host(
    state: &AppState,
    source: Endpoint,
    target: Endpoint,
    filters: HashMap<String, SyncSourceFilter>,
    job_id: &str,
) -> Arc<dyn DataSyncHost> {
    let host = HostDataSync::new(
        state.connection_manager.clone(),
        state.sync_adapters.clone(),
        source,
        target,
        filters,
    );
    Arc::new(RecordingHost::new(Arc::new(host), job_id))
}

async fn endpoint_ref(
    state: &AppState,
    endpoint: &Endpoint,
    role: EndpointRole,
) -> Result<EndpointRef, CommandError> {
    let identity = crate::services::migration_endpoint::session_identity(
        &state.connection_manager,
        &endpoint.connection_id,
        Some((&endpoint.database, endpoint.schema.as_deref())),
    )
    .await?;
    Ok(EndpointRef {
        connection_id: identity.connection_id,
        service_key: identity.service_key,
        objects: vec![endpoint.database.clone()],
        role,
    })
}

/// The pair as the budget sees it: the source is the reader, the target the
/// writer, and a self-sync pair stays two endpoints under one service key so
/// the overlap detector can refuse it instead of silently self-applying.
async fn endpoints_for(
    state: &AppState,
    source: &Endpoint,
    target: &Endpoint,
) -> Result<Vec<EndpointRef>, CommandError> {
    Ok(vec![
        endpoint_ref(state, source, EndpointRole::SourceReader).await?,
        endpoint_ref(state, target, EndpointRole::TargetWriter).await?,
    ])
}

/// accept → run → project, shared by prepare and apply.
///
/// `cancelled_before_job` is the cancel the user gave before this Job existed.
/// It is carried into the accepted record here — the only point where the
/// window registry and the Job record can meet — so CM-44 keeps working after
/// the host-side flag store is gone: the record starts out `cancel_requested`,
/// and `JobRuntime::run` confirms `Cancelled` / `NotStarted` without entering
/// budget or dispatch, so nothing runs at all (strictly stronger than the old
/// flag copy, which could still let a first batch start).
#[allow(clippy::too_many_arguments)]
async fn drive(
    job_id: JobId,
    ctx: RequestContext,
    kind: &'static str,
    payload: serde_json::Value,
    idempotency: IdempotencyKey,
    handler: Arc<dyn JobHandler>,
    endpoints: Vec<EndpointRef>,
    cancelled_before_job: bool,
) -> Result<SyncJobOutcome, CommandError> {
    let plan_id = payload
        .get("consumedPlanId")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let mut handlers = HandlerRegistry::new();
    handlers.register(handler);
    let runtime = JobRuntime::new(
        state::repository(),
        Arc::new(handlers),
        state::ledger(),
        state::clock(),
        state::now_ms(),
    );
    let definition = JobDefinition {
        job_id: job_id.clone(),
        kind: kind.to_string(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ctx.client_instance_id.clone(),
            purpose: format!("data-sync:{kind}"),
        },
        payload,
        created_at: state::clock().now(),
    };

    // CM-41: accept is the one consumption point for a planId. A replay of the
    // same request returns the original record instead of creating a second
    // Job, and a second consumption is refused with `PlanAlreadyConsumed`.
    let accepted = match state::repository()
        .accept(&ctx, definition, &idempotency)
        .await
    {
        Ok(record) => record,
        Err(error) => {
            state::forget(job_id.as_str());
            return Err(port_error(error));
        }
    };
    // A replay is the observable form of single consumption: the repository
    // returns the *first* Job for this idempotency key instead of accepting a
    // second one, so the accepted record carries someone else's job id. The
    // loser of two concurrent applies lands here and runs nothing — its own
    // job id was never accepted, so `runtime.run` would only report
    // `NotFound` for a Job that does not exist (CM-41, §2.1).
    if accepted.view.job_id != job_id {
        state::forget(job_id.as_str());
        return Err(CommandError::Validation(match plan_id {
            Some(plan) => format!("plan {plan} was already submitted for apply; compare again"),
            None => "this data sync job was already submitted; compare again".to_string(),
        }));
    }
    if accepted.view.state != JobState::Queued {
        state::forget(job_id.as_str());
        return Err(CommandError::Validation(match plan_id {
            Some(plan) => {
                format!("plan {plan} was already submitted for apply; compare again")
            }
            None => "this data sync job was already submitted; compare again".to_string(),
        }));
    }

    // CM-44: the window can cancel before `accept`, when there is still no
    // record to mark. The accepted record is the first place that intent can
    // live, so it is written before `run` reads it back. A failure here is not
    // silently swallowed into a *running* Job: `run` re-reads the record, so
    // the observable effect of a missed seed is exactly the old behaviour.
    if cancelled_before_job {
        if let Err(error) = state::repository().request_cancel(&ctx, &job_id) {
            tracing::warn!(%job_id, %error, "could not carry a pre-job cancel into the job record");
        }
    }

    {
        let ledger = state::ledger();
        let mut ledger = state::lock(ledger.as_ref());
        for endpoint in &endpoints {
            ledger.ensure_service(&endpoint.connection_id);
        }
    }

    let outcome = match runtime
        .run(&ctx, &job_id, &WorkerId::new(WORKER), &endpoints)
        .await
    {
        Ok(result) => project(
            &job_id,
            kind,
            result.state,
            result.effect_outcome,
            result.progress.committed.get(),
        ),
        Err(error) => match recorded_outcome(&ctx, &job_id, kind).await {
            // §7: a run that refused to project a result may still have
            // committed something. Only a Job whose recorded effect is
            // NotStarted may be reported as "never started".
            Some(outcome) if outcome.effect != EffectOutcome::NotStarted => outcome,
            _ => {
                state::forget(job_id.as_str());
                return Err(port_error(error));
            }
        },
    };
    tracing::info!(
        job_id = %outcome.job_id,
        kind,
        state = ?outcome.state,
        effect = ?outcome.effect,
        committed = outcome.committed,
        "data sync job finished"
    );
    state::forget(job_id.as_str());
    Ok(outcome)
}

/// Read the committed effect back from the repository, for the rare run that
/// returns `Err` after the handler already crossed a commit boundary.
async fn recorded_outcome(
    ctx: &RequestContext,
    job_id: &JobId,
    kind: &'static str,
) -> Option<SyncJobOutcome> {
    let record = state::repository().get(ctx, job_id.clone()).await.ok()?;
    let effect = record.view.effect_outcome?;
    Some(project(
        job_id,
        kind,
        record.view.state,
        effect,
        record.view.progress.committed.get(),
    ))
}

fn project(
    job_id: &JobId,
    kind: &'static str,
    job_state: JobState,
    effect: EffectOutcome,
    committed: u64,
) -> SyncJobOutcome {
    let job_id = job_id.as_str().to_string();
    let message = match job_state {
        JobState::Succeeded => None,
        JobState::Cancelled => Some(cancelled_message(reason_for(&job_id))),
        JobState::Failed => Some(reason_for(&job_id)),
        JobState::Queued | JobState::Running => Some(format!(
            "{kind} job {job_id} was still {job_state:?} when the request returned"
        )),
    };
    SyncJobOutcome {
        job_id,
        state: job_state,
        effect,
        committed,
        message,
    }
}

/// The cancellation text is a contract: `execution_response_and_cancelled`
/// recognises a run as cancelled only by this prefix.
fn cancelled_message(reason: String) -> String {
    if reason.starts_with("execute cancelled") {
        reason
    } else {
        format!("execute cancelled: {reason}")
    }
}

/// `JobResult::error` is always `None`, so the reason comes from the host's
/// failure record and only falls back to a projection when the host never
/// failed (a cancel, or a terminal state set by the runtime itself).
fn reason_for(job_id: &str) -> String {
    state::failure_of(job_id).unwrap_or_else(|| {
        "the job stopped without a host-recorded reason; compare again before applying".to_string()
    })
}

/// Rejections stay rejections. `PlanAlreadyConsumed` names the plan the user has
/// to compare again; every other port failure is a refusal that happened before
/// any batch ran.
fn port_error(error: PortError) -> CommandError {
    match error {
        PortError::PlanAlreadyConsumed(plan_id) => CommandError::Validation(format!(
            "plan {plan_id} was already submitted for apply; compare again"
        )),
        other => CommandError::Validation(other.to_string()),
    }
}

/// `remove_job` is on the production cleanup path of `execute_data_sync_impl`.
pub(crate) use crate::services::job_registry::remove_job;

/// The legacy statement path only survives for the cfg(test) command harness in
/// `exec`, so this re-export is test-only; the production apply path reads the
/// window flag through the kernel Job record instead.
#[cfg(test)]
pub(crate) use crate::services::job_registry::ensure_job;
