//! Interval scheduler for Workflow jobs (timed backup / automation).
//!
//! Mirrors the dashboard monitor engine: tick loop, in-flight guard, and a
//! host-supplied schedule port (the host binds it to its IPC and app handle).
//!
//! # 调度授权链（WR-A4 / P6 关闭项）
//!
//! 定时派发不再「到点即执行」：派发前必须经
//! [`WorkflowScheduleGrantReader`]（只读窄端口）取得当前授权单并通过校验。
//!
//! * **上膛冻结**（`DueDecision::Arm`）：首次观察某定时工作流时，把当次读到的
//!   有效授权单冻结进 `granted`。无效授权单不上膛——冻结槽留空。
//! * **触发比对**（`DueDecision::Fire`）：重读当前授权单，与冻结副本比较。
//!   撤销、重发、定义变更都表现为「当前 ≠ 冻结」（撤销版本前移）→ 拒绝本次
//!   派发，并以当前单重新上膛（下个 interval 起在新授权下派发）。这与 monitor
//!   引擎的「admission 冻结 → 派发前重读比对」同裁决。
//! * **失败关闭**：端口故障、授权单缺失、单本体无效（撤销/过期/版本 0/作用域
//!   空）一律拒绝，绝不降级为无授权派发。
//!
//! 既有执行路径（`WorkflowScheduleHost::execute_workflow`）与其目标/结果/模板
//! 语义逐字不变——本模块只在它前面加了一道授权闸门。

use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::FutureExt;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::workflow::application::grant_reader::WorkflowScheduleGrantReader;
use crate::workflow::domain::grant::WorkflowScheduleGrant;
use crate::workflow::domain::model::{WorkflowDefinition, WorkflowExecutionResult};

#[async_trait::async_trait]
pub trait WorkflowScheduleHost: Send + Sync {
    async fn list_workflows(&self) -> Result<Vec<WorkflowDefinition>, String>;
    async fn execute_workflow(&self, id: &str) -> Result<WorkflowExecutionResult, String>;
}

pub const MIN_INTERVAL_SECS: u64 = 30;

pub fn clamp_interval_secs(secs: u64) -> u64 {
    secs.max(MIN_INTERVAL_SECS)
}

/// First observation of a scheduled workflow: arm the clock without firing.
/// Firing on process start would re-run every backup job after every restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueDecision {
    Arm,
    Skip,
    Fire,
}

pub fn due_decision(last_run: Option<Instant>, now: Instant, interval_secs: u64) -> DueDecision {
    match last_run {
        None => DueDecision::Arm,
        Some(t) if now.duration_since(t).as_secs() >= interval_secs => DueDecision::Fire,
        Some(_) => DueDecision::Skip,
    }
}

pub fn is_workflow_due(last_run: Option<Instant>, now: Instant, interval_secs: u64) -> bool {
    matches!(
        due_decision(last_run, now, interval_secs),
        DueDecision::Fire
    )
}

pub fn scheduled_interval_secs(workflow: &WorkflowDefinition) -> Option<u64> {
    let schedule = workflow.schedule.as_ref()?;
    if !schedule.enabled {
        return None;
    }
    Some(clamp_interval_secs(schedule.interval_secs.unwrap_or(3600)))
}

/// RAII guard that removes a workflow id from the `in_flight` set on drop.
///
/// This guarantees cleanup even when the spawned future panics or is cancelled
/// by the tokio runtime, preventing the id from being permanently stuck.
struct InFlightGuard {
    scheduler: Arc<WorkflowScheduler>,
    id: String,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        // `try_write` is non-blocking; if the lock is held (extremely rare at
        // drop time since the task is ending), we accept the id staying — the
        // next tick's insert would catch it.
        if let Ok(mut set) = self.scheduler.in_flight.try_write() {
            set.remove(&self.id);
        }
    }
}

pub struct WorkflowScheduler {
    host: Mutex<Option<Arc<dyn WorkflowScheduleHost>>>,
    /// 定时调度授权单的只读端口。派发闸门的事实来源；宿主装配。
    grant_reader: Mutex<Option<Arc<dyn WorkflowScheduleGrantReader>>>,
    /// 上膛时冻结的授权单（按 workflow id）。触发时与当前重读结果比对。
    granted: RwLock<HashMap<String, WorkflowScheduleGrant>>,
    paused: AtomicBool,
    last_run: RwLock<HashMap<String, Instant>>,
    in_flight: RwLock<HashSet<String>>,
    cancel_token: CancellationToken,
}

impl WorkflowScheduler {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            host: Mutex::new(None),
            grant_reader: Mutex::new(None),
            granted: RwLock::new(HashMap::new()),
            paused: AtomicBool::new(false),
            last_run: RwLock::new(HashMap::new()),
            in_flight: RwLock::new(HashSet::new()),
            cancel_token: CancellationToken::new(),
        })
    }

    /// 绑定宿主与授权单端口，并启动 tick 循环。
    pub fn start(
        self: &Arc<Self>,
        host: Arc<dyn WorkflowScheduleHost>,
        grants: Arc<dyn WorkflowScheduleGrantReader>,
    ) {
        self.bind(host, grants);
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            engine.run_loop().await;
        });
    }

    /// 仅绑定宿主与授权单端口（不启动循环）：需要手动驱动 tick 的场景使用。
    fn bind(
        &self,
        host: Arc<dyn WorkflowScheduleHost>,
        grants: Arc<dyn WorkflowScheduleGrantReader>,
    ) {
        *self.host.lock().unwrap_or_else(|error| error.into_inner()) = Some(host);
        *self
            .grant_reader
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(grants);
    }

    fn host_handle(&self) -> Option<Arc<dyn WorkflowScheduleHost>> {
        self.host
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn grant_reader_handle(&self) -> Option<Arc<dyn WorkflowScheduleGrantReader>> {
        self.grant_reader
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    async fn run_loop(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => break,
                _ = ticker.tick() => {
                    if self.paused.load(Ordering::Relaxed) {
                        continue;
                    }
                    self.tick().await;
                }
            }
        }
    }

    async fn tick(self: &Arc<Self>) {
        let handle = match self.host_handle() {
            Some(h) => h,
            None => return,
        };
        let grants = match self.grant_reader_handle() {
            Some(g) => g,
            None => {
                // 授权单端口未装配：失败关闭——没有事实来源就不派发。
                tracing::warn!("workflow scheduler: grant reader unbound; dispatch disabled");
                return;
            }
        };
        let workflows = match handle.list_workflows().await {
            Ok(workflows) => workflows,
            Err(error) => {
                tracing::warn!(%error, "workflow scheduler: failed to load workflows");
                return;
            }
        };
        let now = Instant::now();
        for workflow in workflows {
            let Some(interval) = scheduled_interval_secs(&workflow) else {
                continue;
            };
            let last = self.last_run.read().await.get(&workflow.id).copied();
            match due_decision(last, now, interval) {
                DueDecision::Skip => continue,
                DueDecision::Arm => {
                    // 时钟语义与 v1 逐字一致：首次观察只上膛不触发。授权单在
                    // 上膛时冻结；冻结失败（无单/单无效/端口故障）不清时钟，
                    // 触发时以「无冻结授权单」路径重新评估。
                    self.freeze_grant(&grants, &workflow).await;
                    self.last_run.write().await.insert(workflow.id.clone(), now);
                    continue;
                }
                DueDecision::Fire => {}
            }
            // 派发前授权闸门：无有效授权单（缺失/撤销/过期/版本前移/端口故障）
            // 一律拒绝，不落到既有执行路径。
            if let Err(reason) = self.authorize_dispatch(&grants, &workflow).await {
                tracing::warn!(
                    workflow_id = %workflow.id,
                    reason,
                    "scheduled workflow dispatch rejected by schedule grant"
                );
                // 拒绝也推进时钟：每个 interval 重做一次完整校验，不逐秒重试。
                self.last_run.write().await.insert(workflow.id.clone(), now);
                continue;
            }
            {
                let mut inflight = self.in_flight.write().await;
                if !inflight.insert(workflow.id.clone()) {
                    continue;
                }
            }
            let scheduler = Arc::clone(self);
            let id = workflow.id.clone();
            let app = handle.clone();
            tokio::spawn(async move {
                // RAII guard: guarantees `in_flight.remove` on drop (normal,
                // panic, or cancellation).
                let _guard = InFlightGuard {
                    scheduler: Arc::clone(&scheduler),
                    id: id.clone(),
                };

                // catch_unwind ensures a panic inside the workflow execution
                // does not prevent the guard from running; the panic is logged
                // and `last_run` is still updated to avoid rapid retries.
                let outcome = AssertUnwindSafe(async { app.execute_workflow(&id).await })
                    .catch_unwind()
                    .await;

                match outcome {
                    Ok(Ok(r)) if r.success => {
                        tracing::info!(workflow_id = %id, "scheduled workflow completed");
                    }
                    Ok(Ok(r)) => {
                        tracing::warn!(
                            workflow_id = %id,
                            error = ?r.error,
                            "scheduled workflow finished with errors"
                        );
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(workflow_id = %id, error = %e, "scheduled workflow failed");
                    }
                    Err(_panic) => {
                        tracing::warn!(
                            workflow_id = %id,
                            "scheduled workflow panicked"
                        );
                    }
                }
                // Update last_run regardless of outcome (including panic) to
                // prevent rapid-fire retries on a consistently crashing workflow.
                scheduler.last_run.write().await.insert(id, Instant::now());
                // _guard drops here → in_flight.remove guaranteed
            });
        }
    }

    /// 上膛冻结：把当次读到的有效授权单冻结进 `granted`。
    ///
    /// 读不到或单本体无效 ⇒ 清空冻结槽（触发时的重读路径会拒绝并重新评估）。
    async fn freeze_grant(
        &self,
        grants: &Arc<dyn WorkflowScheduleGrantReader>,
        workflow: &WorkflowDefinition,
    ) {
        let current = match grants.schedule_grant(&workflow.id).await {
            Ok(current) => current,
            Err(error) => {
                tracing::warn!(
                    workflow_id = %workflow.id,
                    %error,
                    "workflow scheduler: grant lookup failed while arming"
                );
                self.granted.write().await.remove(&workflow.id);
                return;
            }
        };
        let mut frozen = self.granted.write().await;
        match current {
            Some(grant) if grant.validate(chrono::Utc::now()).is_ok() => {
                frozen.insert(workflow.id.clone(), grant);
            }
            _ => {
                frozen.remove(&workflow.id);
            }
        }
    }

    /// 派发前授权：返回放行所依据的当前授权单，或拒绝原因。
    ///
    /// 语义（monitor 的「admission 冻结 → 派发前重读比对」同裁决）：
    /// 1. 端口故障 ⇒ 拒绝（失败关闭）；
    /// 2. 无授权单 ⇒ 拒绝并清空冻结槽；
    /// 3. 单本体无效（撤销/过期/版本 0/作用域空）⇒ 拒绝并清空冻结槽；
    /// 4. 当前单 ≠ 上膛冻结单（撤销版本前移/被替换/定义变更）⇒ 拒绝本次派发，
    ///    并以当前单重新上膛——下个 interval 起在新授权下派发；
    /// 5. 冻结槽为空（上膛时还没有有效授权单）⇒ 以当前有效单上膛并放行：
    ///    派发依据是「派发时刻的当前有效授权单」，冻结比对从下一次触发生效。
    async fn authorize_dispatch(
        &self,
        grants: &Arc<dyn WorkflowScheduleGrantReader>,
        workflow: &WorkflowDefinition,
    ) -> Result<WorkflowScheduleGrant, &'static str> {
        let current = match grants.schedule_grant(&workflow.id).await {
            Ok(current) => current,
            Err(error) => {
                tracing::warn!(
                    workflow_id = %workflow.id,
                    %error,
                    "workflow scheduler: grant lookup failed before dispatch"
                );
                return Err("schedule grant lookup failed");
            }
        };
        let Some(current) = current else {
            self.granted.write().await.remove(&workflow.id);
            return Err("no workflow schedule grant");
        };
        if let Err(reason) = current.validate(chrono::Utc::now()) {
            self.granted.write().await.remove(&workflow.id);
            return Err(reason);
        }
        let mut frozen = self.granted.write().await;
        match frozen.get(&workflow.id).cloned() {
            None => {
                frozen.insert(workflow.id.clone(), current.clone());
                Ok(current)
            }
            Some(previous) if previous != current => {
                // 版本前移：撤销、重发或定义变更。本次拒绝，换新单重新上膛。
                frozen.insert(workflow.id.clone(), current.clone());
                Err("workflow schedule grant changed since arm (revoked or superseded)")
            }
            Some(_) => Ok(current),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::error::PortError;
    use crate::shared::id::Counter;
    use crate::workflow::domain::grant::{definition_revision, WorkflowScheduleScope};
    use crate::workflow::domain::model::{WorkflowDefinition, WorkflowSchedule};

    fn wf(enabled: bool, interval: Option<u64>) -> WorkflowDefinition {
        WorkflowDefinition {
            database: None,
            id: "job".into(),
            name: "Job".into(),
            description: String::new(),
            version: None,
            blocks: Vec::new(),
            author: None,
            variables: vec![],
            connection: None,
            steps: vec![],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: Some(WorkflowSchedule {
                enabled,
                interval_secs: interval,
            }),
            visibility: Default::default(),
        }
    }

    #[test]
    fn disabled_or_missing_is_not_scheduled() {
        assert!(scheduled_interval_secs(&wf(false, Some(60))).is_none());
        let mut no = wf(true, Some(60));
        no.schedule = None;
        assert!(scheduled_interval_secs(&no).is_none());
    }

    #[test]
    fn clamps_below_minimum() {
        assert_eq!(
            scheduled_interval_secs(&wf(true, Some(5))),
            Some(MIN_INTERVAL_SECS)
        );
        assert_eq!(scheduled_interval_secs(&wf(true, None)), Some(3600));
    }

    #[test]
    fn never_run_arms_instead_of_firing() {
        let now = Instant::now();
        assert_eq!(due_decision(None, now, 60), DueDecision::Arm);
        assert!(!is_workflow_due(None, now, 60));
        assert_eq!(due_decision(Some(now), now, 60), DueDecision::Skip);
        assert!(!is_workflow_due(Some(now), now, 60));
        assert_eq!(
            due_decision(Some(now - Duration::from_secs(60)), now, 60),
            DueDecision::Fire
        );
        assert!(is_workflow_due(
            Some(now - Duration::from_secs(60)),
            now,
            60
        ));
    }

    #[test]
    fn clamps_interval_helper() {
        assert_eq!(clamp_interval_secs(1), MIN_INTERVAL_SECS);
        assert_eq!(clamp_interval_secs(120), 120);
    }

    #[test]
    fn scheduled_workflow_keeps_its_interval_regardless_of_step_shape() {
        let workflow = wf(true, Some(60));

        // The scheduler delegates the complete definition to the normal
        // workflow executor, so every workflow keeps the same interval and
        // first-observation arming semantics.
        assert_eq!(scheduled_interval_secs(&workflow), Some(60));
        let now = Instant::now();
        assert_eq!(due_decision(None, now, 60), DueDecision::Arm);
    }

    /// Verify that `InFlightGuard` removes the workflow id from `in_flight`
    /// when it is dropped (normal completion path).
    #[tokio::test]
    async fn in_flight_guard_removes_on_normal_drop() {
        let scheduler = WorkflowScheduler::new();
        let wf_id = "panic-test-wf".to_string();

        // Simulate inserting into in_flight (as tick() does).
        scheduler.in_flight.write().await.insert(wf_id.clone());
        assert!(scheduler.in_flight.read().await.contains(&wf_id));

        {
            let _guard = InFlightGuard {
                scheduler: Arc::clone(&scheduler),
                id: wf_id.clone(),
            };
            // Guard alive — id still in set.
            assert!(scheduler.in_flight.read().await.contains(&wf_id));
        }
        // Guard dropped — id must be removed.
        assert!(!scheduler.in_flight.read().await.contains(&wf_id));
    }

    /// Simulate the panic path: a future that panics is wrapped with a guard;
    /// after the panic the id must be cleaned up and the workflow re-triggerable.
    #[tokio::test]
    async fn in_flight_cleanup_after_panic() {
        let scheduler = WorkflowScheduler::new();
        let wf_id = "panicky-wf".to_string();

        // Insert into in_flight as tick() would.
        scheduler.in_flight.write().await.insert(wf_id.clone());

        // Simulate the spawn body: guard + catch_unwind around a panicking body.
        let result = {
            let _guard = InFlightGuard {
                scheduler: Arc::clone(&scheduler),
                id: wf_id.clone(),
            };
            AssertUnwindSafe(async {
                panic!("workflow execution panic");
            })
            .catch_unwind()
            .await
        };
        // The panic was caught.
        assert!(result.is_err());

        // Guard has been dropped — id must be cleaned from in_flight.
        assert!(!scheduler.in_flight.read().await.contains(&wf_id));

        // The workflow can be inserted again (re-triggerable).
        assert!(scheduler.in_flight.write().await.insert(wf_id.clone()));
        // Clean up.
        scheduler.in_flight.write().await.remove(&wf_id);
    }

    /// Verify re-triggerability after normal completion (guard drop + id removed).
    #[tokio::test]
    async fn in_flight_retriggerable_after_normal_completion() {
        let scheduler = WorkflowScheduler::new();
        let wf_id = "normal-wf".to_string();

        scheduler.in_flight.write().await.insert(wf_id.clone());
        assert!(!scheduler.in_flight.write().await.insert(wf_id.clone()));
        // Already in set — duplicate insert returns false (same as tick's skip).

        {
            let _guard = InFlightGuard {
                scheduler: Arc::clone(&scheduler),
                id: wf_id.clone(),
            };
        }
        // After guard drop, id is removed.
        assert!(!scheduler.in_flight.read().await.contains(&wf_id));
        // Now re-insert succeeds — workflow is re-triggerable.
        assert!(scheduler.in_flight.write().await.insert(wf_id.clone()));
        scheduler.in_flight.write().await.remove(&wf_id);
    }

    // ── 调度授权链的行为钉子 ─────────────────────────────────────────────

    const DEFINITION: &str = "id: job\nsteps: []\n";

    fn recorded_host(
        workflows: Vec<WorkflowDefinition>,
    ) -> (Arc<RecordingHost>, Arc<Mutex<Vec<String>>>) {
        let executed = Arc::new(Mutex::new(Vec::new()));
        (
            Arc::new(RecordingHost {
                workflows,
                executed: Arc::clone(&executed),
            }),
            executed,
        )
    }

    struct RecordingHost {
        workflows: Vec<WorkflowDefinition>,
        executed: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl WorkflowScheduleHost for RecordingHost {
        async fn list_workflows(&self) -> Result<Vec<WorkflowDefinition>, String> {
            Ok(self.workflows.clone())
        }
        async fn execute_workflow(&self, id: &str) -> Result<WorkflowExecutionResult, String> {
            self.executed
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(id.to_string());
            Ok(WorkflowExecutionResult {
                success: true,
                final_output: String::new(),
                steps: vec![],
                total_time_ms: 0,
                error: None,
            })
        }
    }

    /// 可变授权单表：测试在「上膛」与「触发」之间改写授权单，
    /// 以驱动撤销版本前移 / 撤销 / 过期 / 自愈路径。
    struct MutableGrants {
        grants: std::sync::Mutex<HashMap<String, WorkflowScheduleGrant>>,
    }

    impl MutableGrants {
        fn with(grants: Vec<WorkflowScheduleGrant>) -> Self {
            Self {
                grants: std::sync::Mutex::new(
                    grants
                        .into_iter()
                        .map(|grant| (grant.workflow_id.clone(), grant))
                        .collect(),
                ),
            }
        }
    }

    #[async_trait::async_trait]
    impl WorkflowScheduleGrantReader for MutableGrants {
        async fn schedule_grant(
            &self,
            workflow_id: &str,
        ) -> Result<Option<WorkflowScheduleGrant>, PortError> {
            Ok(self
                .grants
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get(workflow_id)
                .cloned())
        }
    }

    fn grant_for(workflow_id: &str, version: u64) -> WorkflowScheduleGrant {
        WorkflowScheduleGrant {
            workflow_id: workflow_id.to_string(),
            workflow_revision: definition_revision(DEFINITION),
            organization_id: "org-1".into(),
            service_principal: format!("workflow-schedule:{workflow_id}:v{version}"),
            approved_by: "user-1".into(),
            scopes: vec![WorkflowScheduleScope::namespace(
                "conn-1",
                Some("app".into()),
                None,
                Counter::new(1).get(),
                "query",
            )],
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            revocation_version: version,
            revoked: false,
        }
    }

    fn bound_scheduler(
        workflows: Vec<WorkflowDefinition>,
        grants: Arc<dyn WorkflowScheduleGrantReader>,
    ) -> (Arc<WorkflowScheduler>, Arc<Mutex<Vec<String>>>) {
        let scheduler = WorkflowScheduler::new();
        let (host, executed) = recorded_host(workflows);
        scheduler.bind(host, grants);
        (scheduler, executed)
    }

    /// 让上膛的时钟走到触发点（把 last_run 回拨到 interval 之前）。
    async fn make_due(scheduler: &Arc<WorkflowScheduler>, workflow_id: &str, interval: u64) {
        scheduler.last_run.write().await.insert(
            workflow_id.to_string(),
            Instant::now() - Duration::from_secs(interval + 1),
        );
    }

    /// 等待派发任务在单线程测试运行时上完成。
    async fn settle() {
        for _ in 0..16 {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    /// 行为钉子：授权单有效 → 上膛不触发（v1 语义不变），到点派发。
    #[tokio::test]
    async fn a_valid_schedule_grant_dispatches_the_due_workflow() {
        let grants = Arc::new(MutableGrants::with(vec![grant_for("job", 1)]));
        let (scheduler, executed) = bound_scheduler(vec![wf(true, Some(60))], grants);

        scheduler.tick().await;
        assert!(
            executed
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "首次观察只上膛，不派发（v1 语义逐字不变）"
        );

        make_due(&scheduler, "job", 60).await;
        scheduler.tick().await;
        settle().await;
        assert_eq!(
            *executed.lock().unwrap_or_else(|error| error.into_inner()),
            vec!["job".to_string()],
            "授权单有效 ⇒ 到点派发"
        );
    }

    /// 行为钉子：授权单缺失 → 拒绝派发，不落既有执行路径。
    #[tokio::test]
    async fn a_missing_schedule_grant_rejects_dispatch() {
        let grants: Arc<dyn WorkflowScheduleGrantReader> = Arc::new(MutableGrants::with(vec![]));
        let (scheduler, executed) = bound_scheduler(vec![wf(true, Some(60))], grants);

        scheduler.tick().await;
        make_due(&scheduler, "job", 60).await;
        scheduler.tick().await;
        settle().await;
        assert!(
            executed
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "无授权单 ⇒ 拒绝派发"
        );
        // 拒绝推进时钟：授权单出现后，下个 interval 恢复派发（自愈）。
        let armed = scheduler.last_run.read().await.get("job").copied();
        assert!(armed.is_some(), "拒绝也必须推进时钟，避免逐秒重试");
    }

    /// 行为钉子：撤销版本前移（上膛冻结 v1，触发时已是 v2）→ 拒绝本次派发，
    /// 并以新单重新上膛——下个 interval 在新授权下派发。
    #[tokio::test]
    async fn a_moved_revocation_version_rejects_dispatch_then_rearms() {
        let table = Arc::new(MutableGrants::with(vec![grant_for("job", 1)]));
        let reader: Arc<dyn WorkflowScheduleGrantReader> = Arc::clone(&table) as _;
        let (scheduler, executed) = bound_scheduler(vec![wf(true, Some(60))], reader);

        scheduler.tick().await; // 上膛：冻结 v1
                                // 授权单被撤销并重发：版本前移到 2。
        table
            .grants
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert("job".into(), grant_for("job", 2));
        make_due(&scheduler, "job", 60).await;
        scheduler.tick().await;
        settle().await;
        assert!(
            executed
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "撤销版本前移 ⇒ 拒绝本次派发"
        );

        // 重新上膛完成：下个 interval 在新授权下派发。
        make_due(&scheduler, "job", 60).await;
        scheduler.tick().await;
        settle().await;
        assert_eq!(
            *executed.lock().unwrap_or_else(|error| error.into_inner()),
            vec!["job".to_string()],
            "新授权单上膛后恢复派发"
        );
    }

    /// 行为钉子：授权单过期或已撤销（单本体失效）→ 拒绝派发。
    #[tokio::test]
    async fn expired_or_revoked_grants_reject_dispatch() {
        let mut expired = grant_for("job", 1);
        expired.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1);
        let mut revoked = grant_for("job-2", 1);
        revoked.revoked = true;

        let grants = Arc::new(MutableGrants::with(vec![expired, revoked]));
        let (scheduler, executed) = bound_scheduler(
            vec![wf(true, Some(60)), {
                let mut second = wf(true, Some(60));
                second.id = "job-2".into();
                second
            }],
            grants,
        );

        scheduler.tick().await;
        make_due(&scheduler, "job", 60).await;
        make_due(&scheduler, "job-2", 60).await;
        scheduler.tick().await;
        settle().await;
        assert!(
            executed
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "过期/已撤销的授权单 ⇒ 拒绝派发"
        );
    }

    /// 行为钉子：授权单端口故障（Err）→ 失败关闭，拒绝派发。
    #[tokio::test]
    async fn a_failing_grant_port_rejects_dispatch() {
        struct BrokenReader;
        #[async_trait::async_trait]
        impl WorkflowScheduleGrantReader for BrokenReader {
            async fn schedule_grant(
                &self,
                _workflow_id: &str,
            ) -> Result<Option<WorkflowScheduleGrant>, PortError> {
                Err(PortError::BackendUnavailable("grant store offline".into()))
            }
        }
        let grants: Arc<dyn WorkflowScheduleGrantReader> = Arc::new(BrokenReader);
        let (scheduler, executed) = bound_scheduler(vec![wf(true, Some(60))], grants);

        scheduler.tick().await;
        make_due(&scheduler, "job", 60).await;
        scheduler.tick().await;
        settle().await;
        assert!(
            executed
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "端口故障 ⇒ 失败关闭，绝不降级为无授权派发"
        );
    }

    /// 行为钉子：定义变更 ⇒ 授权单的冻结修订前移 ⇒ 拒绝本 tick，换新单上膛。
    #[tokio::test]
    async fn a_changed_definition_moves_the_frozen_revision_and_rejects_once() {
        let first = grant_for("job", 1);
        let mut changed = grant_for("job", 1);
        changed.workflow_revision = definition_revision("id: job\nsteps: [changed]\n");
        changed.service_principal =
            crate::workflow::domain::grant::service_principal("job", &changed.workflow_revision);

        let table = Arc::new(MutableGrants::with(vec![first]));
        let reader: Arc<dyn WorkflowScheduleGrantReader> = Arc::clone(&table) as _;
        let (scheduler, executed) = bound_scheduler(vec![wf(true, Some(60))], reader);

        scheduler.tick().await; // 上膛：冻结旧修订
        table
            .grants
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert("job".into(), changed);
        make_due(&scheduler, "job", 60).await;
        scheduler.tick().await;
        settle().await;
        assert!(
            executed
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "定义变更 ⇒ 冻结修订前移 ⇒ 拒绝本 tick"
        );

        make_due(&scheduler, "job", 60).await;
        scheduler.tick().await;
        settle().await;
        assert_eq!(
            *executed.lock().unwrap_or_else(|error| error.into_inner()),
            vec!["job".to_string()],
            "新修订的授权单上膛后恢复派发"
        );
    }

    /// 行为钉子：上膛时无有效授权单、触发时出现 ⇒ 按派发时刻的当前有效单
    /// 放行（冻结比对从下一次触发生效）。
    #[tokio::test]
    async fn a_grant_that_appears_after_arming_dispatches_on_the_next_due_tick() {
        let table = Arc::new(MutableGrants::with(vec![]));
        let reader: Arc<dyn WorkflowScheduleGrantReader> = Arc::clone(&table) as _;
        let (scheduler, executed) = bound_scheduler(vec![wf(true, Some(60))], reader);

        scheduler.tick().await; // 上膛：无单可冻结
        table
            .grants
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert("job".into(), grant_for("job", 1));
        make_due(&scheduler, "job", 60).await;
        scheduler.tick().await;
        settle().await;
        assert_eq!(
            *executed.lock().unwrap_or_else(|error| error.into_inner()),
            vec!["job".to_string()],
            "派发时刻存在有效授权单 ⇒ 放行"
        );
        assert!(
            scheduler.granted.read().await.contains_key("job"),
            "派发以当前有效单上膛，下一次触发起进入冻结比对"
        );
    }

    /// 行为钉子：授权单端口未装配 ⇒ 完全不派发（失败关闭）。
    #[tokio::test]
    async fn an_unbound_grant_reader_disables_dispatch_entirely() {
        let scheduler = WorkflowScheduler::new();
        let (host, executed) = recorded_host(vec![wf(true, Some(60))]);
        *scheduler
            .host
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(host);

        scheduler
            .last_run
            .write()
            .await
            .insert("job".into(), Instant::now() - Duration::from_secs(61));
        scheduler.tick().await;
        settle().await;
        assert!(
            executed
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "无授权事实来源 ⇒ 不派发"
        );
    }
}
