//! Restore apply job 行为用例（D6 item 5 切片 b 随迁 feature-core 集成测试）。
//!
//! 唯一定义在 `datazen-modules::backup::restore_job`；这些用例需要真实的
//! FileArtifactRepository 与 JobHost 引擎（runtime/kernel dev-dep），故留在
//! feature-core 垫片层——垫片保留集成测试先例，D8 一并处置。
#[cfg(test)]
mod tests {
    use datazen_feature_core::restore_job::{
        RestoreApplyJob, RestoreEvidence, RestoreGateway, RestoreInput, RestorePlan,
        RestoreRejection, UnsupportedRestoreGateway, RESTORE_APPLY_KIND,
    };
    use datazen_kernel::budget::{ledger::BudgetLedger, quotas::BudgetConfig};
    use datazen_kernel::job::{
        project_frozen_plan, CancelToken, InMemoryJobRepository, JobClock, JobHandler,
        RecoveryVerdict, StageSpec, StageTerminal, SystemJobClock,
    };
    use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
    use datazen_platform_api::{
        context::OwnerRef,
        dto::job::{Checkpoint, JobDefinition},
        id::{
            ArtifactId, ClientInstanceId, ConnectionId, Counter, ExecutionId, IdempotencyKey,
            JobId, OrganizationId, PrincipalId, RequestId,
        },
        ports::{
            artifact::{ArtifactRepository, ArtifactSpec, ChunkPayload},
            job::JobRepository,
        },
        target::ExecutionTarget,
        RequestContext,
    };
    use datazen_runtime::artifact::FileArtifactRepository;
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
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
    async fn input() -> (tempfile::TempDir, Arc<FileArtifactRepository>, RestorePlan) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FileArtifactRepository::open(dir.path().join("artifacts")).unwrap());
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
        let reopened = FileArtifactRepository::open(dir.path().join("artifacts")).unwrap();
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
        include!("restore_apply/rejection_tests.rs");
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
        let store = Arc::new(FileArtifactRepository::open(dir.path().join("artifacts")).unwrap());
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
        use datazen_kernel::job::{EndpointRef, EndpointRole, JobHost};
        let (_dir, store, plan) = input().await;
        let clock = Arc::new(SystemJobClock);
        let repository = Arc::new(InMemoryJobRepository::new(clock.clone(), 60));
        let host = JobHost::new(
            repository,
            Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::local_default()))),
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
