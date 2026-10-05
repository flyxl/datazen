//! Accept and run one Data Transfer Job through the P5 Job runtime.
//!
//! §2.1/§8 contract enforced here:
//!
//! * prepare and apply are distinct Jobs; apply carries only `planId` + plan digest +
//!   selection revision + reviewed selection + confirmation — never the plan body;
//! * one `planId` is consumable by exactly one apply Job: the repository rejects a
//!   second accept with `PlanAlreadyConsumed`, and the host marks the plan consumed
//!   only after a terminal success;
//! * a repeated idempotency key replays the *recorded* Job instead of creating a
//!   second one, so an unknown commit never becomes a second write.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{CommitBoundary, JobDefinition, JobProgress, JobState};
use datazen_platform_api::id::{
    ClientInstanceId, ConnectionId, IdempotencyKey, JobId, OrganizationId, PrincipalId, RequestId,
    Timestamp, WorkerId,
};
use datazen_platform_api::{JobRepository, PortError, RequestContext};
use datazen_runtime::budget::{BudgetConfig, BudgetLedger};
use datazen_runtime::job::{
    EndpointRef, EndpointRole, HandlerRegistry, InMemoryJobRepository, JobClock, JobRuntime,
    SharedClock,
};

use super::super::plans;
use crate::commands::error::CommandError;
use crate::data_transfer::job::DataTransferHandler;

pub(crate) const PREPARE_KIND: &str = "dataTransferPrepare";
pub(crate) const APPLY_KIND: &str = "dataTransferApply";
/// One service key for both endpoints: a reader/writer object overlap under a
/// single key is then a hard refusal instead of two independent budgets.
const TRANSFER_SERVICE_KEY: &str = "data-transfer";
const CLAIM_TTL_SECS: i64 = 300;
const LOCAL_CLIENT_INSTANCE: &str = "datazen-local-client";

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
    /// True when an idempotency key replayed an already accepted Job.
    pub(crate) replayed: bool,
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

struct TransferJobHost {
    repo: Arc<InMemoryJobRepository>,
    ledger: Arc<Mutex<BudgetLedger>>,
    clock: Arc<SharedClock>,
    /// idempotency key → accepted Job id. A replay never mints a second Job (§8).
    receipts: Mutex<HashMap<String, JobId>>,
}

/// Process-wide Job host. State is in-memory like the existing connection
/// sessions: this host *is* the local desktop backend (§8).
fn host() -> &'static TransferJobHost {
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

/// Local desktop identity: §8 forbids a background service from reaching this
/// profile, so the context carries no remote principal and no session token.
fn request_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("datazen-local"),
        PrincipalId::new("datazen-local-user"),
        None,
        ClientInstanceId::new(LOCAL_CLIENT_INSTANCE),
        RequestId::new(format!("transfer-{}", uuid::Uuid::new_v4())),
        None,
    )
}

/// Endpoint refs for a frozen run: one reader, one writer, one service key.
/// A SQL-file target has no writable endpoint, so only the reader is reserved.
pub(crate) fn endpoint_refs(
    source_objects: Vec<String>,
    target_objects: Vec<String>,
    sql_file: bool,
) -> Vec<EndpointRef> {
    let mut refs = vec![EndpointRef {
        connection_id: ConnectionId::new("local-source"),
        service_key: TRANSFER_SERVICE_KEY.to_string(),
        objects: source_objects,
        role: EndpointRole::SourceReader,
    }];
    if !sql_file {
        refs.push(EndpointRef {
            connection_id: ConnectionId::new("local-target"),
            service_key: TRANSFER_SERVICE_KEY.to_string(),
            objects: target_objects,
            role: EndpointRole::TargetWriter,
        });
    }
    refs
}

/// Accept (or replay) and run one Job until it reaches a terminal state.
pub(crate) async fn run(request: JobRunRequest<'_>) -> Result<JobOutcome, CommandError> {
    let host = host();
    host.clock.set(now_timestamp().as_str());
    let ctx = request_context();
    if let Some(receipt) = lookup_receipt(host, request.idempotency_key) {
        return replay(host, &ctx, &receipt, request.artifact_ids).await;
    }
    let mut registry = HandlerRegistry::new();
    registry.register(request.handler.clone());
    let handlers = Arc::new(registry);
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
    let runtime = JobRuntime::new(
        host.repo.clone(),
        handlers,
        host.ledger.clone(),
        host.clock.clone() as Arc<dyn JobClock>,
        now_millis(),
    );
    let worker = WorkerId::new(format!("transfer-{}", request.kind));
    let result = runtime
        .run(&ctx, &job_id, &worker, &request.endpoints)
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
    for artifact in request.artifact_ids {
        if !artifact_ids.contains(&artifact) {
            artifact_ids.push(artifact);
        }
    }
    if result.state == JobState::Succeeded {
        if let Some(plan_id) = request.plan_id {
            plans::mark_plan_consumed(plan_id).map_err(CommandError::from)?;
        }
    }
    Ok(JobOutcome {
        job_id: job_id.as_str().to_string(),
        kind: record.view.kind.clone(),
        state: result.state,
        effect_outcome: result.effect_outcome,
        progress: result.progress,
        commit_boundaries: host.repo.committed_boundaries(&job_id),
        artifact_ids,
        error: result.error,
        replayed: false,
    })
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
) -> Result<JobOutcome, CommandError> {
    let record = host.repo.get(ctx, job_id.clone()).await.map_err(|_| {
        CommandError::Validation(
            "the recorded idempotency receipt is no longer available; re-prepare the migration"
                .to_string(),
        )
    })?;
    Ok(JobOutcome {
        job_id: job_id.as_str().to_string(),
        kind: record.view.kind.clone(),
        state: record.view.state,
        effect_outcome: record
            .view
            .effect_outcome
            .unwrap_or(EffectOutcome::NotStarted),
        progress: record.view.progress.clone(),
        commit_boundaries: host.repo.committed_boundaries(job_id),
        artifact_ids,
        error: None,
        replayed: true,
    })
}

/// Map admission failures onto the command error surface. A consumed plan or a
/// conflicting payload is a refusal, not an infrastructure fault.
fn admit_error(error: PortError) -> CommandError {
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
