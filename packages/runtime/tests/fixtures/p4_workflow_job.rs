use super::*;
use datazen_platform_api::{
    dto::job::{Checkpoint, JobDefinition, JobProgress, JobState},
    ports::job::JobRepository,
};
use datazen_runtime::budget::BudgetLedger;
use datazen_runtime::job::*;
#[path = "../support/mod.rs"]
mod budget_support;

struct Handler {
    service: Arc<RuntimeConnectionUseCases>,
    permit: Mutex<Option<StageOwnerPermit>>,
    backend: Arc<Backend>,
    mode: u8,
    repo: Arc<InMemoryJobRepository>,
}
#[async_trait]
impl JobHandler for Handler {
    fn kind(&self) -> &str {
        "workflowRun"
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(&self, _: &FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
        Ok(vec![StageSpec {
            stage_id: StageId::new("workflow"),
            kind: "workflowRun".into(),
            depends_on: vec![],
        }])
    }
    async fn run_stage(&self, _: &StageSpec, _: &CancelToken) -> Result<StageOutcome, JobError> {
        panic!("untrusted dispatch must not run")
    }
    async fn run_stage_authorized(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
        permit: StageOwnerPermit,
    ) -> Result<StageOutcome, JobError> {
        let deadline = tokio::time::Instant::now()
            + std::time::Duration::from_millis(if self.mode == 1 { 60 } else { 5000 });
        let mut first = permit
            .open_workflow_block(
                self.service.clone(),
                BlockId::new("first"),
                target("a"),
                deadline,
            )
            .await
            .unwrap();
        let handle = first.handle().unwrap().clone();
        let view = self
            .service
            .get_session(permit.context(), handle.clone())
            .await
            .unwrap();
        assert_eq!(
            view.owner,
            OwnerRef::WorkflowBlock {
                job_id: permit.job_id().clone(),
                block_id: BlockId::new("first")
            }
        );
        if self.mode >= 2 {
            if self.mode == 3 {
                // A command has an acceptance receipt, but its async dispatch
                // task has not run yet on this single-thread test runtime.
                let accepted = execute(&self.service, permit.context(), &handle, "marker").await;
                self.repo
                    .request_cancel(permit.context(), permit.job_id())
                    .unwrap();
                let result =
                    terminal(&self.service, permit.context(), &accepted.execution_id).await;
                assert_eq!(
                    result.state,
                    datazen_platform_api::dto::execution::ExecutionState::Failed
                );
                assert!(!lock(&self.backend.journal)
                    .iter()
                    .any(|event| matches!(event, ResourceEvent::Executed(..))));
                tokio::time::timeout(std::time::Duration::from_secs(2), async {
                    while !cancel.is_cancelled() {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                assert!(permit.verify_active().await.is_err());
            }
            drop(first);
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while self.backend.closed.load(Ordering::SeqCst) == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            return Err(JobError::CapabilityUnsupported(
                "injected stage failure".into(),
            ));
        }
        if self.mode == 1 {
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while self.backend.closed.load(Ordering::SeqCst) == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(self
                .service
                .get_session(permit.context(), handle)
                .await
                .is_err());
            // Expiry has closed the session: its execution terminal is failed.
        } else {
            let one = execute(&self.service, permit.context(), &handle, "marker").await;
            terminal(&self.service, permit.context(), &one.execution_id).await;
            let two = execute(&self.service, permit.context(), &handle, "marker").await;
            terminal(&self.service, permit.context(), &two.execution_id).await;
            let events = lock(&self.backend.journal).clone();
            let physical: Vec<_> = events
                .iter()
                .filter_map(|e| {
                    if let ResourceEvent::Executed(id, _) = e {
                        Some(id)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(physical.len(), 2);
            assert_eq!(physical[0], physical[1]);
            let receipt = self
                .service
                .close_session(
                    permit.context(),
                    CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
                )
                .await
                .unwrap();
            assert_eq!(receipt.resource_release, pa::ResourceRelease::Confirmed);
            first.confirm_closed();
            let mut second = permit
                .open_workflow_block(
                    self.service.clone(),
                    BlockId::new("second"),
                    target("a"),
                    deadline,
                )
                .await
                .unwrap();
            assert_eq!(self.backend.opened.load(Ordering::SeqCst), 2);
            let closed = self
                .service
                .close_session(
                    permit.context(),
                    CloseSessionRequest::new(
                        second.handle().unwrap().clone(),
                        CloseMode::RollbackAndClose,
                    ),
                )
                .await
                .unwrap();
            assert_eq!(closed.resource_release, pa::ResourceRelease::Confirmed);
            second.confirm_closed();
        }
        *lock(&self.permit) = Some(permit);
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal: StageTerminal::Succeeded,
            progress: JobProgress::default(),
            commit_boundaries: vec![],
            execution_ids: vec![],
            artifact_ids: vec![],
            effect_outcome: EffectOutcome::Completed,
            error_code: None,
        })
    }
    fn verify_recovery(&self, _: &Checkpoint) -> RecoveryVerdict {
        RecoveryVerdict::RequireManualReview {
            reason: "no resource recovery".into(),
        }
    }
}
async fn run(mode: u8) {
    let (service, backend, _, _) = setup();
    let service = Arc::new(service);
    let context = ctx("user", "desktop");
    let clock = Arc::new(SystemJobClock);
    let repo = Arc::new(InMemoryJobRepository::new(clock.clone(), 300));
    let id = JobId::new("accepted-workflow");
    repo.accept(
        &context,
        JobDefinition {
            job_id: id.clone(),
            kind: "workflowRun".into(),
            owner: OwnerRef::Job {
                job_id: id.clone(),
                stage_id: StageId::new("workflow"),
            },
            payload: serde_json::json!({"planVersion":1,"handlerVersion":1,"checkpointVersion":1}),
            created_at: clock.now(),
        },
        &IdempotencyKey::new("accept"),
    )
    .await
    .unwrap();
    let handler = Arc::new(Handler {
        service: service.clone(),
        backend: backend.clone(),
        permit: Mutex::new(None),
        mode,
        repo: repo.clone(),
    });
    let mut handlers = HandlerRegistry::new();
    handlers.register(handler.clone());
    let runtime = JobRuntime::new(
        repo.clone(),
        Arc::new(handlers),
        Arc::new(Mutex::new(BudgetLedger::new(budget_support::config(
            8,
            [1, 1, 1, 0],
        )))),
        clock,
        0,
    );
    let result = runtime
        .run(&context, &id, &WorkerId::new("trusted-worker"), &[])
        .await
        .unwrap();
    if mode < 2 {
        assert_eq!(result.state, JobState::Succeeded);
    } else {
        assert_ne!(result.state, JobState::Succeeded);
        assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    }
    if let Some(stale) = lock(&handler.permit).clone() {
        assert!(stale
            .open_workflow_block(
                service.clone(),
                BlockId::new("late"),
                target("a"),
                tokio::time::Instant::now() + std::time::Duration::from_secs(1)
            )
            .await
            .is_err());
    }
    assert_eq!(
        backend.closed.load(Ordering::SeqCst),
        backend.opened.load(Ordering::SeqCst)
    );
}
#[tokio::test]
async fn accepted_workflow_stage_retains_one_session_and_isolates_second_block() {
    run(0).await
}
#[tokio::test]
async fn caller_owner_fields_cannot_authorize_job_or_block() {
    let (service, backend, _, _) = setup();
    let context = ctx("user", "desktop");
    for owner in [
        OwnerRef::Job {
            job_id: JobId::new("invented"),
            stage_id: StageId::new("workflow"),
        },
        OwnerRef::WorkflowBlock {
            job_id: JobId::new("invented"),
            block_id: BlockId::new("block"),
        },
    ] {
        let token = service
            .issue_submission_token(&context, IdempotentOperation::OpenSession, None)
            .await
            .unwrap();
        assert!(service
            .open_session(
                &context,
                OpenSessionRequest::new(target("a"), owner, token.idempotency_key)
            )
            .await
            .is_err());
    }
    assert_eq!(backend.opened.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn block_deadline_releases_session_without_gui_attachment() {
    run(1).await
}

#[tokio::test]
async fn failed_job_stage_drops_and_releases_retained_resource() {
    run(2).await
}
#[tokio::test]
async fn cancelled_job_between_enqueue_and_dispatch_rejects_sql_and_releases_resource() {
    run(3).await
}
