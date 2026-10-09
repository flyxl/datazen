//! Durable Data Sync Job lifecycle.
//!
//! Production commands accept and dispatch through `DesktopJobHost`; runtime
//! endpoint refs use the physical identity from `migration_endpoint` and never
//! include a `dbSessionId`. A bounded in-process receipt fence prevents a
//! same-process IPC retry from dispatching an accepted Job twice. The legacy
//! in-memory submission helpers below are compiled only for unit contracts.

use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use datazen_platform_api::context::{OwnerRef, RequestContext};
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{
    JobDefinition, JobRecoveryResult, JobRecoveryVerdict, JobState, JobView,
};
use datazen_platform_api::id::{
    ClientInstanceId, IdempotencyKey, JobId, OrganizationId, PrincipalId, RequestId, WorkerId,
};
#[cfg(test)]
use datazen_platform_api::JobRepository;
use datazen_platform_api::PortError;
use datazen_runtime::job::{EndpointRef, JobHandler};
#[cfg(test)]
use datazen_runtime::job::{EndpointRole, HandlerRegistry, JobClock, JobRuntime};
use futures_util::FutureExt;
#[cfg(test)]
use std::sync::atomic::Ordering;

#[cfg(test)]
use super::plans::SyncRunRequest;
pub(crate) use crate::data_sync::job::body::{APPLY_KIND, PREPARE_KIND};
use crate::data_sync::job::host::DataSyncHost;
#[cfg(test)]
use crate::data_sync::job::{prepare_payload, ApplySpec, DataSyncHandler, PrepareSpec};
use crate::data_sync::{Endpoint, SyncSourceFilter};

use super::super::error::CommandError;
use super::super::AppState;
use super::host::recording::RecordingHost;
#[cfg(test)]
use super::host::state;
use super::host::HostDataSync;

/// One worker identity for the whole desktop app: the Data Sync window owns the
/// Job and the Job never spawns another worker.
const WORKER: &str = "data-sync-window";

const LOCAL_CLIENT_INSTANCE: &str = "datazen-local-client";

/// Admission result for a durable desktop job. `dispatch` is false when the
/// idempotency receipt already points at an existing job; replay never starts
/// a second worker.
pub(crate) struct DurableAdmission {
    pub(crate) job_id: JobId,
    pub(crate) view: JobView,
    pub(crate) ctx: RequestContext,
    pub(crate) dispatch: bool,
    dispatch_claim: Option<DispatchClaim>,
}

pub(crate) fn durable_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("datazen-local"),
        PrincipalId::new("datazen-local-user"),
        None,
        ClientInstanceId::new(LOCAL_CLIENT_INSTANCE),
        RequestId::new(format!("data-sync-{}", uuid::Uuid::new_v4())),
        None,
    )
}

/// Persist the sanitized plan and its idempotency receipt before a caller may
/// dispatch. Runtime endpoint references are held only for admission and the
/// in-memory handler binding.
pub(crate) async fn admit_durable(
    state: &AppState,
    job_id: &str,
    kind: &'static str,
    payload: serde_json::Value,
    idempotency_key: &str,
    endpoints: &[EndpointRef],
) -> Result<DurableAdmission, CommandError> {
    let ctx = durable_context();
    state
        .desktop_job_host
        .ensure_endpoint_services(endpoints)
        .map_err(port_error)?;
    let requested_job_id = JobId::new(job_id.to_string());
    let definition = JobDefinition {
        job_id: requested_job_id.clone(),
        kind: kind.to_string(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new(LOCAL_CLIENT_INSTANCE),
            purpose: format!("data-sync:{kind}"),
        },
        payload,
        created_at: state.desktop_job_host.now(),
    };
    let mut record = state
        .desktop_job_host
        .accept(
            &ctx,
            definition,
            &IdempotencyKey::new(idempotency_key.to_string()),
        )
        .await
        .map_err(port_error)?;
    if consume_preaccept_cancel(job_id) {
        record = state
            .desktop_job_host
            .cancel(&ctx, &requested_job_id)
            .await
            .map_err(port_error)?;
    }
    let dispatch_claim =
        if record.view.job_id == requested_job_id && record.view.state == JobState::Queued {
            DispatchClaim::acquire(record.view.job_id.as_str())
        } else {
            None
        };
    let dispatch = dispatch_claim.is_some();
    Ok(DurableAdmission {
        job_id: record.view.job_id.clone(),
        view: record.view,
        ctx,
        dispatch,
        dispatch_claim,
    })
}

/// Dispatch an already accepted sync job. A panic or runtime error is
/// converted into durable not-started/pending-verification evidence; callers
/// own and release their dedicated sessions around this future.
pub(crate) async fn dispatch_durable(
    state: &AppState,
    admission: &DurableAdmission,
    endpoints: &[EndpointRef],
    handler: Arc<dyn JobHandler>,
) {
    if !admission.dispatch || admission.dispatch_claim.is_none() {
        return;
    }
    let host = state.desktop_job_host.clone();
    let worker = WorkerId::new(WORKER);
    let dispatch = AssertUnwindSafe(host.dispatch(
        &admission.ctx,
        &admission.job_id,
        &worker,
        endpoints,
        handler,
    ))
    .catch_unwind()
    .await;
    match dispatch {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            record_dispatch_failure(&host, &admission.ctx, &admission.job_id, &error.to_string())
                .await;
        }
        Err(_) => {
            record_dispatch_failure(&host, &admission.ctx, &admission.job_id, "handlerPanicked")
                .await;
        }
    }
}

static ACTIVE_DISPATCHES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

static PREACCEPT_CANCELS: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn remember_preaccept_cancel(job_id: &str) {
    let now = Instant::now();
    let mut cancellations = PREACCEPT_CANCELS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cancellations.retain(|_, expires_at| *expires_at > now);
    if cancellations.len() >= 512 {
        if let Some(oldest) = cancellations
            .iter()
            .min_by_key(|(_, expires_at)| *expires_at)
            .map(|(job_id, _)| job_id.clone())
        {
            cancellations.remove(&oldest);
        }
    }
    cancellations.insert(job_id.to_string(), now + Duration::from_secs(120));
}

fn consume_preaccept_cancel(job_id: &str) -> bool {
    let now = Instant::now();
    PREACCEPT_CANCELS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(job_id)
        .is_some_and(|expires_at| expires_at > now)
}

struct DispatchClaim(String);

impl DispatchClaim {
    fn acquire(job_id: &str) -> Option<Self> {
        let claims = ACTIVE_DISPATCHES.get_or_init(|| Mutex::new(HashSet::new()));
        let mut claims = claims
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        claims
            .insert(job_id.to_string())
            .then(|| Self(job_id.to_string()))
    }
}

impl Drop for DispatchClaim {
    fn drop(&mut self) {
        if let Some(claims) = ACTIVE_DISPATCHES.get() {
            claims
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&self.0);
        }
    }
}

async fn record_dispatch_failure(
    host: &datazen_runtime::job::DesktopJobHost,
    ctx: &RequestContext,
    job_id: &JobId,
    _reason: &str,
) {
    let repository = host.repository();
    let Ok(record) = host.get(ctx, job_id.clone()).await else {
        return;
    };
    match record.view.state {
        JobState::Queued => {
            let _ = repository
                .mark_failed_unstarted(ctx, job_id, "dispatchNotStarted")
                .await;
        }
        JobState::Running => {
            if repository
                .mark_pending_verification(ctx, job_id, "dispatchNeedsVerification")
                .await
                .is_ok()
            {
                let _ = repository
                    .persist_recovery(
                        ctx,
                        job_id,
                        JobRecoveryResult {
                            verdict: JobRecoveryVerdict::PendingVerification,
                            resume_through: None,
                            reason_code: Some("dispatchNeedsVerification".into()),
                        },
                    )
                    .await;
            }
        }
        JobState::Succeeded | JobState::Failed | JobState::Cancelled => {}
    }
}

pub(crate) async fn read_durable_job(
    state: &AppState,
    job_id: &str,
) -> Result<JobView, CommandError> {
    let ctx = durable_context();
    state
        .desktop_job_host
        .get(&ctx, JobId::new(job_id.to_string()))
        .await
        .map(|record| {
            let mut view = record.view;
            if view.state == JobState::Failed {
                if let Some(message) = state::safe_failure_of(job_id) {
                    view.error = Some(message);
                }
            }
            view
        })
        .map_err(port_error)
}

pub(crate) async fn cancel_durable_job(
    state: &AppState,
    job_id: &str,
) -> Result<bool, CommandError> {
    let ctx = durable_context();
    match state
        .desktop_job_host
        .cancel(&ctx, &JobId::new(job_id.to_string()))
        .await
    {
        Ok(record) => Ok(record.view.cancel_requested || record.view.state == JobState::Cancelled),
        Err(PortError::NotFound(_)) => {
            remember_preaccept_cancel(job_id);
            Ok(true)
        }
        Err(error) => Err(port_error(error)),
    }
}

pub(crate) async fn list_durable_jobs(state: &AppState) -> Result<Vec<JobView>, CommandError> {
    let ctx = durable_context();
    let records = state
        .desktop_job_host
        .list(
            &ctx,
            datazen_platform_api::dto::job::JobFilter {
                states: Vec::new(),
                owner: None,
                after: None,
                limit: Some(100),
            },
        )
        .await
        .map_err(port_error)?;
    Ok(records
        .into_iter()
        .filter(|record| matches!(record.view.kind.as_str(), PREPARE_KIND | APPLY_KIND))
        .map(|record| record.view)
        .collect())
}

/// Resolve the persisted single-consumption receipt for a reviewed plan.
/// This lets an IPC retry return the original job even after the process-local
/// comparison plan has been consumed and removed.
pub(crate) async fn find_apply_receipt_for_plan(
    state: &AppState,
    plan_id: &str,
) -> Result<Option<JobView>, CommandError> {
    let ctx = durable_context();
    let records = state
        .desktop_job_host
        .list(
            &ctx,
            datazen_platform_api::dto::job::JobFilter {
                states: Vec::new(),
                owner: None,
                after: None,
                limit: None,
            },
        )
        .await
        .map_err(port_error)?;
    Ok(records
        .into_iter()
        .find(|record| {
            record.view.kind == APPLY_KIND
                && record
                    .definition
                    .payload
                    .get("consumedPlanId")
                    .and_then(serde_json::Value::as_str)
                    == Some(plan_id)
        })
        .map(|record| record.view))
}

pub(crate) async fn wait_for_durable_terminal(
    state: &AppState,
    job_id: &str,
) -> Result<JobView, CommandError> {
    loop {
        let view = read_durable_job(state, job_id).await?;
        if view.state.is_terminal() {
            return Ok(view);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

pub(crate) async fn details_durable_job(
    state: &AppState,
    job_id: &str,
) -> Result<datazen_platform_api::dto::job::JobDetails, CommandError> {
    let ctx = durable_context();
    let mut details = state
        .desktop_job_host
        .details(&ctx, JobId::new(job_id.to_string()))
        .await
        .map_err(port_error)?;
    if !matches!(details.job.kind.as_str(), PREPARE_KIND | APPLY_KIND) {
        return Err(CommandError::NotFound(format!(
            "Data Sync job '{job_id}' was not found"
        )));
    }
    if details.job.state == JobState::Failed {
        if let Some(message) = state::safe_failure_of(job_id) {
            details.job.error = Some(message);
        }
    }
    Ok(details)
}

pub(crate) async fn verify_durable_recovery(
    state: &AppState,
    job_id: &str,
    verifier: Arc<dyn datazen_runtime::job::JobRecoveryVerifier>,
) -> Result<datazen_platform_api::dto::job::JobDetails, CommandError> {
    let ctx = durable_context();
    state
        .desktop_job_host
        .verify_recovery(&ctx, JobId::new(job_id.to_string()), verifier)
        .await
        .map_err(port_error)
}

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
#[cfg(test)]
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

/// Test adapter that exercises the production durable apply path for one
/// reviewed planId. The fixture's authorized source and target sessions come
/// from the live plan; production then creates and releases dedicated sessions.
/// A repeated call resolves the durable consumed-plan receipt rather than
/// entering a legacy in-memory executor.
///
/// `window_job_id` is the id the caller already shows in the UI; see
/// [`submit_prepare`] for what adopting it buys.
#[cfg(test)]
pub(crate) async fn submit_apply(
    state: &AppState,
    spec: ApplySpec,
    window_job_id: Option<String>,
) -> Result<SyncJobOutcome, CommandError> {
    let (source_session_id, target_session_id) = super::plans::peek_plan(&spec.plan_id)
        .map(|plan| (plan.source_db_session_id, plan.target_db_session_id))
        .unwrap_or_default();
    let selection = state::confirmed_selection(&spec.plan_id).ok_or_else(|| {
        CommandError::Validation("reviewed selection is unavailable; compare again".into())
    })?;
    let job_id = window_job_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("data-sync-{}", uuid::Uuid::new_v4()));
    let view = super::exec::start_data_sync_apply_job_impl(
        state,
        source_session_id,
        target_session_id,
        SyncRunRequest {
            plan_id: spec.plan_id,
            selection: selection.selection,
            options: spec.options,
            job_id: Some(job_id),
        },
    )
    .await?;
    let finished = wait_for_durable_terminal(state, view.job_id.as_str()).await?;
    let details = details_durable_job(state, finished.job_id.as_str()).await?;
    let message = match finished.state {
        JobState::Succeeded => None,
        JobState::Cancelled => Some(
            state::failure_of(finished.job_id.as_str())
                .or(finished.error)
                .unwrap_or_else(|| "execute cancelled".into()),
        ),
        JobState::Failed => Some(
            state::failure_of(finished.job_id.as_str())
                .or(details.job.error)
                .unwrap_or_else(|| {
                    "the Data Sync apply job failed; compare current data before continuing".into()
                }),
        ),
        JobState::Queued | JobState::Running => Some(format!(
            "apply job {} was still running when the request returned",
            finished.job_id
        )),
    };
    Ok(SyncJobOutcome {
        job_id: finished.job_id.as_str().to_string(),
        state: finished.state,
        effect: finished.effect_outcome.unwrap_or(EffectOutcome::NotStarted),
        committed: finished.progress.committed.get(),
        message,
    })
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
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
pub(crate) fn recording_host(
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

#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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

#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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

/// Test harnesses still assert the legacy registry cleanup behavior.
#[cfg(test)]
pub(crate) use crate::services::job_registry::remove_job;

/// The legacy statement path only survives for the cfg(test) command harness in
/// `exec`, so this re-export is test-only; the production apply path reads the
/// window flag through the kernel Job record instead.
#[cfg(test)]
pub(crate) use crate::services::job_registry::ensure_job;
