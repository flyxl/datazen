//! Restore is an apply Job, with an immutable, authorized Artifact input.
//! Drivers must implement a real Gateway binding; no legacy-handle fallback exists.
use datazen_platform_api::{
    dto::{
        artifact::ArtifactLifecycle,
        execution::{EffectOutcome, ExecutionErrorCode},
        job::{Checkpoint, CommitBoundary, JobDomainResult, JobProgress},
    },
    id::{ArtifactId, Counter, ExecutionId, StageId},
    ports::artifact::ArtifactStore,
    target::ExecutionTarget,
    RequestContext,
};
use datazen_runtime::job::{
    CancelToken, FrozenPlan, JobError, JobHandler, RecoveryVerdict, StageOutcome, StageSpec,
    StageTerminal,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

pub const RESTORE_APPLY_KIND: &str = "databaseRestoreApply";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestorePlan {
    pub plan_id: String,
    pub target: ExecutionTarget,
    pub config_revision: Counter,
    pub artifact_id: ArtifactId,
    pub input_sha256: String,
    pub input_bytes: Counter,
    pub single_transaction: bool,
    pub overwrite: bool,
}
impl RestorePlan {
    /// Includes connection and every submitted namespace/object field.
    pub fn target_fingerprint(target: &ExecutionTarget) -> String {
        // ExecutionTarget contains only infallible serde data types.
        let encoded = serde_json::json!(target).to_string();
        format!("restore:v1:{:x}", Sha256::digest(encoded.as_bytes()))
    }
    pub fn payload(&self) -> serde_json::Value {
        serde_json::json!({"consumedPlanId":self.plan_id,"planVersion":1,"handlerVersion":1,"checkpointVersion":1,"selectionRevision":self.config_revision.get(),"restorePlan":self})
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreRejection {
    Unsupported,
    NotAuthorized,
    InvalidInput,
    RuntimeUnavailable,
    AdmissionRejected,
    TargetMismatch,
}
#[async_trait::async_trait]
pub trait RestoreInput: Send + Sync {
    /// Every read rechecks Artifact ACL/TTL. No filesystem path crosses this port.
    async fn chunk(&self, index: Counter) -> Result<Vec<u8>, RestoreRejection>;
    fn chunks(&self) -> u64;
}
/// Trusted Gateway completion evidence; never infer rollback from a cancel request.
pub struct RestoreEvidence {
    /// Only for rejection before submission; never use after any external write.
    pub rejection: Option<RestoreRejection>,
    pub terminal: StageTerminal,
    pub effects: EffectOutcome,
    pub progress: JobProgress,
    pub executions: Vec<ExecutionId>,
    pub boundaries: Vec<CommitBoundary>,
}
#[async_trait::async_trait]
pub trait RestoreGateway: Send + Sync {
    /// Reauthorize target/revision/capability before acquiring any resource.
    async fn validate(
        &self,
        context: &RequestContext,
        plan: &RestorePlan,
    ) -> Result<(), RestoreRejection>;
    /// Must use a counted, explicit Gateway target. Errors after submission must
    /// return Unknown evidence rather than a pre-dispatch rejection.
    async fn execute(
        &self,
        context: &RequestContext,
        plan: &RestorePlan,
        input: Arc<dyn RestoreInput>,
        cancel: &CancelToken,
    ) -> RestoreEvidence;
}
struct ArtifactInput {
    store: Arc<dyn ArtifactStore>,
    context: RequestContext,
    artifact: ArtifactId,
    chunks: u64,
}
#[async_trait::async_trait]
impl RestoreInput for ArtifactInput {
    async fn chunk(&self, index: Counter) -> Result<Vec<u8>, RestoreRejection> {
        if index.get() >= self.chunks {
            return Err(RestoreRejection::InvalidInput);
        }
        self.store
            .read_chunk(&self.context, self.artifact.clone(), index)
            .await
            .map(|chunk| chunk.bytes)
            .map_err(|_| RestoreRejection::NotAuthorized)
    }
    fn chunks(&self) -> u64 {
        self.chunks
    }
}
pub struct RestoreApplyJob {
    context: RequestContext,
    plan: RestorePlan,
    artifacts: Arc<dyn ArtifactStore>,
    gateway: Arc<dyn RestoreGateway>,
    rejection: Mutex<Option<RestoreRejection>>,
}
impl RestoreApplyJob {
    pub fn new(
        context: RequestContext,
        plan: RestorePlan,
        artifacts: Arc<dyn ArtifactStore>,
        gateway: Arc<dyn RestoreGateway>,
    ) -> Self {
        Self {
            context,
            plan,
            artifacts,
            gateway,
            rejection: Mutex::new(None),
        }
    }
    async fn input(&self, cancel: &CancelToken) -> Result<Arc<dyn RestoreInput>, RestoreRejection> {
        let metadata = self
            .artifacts
            .describe(&self.context, self.plan.artifact_id.clone())
            .await
            .map_err(|_| RestoreRejection::NotAuthorized)?;
        if metadata.target_fingerprint != RestorePlan::target_fingerprint(&self.plan.target)
            || metadata.lifecycle != ArtifactLifecycle::Complete
            || metadata.published_byte_size != self.plan.input_bytes
        {
            return Err(RestoreRejection::InvalidInput);
        }
        let input = Arc::new(ArtifactInput {
            store: self.artifacts.clone(),
            context: self.context.clone(),
            artifact: self.plan.artifact_id.clone(),
            chunks: metadata.published_chunk_count.get(),
        });
        let mut hash = Sha256::new();
        for index in 0..input.chunks() {
            if cancel.is_cancelled() {
                return Err(RestoreRejection::InvalidInput);
            }
            hash.update(input.chunk(Counter::new(index)).await?);
        }
        if format!("{:x}", hash.finalize()) != self.plan.input_sha256 {
            return Err(RestoreRejection::InvalidInput);
        }
        Ok(input)
    }
    fn rejected(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
        reason: RestoreRejection,
    ) -> StageOutcome {
        if let Ok(mut rejection) = self.rejection.lock() {
            *rejection = Some(reason);
        }
        StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal: if cancel.is_cancelled() {
                StageTerminal::Cancelled
            } else {
                StageTerminal::Failed
            },
            progress: Default::default(),
            commit_boundaries: vec![],
            execution_ids: vec![],
            artifact_ids: vec![],
            effect_outcome: EffectOutcome::NotStarted,
            error_code: Some(if cancel.is_cancelled() {
                ExecutionErrorCode::Cancelled
            } else {
                ExecutionErrorCode::HostRejected
            }),
        }
    }
}
#[async_trait::async_trait]
impl JobHandler for RestoreApplyJob {
    fn kind(&self) -> &str {
        RESTORE_APPLY_KIND
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(&self, plan: &FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
        if !plan.is_apply
            || plan.kind != RESTORE_APPLY_KIND
            || plan.consumed_plan_id.as_deref() != Some(self.plan.plan_id.as_str())
            || plan.selection_revision != Some(self.plan.config_revision.get())
        {
            return Err(JobError::PlanProjectionInvalid(
                "restore plan binding mismatch".into(),
            ));
        }
        if self.plan.target.connection_id.as_str().trim().is_empty()
            || self.plan.input_sha256.len() != 64
            || self.plan.input_bytes.get() == 0
        {
            return Err(JobError::PlanProjectionInvalid(
                "restore target/input is incomplete".into(),
            ));
        }
        Ok(vec![StageSpec {
            stage_id: StageId::new("restoreArtifact"),
            kind: RESTORE_APPLY_KIND.into(),
            depends_on: vec![],
        }])
    }
    async fn run_stage(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        if cancel.is_cancelled() {
            return Ok(self.rejected(spec, cancel, RestoreRejection::InvalidInput));
        }
        let input = match self.input(cancel).await {
            Ok(input) => input,
            Err(reason) => return Ok(self.rejected(spec, cancel, reason)),
        };
        if let Err(reason) = self.gateway.validate(&self.context, &self.plan).await {
            return Ok(self.rejected(spec, cancel, reason));
        }
        if cancel.is_cancelled() {
            return Ok(self.rejected(spec, cancel, RestoreRejection::InvalidInput));
        }
        let evidence = self
            .gateway
            .execute(&self.context, &self.plan, input, cancel)
            .await;
        if evidence.effects == EffectOutcome::NotStarted {
            if let Ok(mut rejection) = self.rejection.lock() {
                *rejection = evidence.rejection;
            }
        }
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal: evidence.terminal,
            progress: evidence.progress,
            commit_boundaries: evidence.boundaries,
            execution_ids: evidence.executions,
            artifact_ids: vec![self.plan.artifact_id.clone()],
            effect_outcome: evidence.effects,
            error_code: match evidence.terminal {
                StageTerminal::Succeeded => None,
                StageTerminal::Cancelled => Some(ExecutionErrorCode::Cancelled),
                StageTerminal::Unknown => Some(ExecutionErrorCode::ResourceLost),
                StageTerminal::Failed if evidence.effects == EffectOutcome::NotStarted => {
                    Some(ExecutionErrorCode::HostRejected)
                }
                StageTerminal::Failed => Some(ExecutionErrorCode::SqlError),
            },
        })
    }
    fn durable_result(&self, outcome: &StageOutcome) -> Option<JobDomainResult> {
        let code = if outcome.effect_outcome == EffectOutcome::NotStarted
            && outcome.terminal == StageTerminal::Cancelled
        {
            "restoreCancelledBeforeSubmission"
        } else {
            match self.rejection.lock().ok().and_then(|reason| *reason) {
                Some(RestoreRejection::Unsupported) => "restoreUnsupported",
                Some(RestoreRejection::NotAuthorized) => "restoreNotAuthorized",
                Some(RestoreRejection::InvalidInput) => "restoreInvalidInput",
                Some(RestoreRejection::RuntimeUnavailable) => "restoreRuntimeUnavailable",
                Some(RestoreRejection::AdmissionRejected) => "restoreAdmissionRejected",
                Some(RestoreRejection::TargetMismatch) => "restoreTargetMismatch",
                None if outcome.effect_outcome == EffectOutcome::NotStarted => {
                    "restoreNotSubmitted"
                }
                None => "restoreExecuted",
            }
        };
        Some(JobDomainResult {
            stage_id: outcome.stage_id.clone(),
            result_code: code.into(),
            outcome_code: match outcome.effect_outcome {
                EffectOutcome::NotStarted => "notStarted",
                EffectOutcome::Completed => "completed",
                EffectOutcome::RolledBack => "rolledBack",
                EffectOutcome::PartiallyApplied => "partiallyApplied",
                EffectOutcome::Unknown => "unknown",
            }
            .into(),
            counters: vec![],
            items: vec![],
            artifact_ids: outcome.artifact_ids.clone(),
        })
    }
    fn verify_recovery(&self, _: &Checkpoint) -> RecoveryVerdict {
        RecoveryVerdict::RequireManualReview {
            reason: "restore effects require driver verification; never automatically replay"
                .into(),
        }
    }
}

/// Hosts without a verified ResourceProvider restore binding must inject this
/// explicit refusal, never adapt a legacy ConnectionHandle into Gateway evidence.
pub struct UnsupportedRestoreGateway;
#[async_trait::async_trait]
impl RestoreGateway for UnsupportedRestoreGateway {
    async fn validate(&self, _: &RequestContext, _: &RestorePlan) -> Result<(), RestoreRejection> {
        Err(RestoreRejection::Unsupported)
    }
    async fn execute(
        &self,
        _: &RequestContext,
        _: &RestorePlan,
        _: Arc<dyn RestoreInput>,
        _: &CancelToken,
    ) -> RestoreEvidence {
        RestoreEvidence {
            rejection: Some(RestoreRejection::Unsupported),
            terminal: StageTerminal::Failed,
            effects: EffectOutcome::NotStarted,
            progress: Default::default(),
            executions: vec![],
            boundaries: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::{
        context::OwnerRef,
        dto::job::JobDefinition,
        id::{
            ClientInstanceId, ConnectionId, IdempotencyKey, JobId, OrganizationId, PrincipalId,
            RequestId,
        },
        ports::{
            artifact::{ArtifactSpec, ChunkPayload},
            job::JobRepository,
        },
    };
    use datazen_runtime::{
        artifact::FileArtifactStore,
        job::{project_frozen_plan, InMemoryJobRepository, JobClock, SystemJobClock},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn context(principal: &str) -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org"),
            PrincipalId::new(principal),
            None,
            ClientInstanceId::new("client"),
            RequestId::new("request"),
            None,
        )
    }
    async fn input() -> (tempfile::TempDir, Arc<FileArtifactStore>, RestorePlan) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FileArtifactStore::open(dir.path().join("artifacts")).unwrap());
        let bytes = b"CREATE TABLE synthetic_probe(id int);";
        let (artifact, mut writer) = store
            .create(
                &context("alice"),
                ArtifactSpec::new(
                    OrganizationId::new("org"),
                    PrincipalId::new("alice"),
                    ExecutionId::new("upload"),
                    RestorePlan::target_fingerprint(&ExecutionTarget::new(
                        ConnectionId::new("profile"),
                        datazen_platform_api::target::NamespaceTarget::new(
                            Some("app".into()),
                            None,
                            None,
                            vec![],
                        ),
                    )),
                ),
            )
            .await
            .unwrap();
        store
            .append_chunk(&mut writer, ChunkPayload::new(bytes.to_vec()))
            .await
            .unwrap();
        store.finalize(writer).await.unwrap();
        let plan = RestorePlan {
            plan_id: "plan".into(),
            target: ExecutionTarget::new(
                ConnectionId::new("profile"),
                datazen_platform_api::target::NamespaceTarget::new(
                    Some("app".into()),
                    None,
                    None,
                    vec![],
                ),
            ),
            config_revision: Counter::new(1),
            artifact_id: artifact,
            input_sha256: format!("{:x}", Sha256::digest(bytes)),
            input_bytes: Counter::new(bytes.len() as u64),
            single_transaction: false,
            overwrite: false,
        };
        (dir, store, plan)
    }
    struct Gateway {
        calls: AtomicUsize,
        unknown: bool,
    }
    #[async_trait::async_trait]
    impl RestoreGateway for Gateway {
        async fn validate(
            &self,
            context: &RequestContext,
            plan: &RestorePlan,
        ) -> Result<(), RestoreRejection> {
            assert_eq!(context.principal_id.as_str(), "alice");
            assert_eq!(plan.target.connection_id.as_str(), "profile");
            Ok(())
        }
        async fn execute(
            &self,
            _: &RequestContext,
            _: &RestorePlan,
            input: Arc<dyn RestoreInput>,
            _: &CancelToken,
        ) -> RestoreEvidence {
            assert!(!input.chunk(Counter::ZERO).await.unwrap().is_empty());
            self.calls.fetch_add(1, Ordering::SeqCst);
            RestoreEvidence {
                rejection: None,
                terminal: if self.unknown {
                    StageTerminal::Unknown
                } else {
                    StageTerminal::Succeeded
                },
                effects: if self.unknown {
                    EffectOutcome::Unknown
                } else {
                    EffectOutcome::Completed
                },
                progress: Default::default(),
                executions: vec![ExecutionId::new("accepted-execution")],
                boundaries: vec![],
            }
        }
    }
    fn stage(job: &RestoreApplyJob, plan: &RestorePlan) -> StageSpec {
        job.validate_plan(&project_frozen_plan(RESTORE_APPLY_KIND, &plan.payload()).unwrap())
            .unwrap()
            .remove(0)
    }
    #[tokio::test]
    async fn complete_authorized_artifact_reaches_gateway_and_preserves_receipt() {
        let (_dir, store, plan) = input().await;
        let gateway = Arc::new(Gateway {
            calls: AtomicUsize::new(0),
            unknown: false,
        });
        let job = RestoreApplyJob::new(context("alice"), plan.clone(), store, gateway.clone());
        let outcome = job
            .run_stage(&stage(&job, &plan), &CancelToken::new())
            .await
            .unwrap();
        assert_eq!(outcome.terminal, StageTerminal::Succeeded);
        assert_eq!(outcome.effect_outcome, EffectOutcome::Completed);
        assert_eq!(
            outcome.execution_ids,
            vec![ExecutionId::new("accepted-execution")]
        );
        assert_eq!(gateway.calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn foreign_owner_and_changed_input_are_rejected_before_execution() {
        for foreign in [true, false] {
            let (_dir, store, mut plan) = input().await;
            if !foreign {
                plan.input_sha256 = "0".repeat(64);
            }
            let gateway = Arc::new(Gateway {
                calls: AtomicUsize::new(0),
                unknown: false,
            });
            let job = RestoreApplyJob::new(
                context(if foreign { "bob" } else { "alice" }),
                plan.clone(),
                store,
                gateway.clone(),
            );
            let outcome = job
                .run_stage(&stage(&job, &plan), &CancelToken::new())
                .await
                .unwrap();
            assert_eq!(outcome.effect_outcome, EffectOutcome::NotStarted);
            assert_eq!(gateway.calls.load(Ordering::SeqCst), 0);
        }
    }
    #[tokio::test]
    async fn persisted_artifact_describe_returns_creation_target_binding() {
        let (dir, store, plan) = input().await;
        drop(store);
        let reopened = FileArtifactStore::open(dir.path().join("artifacts")).unwrap();
        let metadata = reopened
            .describe(&context("alice"), plan.artifact_id.clone())
            .await
            .unwrap();
        assert_eq!(
            metadata.target_fingerprint,
            RestorePlan::target_fingerprint(&plan.target)
        );
    }
    #[tokio::test]
    async fn artifact_target_substitution_is_rejected_before_gateway() {
        for change_connection in [true, false] {
            let (_dir, store, mut plan) = input().await;
            if change_connection {
                plan.target.connection_id = ConnectionId::new("other-profile");
            } else {
                plan.target.namespace = datazen_platform_api::target::NamespaceTarget::new(
                    Some("other-database".into()),
                    None,
                    None,
                    vec![],
                );
            }
            let gateway = Arc::new(Gateway {
                calls: AtomicUsize::new(0),
                unknown: false,
            });
            let job = RestoreApplyJob::new(context("alice"), plan.clone(), store, gateway.clone());
            let outcome = job
                .run_stage(&stage(&job, &plan), &CancelToken::new())
                .await
                .unwrap();
            assert_eq!(outcome.effect_outcome, EffectOutcome::NotStarted);
            assert_eq!(
                job.durable_result(&outcome).unwrap().result_code,
                "restoreInvalidInput"
            );
            assert_eq!(gateway.calls.load(Ordering::SeqCst), 0);
        }
    }
    #[tokio::test]
    async fn unsupported_is_durable_not_started_without_legacy_fallback() {
        let (_dir, store, plan) = input().await;
        let job = RestoreApplyJob::new(
            context("alice"),
            plan.clone(),
            store,
            Arc::new(UnsupportedRestoreGateway),
        );
        let outcome = job
            .run_stage(&stage(&job, &plan), &CancelToken::new())
            .await
            .unwrap();
        assert_eq!(outcome.terminal, StageTerminal::Failed);
        assert_eq!(outcome.effect_outcome, EffectOutcome::NotStarted);
        assert_eq!(
            job.durable_result(&outcome).unwrap().result_code,
            "restoreUnsupported"
        );
    }
    #[tokio::test]
    async fn cancelled_before_submission_does_not_claim_rollback() {
        let (_dir, store, plan) = input().await;
        let gateway = Arc::new(Gateway {
            calls: AtomicUsize::new(0),
            unknown: false,
        });
        let job = RestoreApplyJob::new(context("alice"), plan.clone(), store, gateway.clone());
        let cancel = CancelToken::new();
        cancel.cancel();
        let outcome = job.run_stage(&stage(&job, &plan), &cancel).await.unwrap();
        assert_eq!(outcome.terminal, StageTerminal::Cancelled);
        assert_eq!(outcome.effect_outcome, EffectOutcome::NotStarted);
        assert_eq!(
            job.durable_result(&outcome).unwrap().result_code,
            "restoreCancelledBeforeSubmission"
        );
        assert_eq!(gateway.calls.load(Ordering::SeqCst), 0);
    }

    mod no_write_outcomes {
        include!("restore_job_rejection_tests.rs");
    }
    #[tokio::test]
    async fn unknown_gateway_effects_stay_unknown() {
        let (_dir, store, plan) = input().await;
        let gateway = Arc::new(Gateway {
            calls: AtomicUsize::new(0),
            unknown: true,
        });
        let job = RestoreApplyJob::new(context("alice"), plan.clone(), store, gateway);
        let outcome = job
            .run_stage(&stage(&job, &plan), &CancelToken::new())
            .await
            .unwrap();
        assert_eq!(outcome.terminal, StageTerminal::Unknown);
        assert_eq!(outcome.effect_outcome, EffectOutcome::Unknown);
        assert_eq!(
            job.durable_result(&outcome).unwrap().outcome_code,
            "unknown"
        );
    }
    #[test]
    fn interrupted_restore_never_authorizes_automatic_replay() {
        let context = context("alice");
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FileArtifactStore::open(dir.path().join("artifacts")).unwrap());
        let plan = RestorePlan {
            plan_id: "plan".into(),
            target: ExecutionTarget::new(
                ConnectionId::new("profile"),
                datazen_platform_api::target::NamespaceTarget::new(None, None, None, vec![]),
            ),
            config_revision: Counter::new(1),
            artifact_id: ArtifactId::new("input"),
            input_sha256: "0".repeat(64),
            input_bytes: Counter::new(1),
            single_transaction: false,
            overwrite: false,
        };
        let job = RestoreApplyJob::new(context, plan, store, Arc::new(UnsupportedRestoreGateway));
        let checkpoint = Checkpoint {
            job_id: JobId::new("job"),
            state_version: datazen_platform_api::id::JobStateVersion::new(1),
            stable_target_fingerprint: "target".into(),
            committed: vec![],
            verification_evidence: vec![],
            recovery_policy: "neverReplay".into(),
        };
        assert!(matches!(
            job.verify_recovery(&checkpoint),
            RecoveryVerdict::RequireManualReview { .. }
        ));
    }

    #[tokio::test]
    async fn restore_plan_is_atomically_single_use_across_distinct_idempotency_keys() {
        let (_dir, _store, plan) = input().await;
        let clock = Arc::new(SystemJobClock);
        let repository = InMemoryJobRepository::new(clock.clone(), 60);
        let definition = |id: &str| JobDefinition {
            job_id: JobId::new(id),
            organization_id: context("alice").organization_id,
            kind: RESTORE_APPLY_KIND.into(),
            owner: OwnerRef::ClientSession {
                client_instance_id: ClientInstanceId::new("client"),
                purpose: RESTORE_APPLY_KIND.into(),
            },
            payload: plan.payload(),
            created_at: clock.now(),
        };
        repository
            .accept(
                &context("alice"),
                definition("first"),
                &IdempotencyKey::new("key1"),
            )
            .await
            .unwrap();
        assert!(matches!(
            repository
                .accept(
                    &context("alice"),
                    definition("second"),
                    &IdempotencyKey::new("key2")
                )
                .await,
            Err(datazen_platform_api::PortError::PlanAlreadyConsumed(_))
        ));
    }
    #[tokio::test]
    async fn durable_restore_job_records_unsupported_under_target_writer_budget() {
        use datazen_runtime::{
            budget::{ledger::BudgetLedger, quotas::BudgetConfig},
            job::{EndpointRef, EndpointRole, JobHost},
        };
        let (_dir, store, plan) = input().await;
        let clock = Arc::new(SystemJobClock);
        let repository = Arc::new(InMemoryJobRepository::new(clock.clone(), 60));
        let host = JobHost::new(
            repository,
            Arc::new(Mutex::new(BudgetLedger::new(
                BudgetConfig::desktop_default(),
            ))),
            clock.clone(),
        );
        let context = context("alice");
        let job_id = JobId::new("restore-job");
        let endpoints = vec![EndpointRef {
            connection_id: plan.target.connection_id.clone(),
            service_key: "physical-target".into(),
            objects: vec!["*".into()],
            role: EndpointRole::TargetWriter,
        }];
        host.ensure_endpoint_services(&endpoints).unwrap();
        host.accept(
            &context,
            JobDefinition {
                job_id: job_id.clone(),
                organization_id: context.organization_id.clone(),
                kind: RESTORE_APPLY_KIND.into(),
                owner: OwnerRef::ClientSession {
                    client_instance_id: context.client_instance_id.clone(),
                    purpose: RESTORE_APPLY_KIND.into(),
                },
                payload: plan.payload(),
                created_at: clock.now(),
            },
            &IdempotencyKey::new("restore-key"),
        )
        .await
        .unwrap();
        let handler = Arc::new(RestoreApplyJob::new(
            context.clone(),
            plan,
            store,
            Arc::new(UnsupportedRestoreGateway),
        ));
        host.dispatch(
            &context,
            &job_id,
            &datazen_platform_api::id::WorkerId::new("worker"),
            &endpoints,
            handler,
        )
        .await
        .unwrap();
        let details = host.details(&context, job_id).await.unwrap();
        assert_eq!(
            details.job.state,
            datazen_platform_api::dto::job::JobState::Failed
        );
        assert_eq!(details.job.effect_outcome, Some(EffectOutcome::NotStarted));
        assert_eq!(details.domain_results[0].result_code, "restoreUnsupported");
        assert!(details.commit_boundaries.is_empty());
    }
}
