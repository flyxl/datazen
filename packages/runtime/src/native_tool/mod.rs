//! Job-owned native execution. DB sockets use the existing physical coordinator;
//! a separate semaphore counts OS processes, never pretends one process is one DB
//! socket. Unconfirmed cleanup retains BOTH reservations; effects never replay.
mod handler;
use crate::{
    budget::{BudgetClaim, InProcessBudgetCoordinator, ResourceClass},
    job::{CancelToken, StageOwnerPermit},
};
use datazen_platform_api::{
    dto::execution::EffectOutcome, error::PortError, ports::native_tool::*,
};
use futures_util::FutureExt;
pub use handler::NativeToolJobHandler;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{Notify, Semaphore};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeToolResult {
    pub report: Option<NativeToolReport>,
    pub effect: EffectOutcome,
    pub rejection: Option<&'static str>,
}
struct Entry {
    selection: NativeToolSelection,
    result: Option<NativeToolResult>,
}
pub struct NativeToolRuntime {
    port: Arc<dyn NativeToolPort>,
    coordinator: Arc<InProcessBudgetCoordinator>,
    processes: Arc<Semaphore>,
    limits: NativeToolLimits,
    entries: Mutex<HashMap<String, Entry>>,
    wake: Notify,
}
impl NativeToolRuntime {
    /// One instance per host execution environment; do not instantiate per Job.
    pub fn new(
        port: Arc<dyn NativeToolPort>,
        coordinator: Arc<InProcessBudgetCoordinator>,
        max_processes: usize,
        limits: NativeToolLimits,
    ) -> Result<Arc<Self>, PortError> {
        if !(1..=64).contains(&max_processes)
            || !(1..=1_048_576).contains(&limits.max_output_bytes)
            || !(1..=300_000).contains(&limits.timeout_ms)
            || !(1..=5_000).contains(&limits.terminate_grace_ms)
        {
            return Err(PortError::QuotaExceeded("native limits".into()));
        }
        Ok(Arc::new(Self {
            port,
            coordinator,
            processes: Arc::new(Semaphore::new(max_processes)),
            limits,
            entries: Mutex::new(HashMap::new()),
            wake: Notify::new(),
        }))
    }
    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub async fn execute(
        self: &Arc<Self>,
        owner: StageOwnerPermit,
        selection: NativeToolSelection,
        cancel: CancelToken,
    ) -> Result<NativeToolResult, PortError> {
        owner
            .verify_native_selection(&selection)
            .await
            .map_err(|_| PortError::StaleClaim)?;
        let key = format!(
            "{}:{}{}",
            owner.job_id().as_str().len(),
            owner.job_id().as_str(),
            owner.stage_id().as_str()
        );
        let launch = {
            let mut entries = self.entries();
            if let Some(entry) = entries.get(&key) {
                if entry.selection != selection {
                    return Err(PortError::IdempotencyConflict);
                }
                false
            } else {
                // Never evict tombstones while the host can accept that Job again.
                if entries.len() >= 4096 {
                    return Err(PortError::QuotaExceeded("native receipt capacity".into()));
                }
                entries.insert(
                    key.clone(),
                    Entry {
                        selection: selection.clone(),
                        result: None,
                    },
                );
                true
            }
        };
        if launch {
            let runtime = self.clone();
            let task_key = key.clone();
            tokio::spawn(async move {
                runtime.run_once(task_key, owner, selection, cancel).await;
            });
        }
        loop {
            let notified = self.wake.notified();
            if let Some(result) = self
                .entries()
                .get(&key)
                .and_then(|entry| entry.result.clone())
            {
                return Ok(result);
            }
            notified.await;
        }
    }
    fn finish(&self, key: &str, result: NativeToolResult) {
        if let Some(entry) = self.entries().get_mut(key) {
            entry.result = Some(result);
        }
        self.wake.notify_waiters();
    }
    fn reject(&self, key: &str, code: &'static str) {
        self.finish(
            key,
            NativeToolResult {
                report: None,
                effect: EffectOutcome::NotStarted,
                rejection: Some(code),
            },
        );
    }
    async fn run_once(
        self: Arc<Self>,
        key: String,
        owner: StageOwnerPermit,
        selection: NativeToolSelection,
        cancel: CancelToken,
    ) {
        let resolution = match self.port.resolve(&selection) {
            Ok(value) => value,
            Err(_) => {
                self.reject(&key, "toolUnavailable");
                return;
            }
        };
        let sockets = match (resolution.max_processes, resolution.max_database_sockets) {
            (Some(1), Some(cost @ 0..=64))
                if resolution.selection == selection
                    && (cost == 0 || resolution.connection_id.is_some()) =>
            {
                cost
            }
            _ => {
                self.reject(&key, "unprovenResourceBound");
                return;
            }
        };
        let process_permit = match self.processes.clone().try_acquire_owned() {
            Ok(value) => value,
            Err(_) => {
                self.reject(&key, "processBudgetDenied");
                return;
            }
        };
        let permits = if sockets == 0 {
            Vec::new()
        } else {
            let Some(connection) = resolution.connection_id.clone() else {
                self.reject(&key, "targetRequired");
                return;
            };
            let context = owner.context();
            let claim = BudgetClaim::single(
                context.organization_id.clone(),
                connection,
                context.principal_id.clone(),
                ResourceClass::Job,
            )
            .with_slots(sockets)
            .pinned();
            match self.coordinator.acquire_cost_now(&claim) {
                Ok(value) => value,
                Err(_) => {
                    self.reject(&key, "databaseBudgetDenied");
                    return;
                }
            }
        };
        if cancel.is_cancelled() || owner.verify_active().await.is_err() {
            for permit in &permits {
                self.coordinator.release_permit(permit, false);
            }
            self.reject(&key, "stageRevoked");
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let run = std::panic::AssertUnwindSafe(self.port.run(
            &key,
            resolution,
            stop.clone(),
            self.limits,
        ))
        .catch_unwind();
        tokio::pin!(run);
        let report = loop {
            tokio::select! {
                report = &mut run => break match report { Ok(value) => value, Err(_) => Ok(NativeToolReport { termination: NativeTermination::Failed, exit_code: None, stdout_bytes: 0, stderr_bytes: 0, cleanup: NativeCleanup::Unconfirmed, version_verified: false }) },
                _ = tokio::time::sleep(Duration::from_millis(25)) => {
                    if cancel.is_cancelled() || owner.verify_active().await.is_err() { stop.store(true, Ordering::Release); }
                }
            }
        };
        let (cleanup, consumed) = match report {
            Err(_) => {
                self.reject(&key, "spawnRejected");
                (NativeCleanup::Confirmed, false)
            }
            Ok(report) => {
                let cleanup = report.cleanup;
                let effect = if sockets > 0 || cleanup != NativeCleanup::Confirmed {
                    EffectOutcome::Unknown
                } else if report.termination == NativeTermination::Exited
                    && report.exit_code == Some(0)
                    && report.version_verified
                {
                    EffectOutcome::Completed
                } else {
                    EffectOutcome::NotStarted
                };
                self.finish(
                    &key,
                    NativeToolResult {
                        report: Some(report),
                        effect,
                        rejection: None,
                    },
                );
                (cleanup, true)
            }
        };
        if cleanup != NativeCleanup::Confirmed {
            // The provider owns the original process/cleanup worker. Poll only cleanup;
            // never invoke run again, and never release budgets for unknown cleanup.
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                // Observer failure is not resource release. Keep both reservations
                // in this owner task and retry only cleanup, never application work.
                let observed = std::panic::AssertUnwindSafe(self.port.cleanup_status(&key))
                    .catch_unwind()
                    .await;
                if matches!(observed, Ok(NativeCleanup::Confirmed)) {
                    break;
                }
            }
        }
        for permit in &permits {
            self.coordinator.release_permit(permit, consumed);
        }
        drop(process_permit);
    }
}
#[cfg(test)]
mod tests;
