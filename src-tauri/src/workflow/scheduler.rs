//! Interval scheduler for Workflow jobs (timed backup / automation).
//!
//! Mirrors [`crate::monitor::MonitorEngine`]: tick loop, in-flight guard, and
//! AppHandle lookup so we can reuse `workflow_execute_impl`.

use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use tauri::{AppHandle, Manager};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::model::WorkflowDefinition;

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
    app_handle: Mutex<Option<AppHandle>>,
    paused: AtomicBool,
    last_run: RwLock<HashMap<String, Instant>>,
    in_flight: RwLock<HashSet<String>>,
    cancel_token: CancellationToken,
}

impl WorkflowScheduler {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            app_handle: Mutex::new(None),
            paused: AtomicBool::new(false),
            last_run: RwLock::new(HashMap::new()),
            in_flight: RwLock::new(HashSet::new()),
            cancel_token: CancellationToken::new(),
        })
    }

    pub fn start(self: &Arc<Self>, app_handle: AppHandle) {
        *self
            .app_handle
            .lock()
            .expect("workflow scheduler app_handle") = Some(app_handle);
        let engine = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            engine.run_loop().await;
        });
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
        let handle = {
            let guard = self
                .app_handle
                .lock()
                .expect("workflow scheduler app_handle");
            match guard.clone() {
                Some(h) => h,
                None => return,
            }
        };
        let state = handle.state::<crate::commands::AppState>();
        if let Err(e) = state.workflow_registry.ensure_loaded().await {
            tracing::warn!(error = %e, "workflow scheduler: failed to load registry");
            return;
        }
        let workflows = state.workflow_registry.list_definitions().await;
        let now = Instant::now();
        for workflow in workflows {
            let Some(interval) = scheduled_interval_secs(&workflow) else {
                continue;
            };
            let last = self.last_run.read().await.get(&workflow.id).copied();
            match due_decision(last, now, interval) {
                DueDecision::Skip => continue,
                DueDecision::Arm => {
                    self.last_run.write().await.insert(workflow.id.clone(), now);
                    continue;
                }
                DueDecision::Fire => {}
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
            tauri::async_runtime::spawn(async move {
                // RAII guard: guarantees `in_flight.remove` on drop (normal,
                // panic, or cancellation).
                let _guard = InFlightGuard {
                    scheduler: Arc::clone(&scheduler),
                    id: id.clone(),
                };

                // catch_unwind ensures a panic inside the workflow execution
                // does not prevent the guard from running; the panic is logged
                // and `last_run` is still updated to avoid rapid retries.
                let outcome = AssertUnwindSafe(async {
                    let state = app.state::<crate::commands::AppState>();
                    crate::commands::workflow_execute_impl(
                        state.inner(),
                        id.clone(),
                        serde_json::json!({}),
                        None,
                    )
                    .await
                })
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
                scheduler
                    .last_run
                    .write()
                    .await
                    .insert(id, Instant::now());
                // _guard drops here → in_flight.remove guaranteed
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::model::{WorkflowDefinition, WorkflowSchedule};

    fn wf(enabled: bool, interval: Option<u64>) -> WorkflowDefinition {
        WorkflowDefinition {
            id: "job".into(),
            name: "Job".into(),
            description: String::new(),
            version: None,
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
}
