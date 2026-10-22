use super::*;
use crate::{
    budget::{BudgetConfig, BudgetLedger},
    job::{HandlerRegistry, InMemoryJobRepository, JobHandler, JobRuntime, SharedClock},
};
use datazen_platform_api::{
    context::{OwnerRef, RequestContext},
    dto::job::{JobDefinition, JobState},
    id::*,
    ports::{budget::BudgetCoordinator, job::JobRepository},
};
use std::sync::atomic::AtomicUsize;
struct FakePort {
    sockets: Option<u32>,
    processes: Option<u32>,
    cleanup: AtomicBool,
    calls: AtomicUsize,
    wait: bool,
}
#[async_trait::async_trait]
impl NativeToolPort for FakePort {
    fn resolve(&self, selection: &NativeToolSelection) -> Result<NativeToolResolution, PortError> {
        Ok(NativeToolResolution {
            key: "pinned".into(),
            selection: selection.clone(),
            max_processes: self.processes,
            max_database_sockets: self.sockets,
            connection_id: Some(ConnectionId::new("native-target")),
        })
    }
    async fn run(
        &self,
        _: &str,
        _: NativeToolResolution,
        cancel: Arc<AtomicBool>,
        _: NativeToolLimits,
    ) -> Result<NativeToolReport, PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.wait {
            while !cancel.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
        Ok(NativeToolReport {
            termination: if self.wait {
                NativeTermination::Cancelled
            } else {
                NativeTermination::Exited
            },
            exit_code: Some(0),
            stdout_bytes: 12,
            stderr_bytes: 0,
            cleanup: if self.cleanup.load(Ordering::Acquire) {
                NativeCleanup::Confirmed
            } else {
                NativeCleanup::Unconfirmed
            },
            version_verified: true,
        })
    }
    async fn cleanup_status(&self, _: &str) -> NativeCleanup {
        if self.cleanup.load(Ordering::Acquire) {
            NativeCleanup::Confirmed
        } else {
            NativeCleanup::Unconfirmed
        }
    }
}
fn context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("native-org"),
        PrincipalId::new("native-user"),
        None,
        ClientInstanceId::new("native-client"),
        RequestId::new("native-request"),
        None,
    )
}
fn fake(sockets: Option<u32>, cleanup: bool, wait: bool) -> Arc<FakePort> {
    Arc::new(FakePort {
        sockets,
        processes: Some(1),
        cleanup: AtomicBool::new(cleanup),
        calls: AtomicUsize::new(0),
        wait,
    })
}
fn native(
    port: Arc<dyn NativeToolPort>,
    cap: u32,
) -> (Arc<NativeToolRuntime>, Arc<InProcessBudgetCoordinator>) {
    let budget = Arc::new(InProcessBudgetCoordinator::new(
        BudgetConfig::from_parts(cap, [0; 4]).with_physical_connections(cap),
        context().organization_id,
    ));
    (
        NativeToolRuntime::new(port, budget.clone(), 1, NativeToolLimits::default()).unwrap(),
        budget,
    )
}
async fn job_with_selection(
    runtime: Arc<NativeToolRuntime>,
    id: &str,
    selection: NativeToolSelection,
) -> (JobRuntime, Arc<InMemoryJobRepository>, NativeToolJobHandler) {
    let clock = Arc::new(SharedClock::at("2026-01-01T00:00:00Z"));
    let repo = Arc::new(InMemoryJobRepository::new(clock.clone(), 300));
    let handler = NativeToolJobHandler::new(JobId::new(id), selection, runtime.clone());
    let handler2 = NativeToolJobHandler::new(
        JobId::new(id),
        NativeToolSelection {
            tool_id: "probe".into(),
            version: "1".into(),
        },
        runtime,
    );
    repo.accept(
        &context(),
        JobDefinition {
            job_id: JobId::new(id),
            organization_id: context().organization_id,
            kind: "nativeToolProbe".into(),
            owner: OwnerRef::ClientSession {
                client_instance_id: context().client_instance_id,
                purpose: "native".into(),
            },
            payload: NativeToolJobHandler::plan_payload(&NativeToolSelection {
                tool_id: "probe".into(),
                version: "1".into(),
            }),
            created_at: Timestamp::new("2026-01-01T00:00:00Z"),
        },
        &IdempotencyKey::new(id),
    )
    .await
    .unwrap();
    let mut handlers = HandlerRegistry::new();
    handlers.register(Arc::new(handler));
    (
        JobRuntime::new(
            repo.clone(),
            Arc::new(handlers),
            Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::local_default()))),
            clock,
            0,
        ),
        repo,
        handler2,
    )
}
async fn job(
    runtime: Arc<NativeToolRuntime>,
    id: &str,
) -> (JobRuntime, Arc<InMemoryJobRepository>, NativeToolJobHandler) {
    job_with_selection(
        runtime,
        id,
        NativeToolSelection {
            tool_id: "probe".into(),
            version: "1".into(),
        },
    )
    .await
}

#[tokio::test]
async fn native_selection_must_match_the_accepted_job_before_process_admission() {
    let port = fake(Some(0), true, false);
    let (runtime, budget) = native(port.clone(), 2);
    let (jobs, _, _) = job_with_selection(
        runtime,
        "wrong-selection",
        NativeToolSelection {
            tool_id: "probe".into(),
            version: "2".into(),
        },
    )
    .await;
    let result = jobs
        .run(
            &context(),
            &JobId::new("wrong-selection"),
            &WorkerId::new("worker"),
            &[],
        )
        .await
        .unwrap();
    assert_ne!(result.state, JobState::Succeeded);
    assert_eq!(port.calls.load(Ordering::SeqCst), 0);
    assert_eq!(in_use(&budget).await, 0);
}

async fn in_use(budget: &InProcessBudgetCoordinator) -> u64 {
    budget.snapshot().await.unwrap().in_use.get()
}
#[tokio::test]
async fn native_job_success_is_owned_and_unknown_resources_reject_before_spawn() {
    let port = fake(Some(0), true, false);
    let (runtime, budget) = native(port.clone(), 2);
    let (jobs, _, _) = job(runtime, "zero").await;
    let result = jobs
        .run(
            &context(),
            &JobId::new("zero"),
            &WorkerId::new("worker"),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(result.state, JobState::Succeeded);
    assert_eq!(result.effect_outcome, EffectOutcome::Completed);
    assert_eq!(port.calls.load(Ordering::SeqCst), 1);
    assert_eq!(in_use(&budget).await, 0);
    for (processes, sockets) in [(Some(1), None), (None, Some(1)), (Some(2), Some(1))] {
        let port = Arc::new(FakePort {
            sockets,
            processes,
            cleanup: AtomicBool::new(true),
            calls: AtomicUsize::new(0),
            wait: false,
        });
        let (runtime, _) = native(port.clone(), 2);
        let (jobs, _, _) = job(runtime, "unproven").await;
        assert!(jobs
            .run(
                &context(),
                &JobId::new("unproven"),
                &WorkerId::new("worker"),
                &[]
            )
            .await
            .is_err());
        assert_eq!(port.calls.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test]
async fn native_atomic_socket_budget_insufficient_never_spawns() {
    let port = fake(Some(3), true, false);
    let (runtime, budget) = native(port.clone(), 2);
    let (jobs, _, _) = job(runtime, "denied").await;
    let result = jobs
        .run(
            &context(),
            &JobId::new("denied"),
            &WorkerId::new("worker"),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(result.effect_outcome, EffectOutcome::NotStarted);
    assert_eq!(port.calls.load(Ordering::SeqCst), 0);
    assert_eq!(in_use(&budget).await, 0);
}
#[tokio::test]
async fn native_unknown_retains_full_socket_and_process_budget_never_replays_then_cleanup_releases()
{
    let port = fake(Some(2), false, false);
    let (runtime, budget) = native(port.clone(), 2);
    let (jobs, _, handler) = job(runtime.clone(), "unknown").await;
    let result = jobs
        .run(
            &context(),
            &JobId::new("unknown"),
            &WorkerId::new("worker"),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(in_use(&budget).await, 2);
    assert!(jobs
        .run(
            &context(),
            &JobId::new("unknown"),
            &WorkerId::new("worker"),
            &[]
        )
        .await
        .is_err());
    let (other, _, _) = job(runtime, "other").await;
    assert_eq!(
        other
            .run(
                &context(),
                &JobId::new("other"),
                &WorkerId::new("worker"),
                &[]
            )
            .await
            .unwrap()
            .effect_outcome,
        EffectOutcome::NotStarted
    );
    assert_eq!(port.calls.load(Ordering::SeqCst), 1);
    let checkpoint = datazen_platform_api::dto::job::Checkpoint {
        job_id: JobId::new("unknown"),
        state_version: JobStateVersion::new(1),
        stable_target_fingerprint: "native-probe".into(),
        committed: vec![],
        verification_evidence: vec![],
        recovery_policy: "manual".into(),
    };
    assert!(matches!(
        handler.verify_recovery(&checkpoint),
        crate::job::RecoveryVerdict::RequireManualReview { .. }
    ));
    port.cleanup.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(2), async {
        while in_use(&budget).await != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn native_job_cancel_reaches_inflight_port_and_cleanup_releases_reserved_cost() {
    let port = fake(Some(1), true, true);
    let (runtime, budget) = native(port.clone(), 2);
    let (jobs, repo, _) = job(runtime, "cancel").await;
    let task = tokio::spawn(async move {
        jobs.run(
            &context(),
            &JobId::new("cancel"),
            &WorkerId::new("worker"),
            &[],
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while port.calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(in_use(&budget).await, 1);
    repo.request_cancel(&context(), &JobId::new("cancel"))
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    // DB effect is unknown even when cancellation and process cleanup are confirmed.
    assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(in_use(&budget).await, 0);
}
#[test]
fn native_selection_rejects_executable_argv_environment_fields() {
    for key in ["path", "executable", "argv", "env", "workingDirectory"] {
        let mut input = serde_json::json!({"toolId":"probe","version":"1"});
        input[key] = serde_json::json!("untrusted");
        assert!(serde_json::from_value::<NativeToolSelection>(input).is_err());
    }
}

struct CleanupFaultPort {
    inner: Arc<FakePort>,
    polls: AtomicUsize,
    panic_until_recovered: AtomicBool,
}
#[async_trait::async_trait]
impl NativeToolPort for CleanupFaultPort {
    fn resolve(&self, selection: &NativeToolSelection) -> Result<NativeToolResolution, PortError> {
        self.inner.resolve(selection)
    }
    async fn run(
        &self,
        key: &str,
        resolution: NativeToolResolution,
        cancel: Arc<AtomicBool>,
        limits: NativeToolLimits,
    ) -> Result<NativeToolReport, PortError> {
        self.inner.run(key, resolution, cancel, limits).await
    }
    async fn cleanup_status(&self, key: &str) -> NativeCleanup {
        self.polls.fetch_add(1, Ordering::SeqCst);
        assert!(
            !self.panic_until_recovered.load(Ordering::Acquire),
            "synthetic cleanup observer fault"
        );
        self.inner.cleanup_status(key).await
    }
}

#[tokio::test]
async fn cleanup_observer_panic_retains_both_budgets_until_confirmed_without_replay() {
    let inner = fake(Some(2), false, false);
    let port = Arc::new(CleanupFaultPort {
        inner: inner.clone(),
        polls: AtomicUsize::new(0),
        panic_until_recovered: AtomicBool::new(true),
    });
    let (runtime, budget) = native(port.clone(), 4);
    let (jobs, _, _) = job(runtime.clone(), "cleanup-fault").await;
    let initial = jobs
        .run(
            &context(),
            &JobId::new("cleanup-fault"),
            &WorkerId::new("worker"),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(initial.effect_outcome, EffectOutcome::Unknown);
    tokio::time::timeout(Duration::from_secs(2), async {
        while port.polls.load(Ordering::Acquire) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    // Let the panic unwind through the observer, before checking its owner.
    tokio::task::yield_now().await;
    assert_eq!(runtime.processes.available_permits(), 0);
    assert_eq!(in_use(&budget).await, 2);
    let (other, _, _) = job(runtime.clone(), "during-cleanup-fault").await;
    let denied = other
        .run(
            &context(),
            &JobId::new("during-cleanup-fault"),
            &WorkerId::new("worker"),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(denied.effect_outcome, EffectOutcome::NotStarted);
    assert_eq!(inner.calls.load(Ordering::Acquire), 1);
    port.panic_until_recovered.store(false, Ordering::Release);
    inner.cleanup.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(2), async {
        while in_use(&budget).await != 0 || runtime.processes.available_permits() != 1 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(port.polls.load(Ordering::Acquire) >= 2);
    assert_eq!(inner.calls.load(Ordering::Acquire), 1);
    assert_eq!(
        runtime
            .entries()
            .values()
            .filter_map(|entry| entry.result.as_ref())
            .filter(|result| result.effect == EffectOutcome::Unknown)
            .count(),
        1
    );
    let (after, _, _) = job(runtime, "after-cleanup-confirmed").await;
    after
        .run(
            &context(),
            &JobId::new("after-cleanup-confirmed"),
            &WorkerId::new("worker"),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(inner.calls.load(Ordering::Acquire), 2);
    assert_eq!(in_use(&budget).await, 0);
}

#[tokio::test]
async fn abandoned_job_waiter_still_cancels_original_invocation_and_retains_cleanup_budgets() {
    // A stalled provider future models a blocked native pipeline. Real pipe
    // output limits/reaping are separately tested by the desktop process adapter.
    let port = fake(Some(2), false, true);
    let (runtime, budget) = native(port.clone(), 4);
    let (jobs, repo, _) = job(runtime.clone(), "abandoned-native").await;
    let jobs = Arc::new(jobs);
    let caller = jobs.clone();
    let waiter = tokio::spawn(async move {
        caller
            .run(
                &context(),
                &JobId::new("abandoned-native"),
                &WorkerId::new("worker"),
                &[],
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while port.calls.load(Ordering::Acquire) != 1 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert_eq!(in_use(&budget).await, 2);
    assert_eq!(runtime.processes.available_permits(), 0);
    repo.request_cancel(&context(), &JobId::new("abandoned-native"))
        .unwrap();
    let persisted = repo
        .get(&context(), JobId::new("abandoned-native"))
        .await
        .unwrap();
    // Aborting the JobRuntime caller is a failure injection, not product window
    // unsubscribe: it cannot fabricate a persisted successful Job completion.
    assert_eq!(persisted.view.state, JobState::Running);
    assert!(persisted.view.cancel_requested);
    // The JobRuntime cancel watcher was dropped with its caller. The retained
    // native owner must independently see revocation and stop the original run.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let finished = runtime
                .entries()
                .values()
                .any(|entry| entry.result.is_some());
            if finished {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    let receipt = runtime
        .entries()
        .values()
        .find_map(|entry| entry.result.clone())
        .unwrap();
    assert_eq!(receipt.effect, EffectOutcome::Unknown);
    let report = receipt.report.as_ref().unwrap();
    assert_eq!(report.termination, NativeTermination::Cancelled);
    assert_eq!(report.cleanup, NativeCleanup::Unconfirmed);
    assert_eq!(port.calls.load(Ordering::Acquire), 1);
    assert_eq!(in_use(&budget).await, 2);
    assert_eq!(runtime.processes.available_permits(), 0);
    // Neither resubmitting the accepted Job nor a different Job may replay it.
    assert!(jobs
        .run(
            &context(),
            &JobId::new("abandoned-native"),
            &WorkerId::new("worker"),
            &[]
        )
        .await
        .is_err());
    let (other, _, _) = job(runtime.clone(), "while-abandoned-cleans").await;
    assert_eq!(
        other
            .run(
                &context(),
                &JobId::new("while-abandoned-cleans"),
                &WorkerId::new("worker"),
                &[]
            )
            .await
            .unwrap()
            .effect_outcome,
        EffectOutcome::NotStarted
    );
    assert_eq!(port.calls.load(Ordering::Acquire), 1);
    port.cleanup.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(2), async {
        while in_use(&budget).await != 0 || runtime.processes.available_permits() != 1 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let after = runtime
        .entries()
        .values()
        .find_map(|entry| {
            entry
                .result
                .clone()
                .filter(|result| result.effect == EffectOutcome::Unknown)
        })
        .unwrap();
    assert_eq!(after, receipt); // late cleanup never rewrites effects or replays.
    assert_eq!(port.calls.load(Ordering::Acquire), 1);
}

struct RunPanicPort(Arc<FakePort>);
#[async_trait::async_trait]
impl NativeToolPort for RunPanicPort {
    fn resolve(&self, selection: &NativeToolSelection) -> Result<NativeToolResolution, PortError> {
        self.0.resolve(selection)
    }
    async fn run(
        &self,
        _: &str,
        _: NativeToolResolution,
        _: Arc<AtomicBool>,
        _: NativeToolLimits,
    ) -> Result<NativeToolReport, PortError> {
        self.0.calls.fetch_add(1, Ordering::SeqCst);
        // A panic cannot promise the pre-spawn Err contract; cleanup is unknown.
        panic!("synthetic post-start provider panic");
    }
    async fn cleanup_status(&self, key: &str) -> NativeCleanup {
        self.0.cleanup_status(key).await
    }
}

#[tokio::test]
async fn provider_run_panic_is_unknown_and_retains_reservations_until_late_cleanup() {
    let inner = fake(Some(2), false, false);
    let (runtime, budget) = native(Arc::new(RunPanicPort(inner.clone())), 4);
    let (jobs, _, _) = job(runtime.clone(), "run-panic").await;
    let result = jobs
        .run(
            &context(),
            &JobId::new("run-panic"),
            &WorkerId::new("worker"),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(in_use(&budget).await, 2);
    assert_eq!(runtime.processes.available_permits(), 0);
    let receipt = runtime
        .entries()
        .values()
        .find_map(|entry| entry.result.clone())
        .unwrap();
    assert_eq!(
        receipt.report.as_ref().unwrap().cleanup,
        NativeCleanup::Unconfirmed
    );
    let (other, _, _) = job(runtime.clone(), "during-run-panic-cleanup").await;
    assert_eq!(
        other
            .run(
                &context(),
                &JobId::new("during-run-panic-cleanup"),
                &WorkerId::new("worker"),
                &[]
            )
            .await
            .unwrap()
            .effect_outcome,
        EffectOutcome::NotStarted
    );
    assert_eq!(inner.calls.load(Ordering::Acquire), 1);
    inner.cleanup.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(2), async {
        while in_use(&budget).await != 0 || runtime.processes.available_permits() != 1 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        runtime
            .entries()
            .values()
            .find_map(|entry| entry
                .result
                .clone()
                .filter(|result| result.effect == EffectOutcome::Unknown))
            .unwrap(),
        receipt
    );
    assert_eq!(inner.calls.load(Ordering::Acquire), 1);
}
