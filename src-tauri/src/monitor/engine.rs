//! Background scheduler for dashboard widget refresh.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use tokio::sync::{RwLock, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::commands::AppState;
use crate::dashboard::execute::{execute_widget_once, DashboardExecuteError};
use crate::dashboard::store::{
    list_dashboards, load_monitor_settings, set_dashboard_refresh_paused,
};
use crate::dashboard::types::{Dashboard, DashboardWidget, RefreshMode, WidgetRun};
use crate::monitor::channels::{dispatch_notifications, AlertChannelState};
use crate::store::Store;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledWidget {
    pub dashboard_id: String,
    pub widget_id: String,
    pub workflow_id: String,
    pub refresh_sec: u32,
}

/// Build the monitor schedule from persisted dashboards (no DB I/O).
pub fn build_schedule_table(dashboards: &[Dashboard]) -> Vec<ScheduledWidget> {
    let mut entries = Vec::new();
    for dashboard in dashboards {
        if !dashboard.enabled || dashboard.refresh_paused {
            continue;
        }
        for widget in &dashboard.widgets {
            if !widget.enabled {
                continue;
            }
            if widget.refresh.mode != RefreshMode::Interval {
                continue;
            }
            let Some(refresh_sec) = widget.refresh.interval_secs() else {
                continue;
            };
            entries.push(ScheduledWidget {
                dashboard_id: dashboard.id.clone(),
                widget_id: widget.id.clone(),
                workflow_id: widget.workflow_id.clone(),
                refresh_sec,
            });
        }
    }
    entries
}

/// Whether the scheduler should spawn a tick for a widget.
pub(crate) fn should_spawn_widget_tick(due: bool, in_flight: bool) -> bool {
    due && !in_flight
}

/// Returns true when `refresh_sec` has elapsed since `last_run` (or never run).
pub(crate) fn is_widget_refresh_due(
    last_run: Option<Instant>,
    now: Instant,
    refresh_sec: u32,
) -> bool {
    match last_run {
        None => true,
        Some(t) => now.duration_since(t).as_secs() >= refresh_sec as u64,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RunUpdatedPayload {
    dashboard_id: String,
    widget_id: String,
    run: WidgetRun,
}

#[derive(Clone)]
struct ScheduleEntry {
    dashboard_id: String,
    widget: DashboardWidget,
    refresh_sec: u32,
}

pub struct MonitorEngine {
    store: Arc<Store>,
    app_state: Mutex<Option<Arc<AppState>>>,
    app_handle: Mutex<Option<AppHandle>>,
    active_widget_count: AtomicUsize,
    schedule: RwLock<Vec<ScheduleEntry>>,
    dashboard_names: RwLock<HashMap<String, String>>,
    alert_channels: tokio::sync::Mutex<AlertChannelState>,
    last_run: RwLock<HashMap<(String, String), Instant>>,
    in_flight: RwLock<HashSet<(String, String)>>,
    workflow_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    semaphore: RwLock<Arc<Semaphore>>,
    cancel_token: CancellationToken,
}

impl MonitorEngine {
    pub fn new(store: Arc<Store>) -> Arc<Self> {
        Arc::new(Self {
            store,
            app_state: Mutex::new(None),
            app_handle: Mutex::new(None),
            active_widget_count: AtomicUsize::new(0),
            schedule: RwLock::new(Vec::new()),
            dashboard_names: RwLock::new(HashMap::new()),
            alert_channels: tokio::sync::Mutex::new(AlertChannelState::new()),
            last_run: RwLock::new(HashMap::new()),
            in_flight: RwLock::new(HashSet::new()),
            workflow_locks: Mutex::new(HashMap::new()),
            semaphore: RwLock::new(Arc::new(Semaphore::new(2))),
            cancel_token: CancellationToken::new(),
        })
    }

    pub fn attach_app_state(self: &Arc<Self>, app_state: Arc<AppState>) {
        *self.app_state.lock().expect("monitor app_state lock") = Some(app_state);
    }

    pub fn start(self: &Arc<Self>, app_handle: AppHandle) {
        *self.app_handle.lock().expect("monitor app_handle lock") = Some(app_handle);
        let engine = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            if let Err(e) = engine.reload_from_store().await {
                tracing::warn!(error = %e, "monitor engine initial reload failed");
            }
            loop {
                tokio::select! {
                    () = engine.cancel_token.cancelled() => break,
                    () = tokio::time::sleep(Duration::from_secs(1)) => {
                        engine.poll_due_widgets().await;
                    }
                }
            }
        });
    }

    pub async fn reload_from_store(
        &self,
    ) -> Result<(), crate::dashboard::store::DashboardStoreError> {
        let app_db = self.store.app_db();
        let dashboards = list_dashboards(&app_db)?;
        let table = build_schedule_table(&dashboards);

        let mut schedule = Vec::with_capacity(table.len());
        let mut names = HashMap::new();
        for dashboard in &dashboards {
            names.insert(dashboard.id.clone(), dashboard.name.clone());
            if !dashboard.enabled || dashboard.refresh_paused {
                continue;
            }
            for widget in &dashboard.widgets {
                if !widget.enabled || widget.refresh.mode != RefreshMode::Interval {
                    continue;
                }
                let Some(refresh_sec) = widget.refresh.interval_secs() else {
                    continue;
                };
                schedule.push(ScheduleEntry {
                    dashboard_id: dashboard.id.clone(),
                    widget: widget.clone(),
                    refresh_sec,
                });
            }
        }
        let count = schedule.len();
        *self.schedule.write().await = schedule;
        *self.dashboard_names.write().await = names;
        self.active_widget_count.store(count, Ordering::Relaxed);

        let settings = self.store.get_settings().await;
        let max = load_monitor_settings(&settings)
            .max_concurrent_queries
            .max(1) as usize;
        *self.semaphore.write().await = Arc::new(Semaphore::new(max));

        tracing::debug!(count = table.len(), "monitor schedule reloaded");

        let app = self
            .app_handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(app) = app {
            crate::tray::sync_tray_async(&app).await;
        }

        Ok(())
    }

    pub fn is_monitoring_active(&self) -> bool {
        self.active_widget_count.load(Ordering::Relaxed) > 0
    }

    /// Legacy global pause — tray toggles refresh_paused on every dashboard.
    pub fn is_paused(&self) -> bool {
        let Some(app_state) = self.app_state() else {
            return false;
        };
        let app_db = app_state.store.app_db();
        match list_dashboards(&app_db) {
            Ok(dashboards) if !dashboards.is_empty() => dashboards.iter().all(|d| d.refresh_paused),
            _ => false,
        }
    }

    /// Pause or resume scheduled refresh for all dashboards (tray menu).
    pub fn set_paused(&self, paused: bool) {
        let Some(app_state) = self.app_state() else {
            tracing::warn!("set_paused: monitor engine has no app state");
            return;
        };
        let app_db = app_state.store.app_db();
        match list_dashboards(&app_db) {
            Ok(dashboards) => {
                for dashboard in &dashboards {
                    if let Err(e) = set_dashboard_refresh_paused(&app_db, &dashboard.id, paused) {
                        tracing::warn!(
                            dashboard_id = %dashboard.id,
                            error = %e,
                            "failed to set dashboard refresh_paused"
                        );
                    }
                }
                if let Err(e) =
                    tauri::async_runtime::block_on(app_state.monitor_engine.reload_from_store())
                {
                    tracing::warn!(error = %e, "failed to reload monitor after set_paused");
                }
                if let Some(app) = self.app_handle() {
                    crate::tray::sync_tray(&app);
                }
            }
            Err(e) => tracing::warn!(error = %e, "set_paused: failed to list dashboards"),
        }
    }

    pub fn app_handle(&self) -> Option<AppHandle> {
        self.app_handle
            .lock()
            .expect("monitor app_handle lock")
            .clone()
    }

    fn app_state(&self) -> Option<Arc<AppState>> {
        self.app_state
            .lock()
            .expect("monitor app_state lock")
            .clone()
    }

    /// Execute one widget refresh (shared by manual refresh and the scheduler).
    pub async fn tick_widget(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<WidgetRun, DashboardExecuteError> {
        self.tick_widget_inner(dashboard_id, widget, true).await
    }

    async fn tick_widget_inner(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
        emit_event: bool,
    ) -> Result<WidgetRun, DashboardExecuteError> {
        let app_state = self.app_state().ok_or_else(|| {
            DashboardExecuteError::Workflow("App state not attached to monitor engine".into())
        })?;

        let sem = self.semaphore.read().await.clone();
        let _permit = sem
            .acquire()
            .await
            .map_err(|_| DashboardExecuteError::Workflow("monitor semaphore closed".into()))?;

        let workflow_lock = {
            let mut locks = self.workflow_locks.lock().expect("workflow_locks mutex");
            locks
                .entry(widget.workflow_id.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _workflow_guard = workflow_lock.lock().await;

        let settings = self.store.get_settings().await;
        let app_db = self.store.app_db();
        let run = execute_widget_once(&app_state, &app_db, &settings, dashboard_id, widget).await?;

        self.dispatch_alert_channels(dashboard_id, widget, &run, &settings)
            .await;

        if emit_event {
            self.emit_run_updated(dashboard_id, &widget.id, &run);
        }

        Ok(run)
    }

    async fn dispatch_alert_channels(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
        run: &WidgetRun,
        settings: &crate::store::AppSettings,
    ) {
        let dashboard_name = self
            .dashboard_names
            .read()
            .await
            .get(dashboard_id)
            .cloned()
            .unwrap_or_else(|| dashboard_id.to_string());
        let monitor_settings = load_monitor_settings(settings);
        let app = self
            .app_handle
            .lock()
            .expect("monitor app_handle lock")
            .clone();
        let (notifications, client) = {
            let mut channels = self.alert_channels.lock().await;
            let notifications =
                channels.process_run_state(dashboard_id, &dashboard_name, widget, run);
            let client = channels.http_client().clone();
            (notifications, client)
        };
        dispatch_notifications(app.as_ref(), &monitor_settings, &client, &notifications).await;
    }

    fn emit_run_updated(&self, dashboard_id: &str, widget_id: &str, run: &WidgetRun) {
        let handle = self
            .app_handle
            .lock()
            .expect("monitor app_handle lock")
            .clone();
        let Some(app) = handle else {
            return;
        };
        let payload = RunUpdatedPayload {
            dashboard_id: dashboard_id.to_string(),
            widget_id: widget_id.to_string(),
            run: run.clone(),
        };
        let _ = app.emit("dashboard:run-updated", &payload);
    }

    async fn poll_due_widgets(self: &Arc<Self>) {
        let schedule = self.schedule.read().await.clone();
        let now = Instant::now();

        for entry in schedule {
            let key = (entry.dashboard_id.clone(), entry.widget.id.clone());
            let (due, already_in_flight) = {
                let last = self.last_run.read().await;
                let in_flight = self.in_flight.read().await;
                let due = is_widget_refresh_due(last.get(&key).copied(), now, entry.refresh_sec);
                (due, in_flight.contains(&key))
            };
            if !should_spawn_widget_tick(due, already_in_flight) {
                continue;
            }

            self.in_flight.write().await.insert(key.clone());

            let engine = Arc::clone(self);
            let dashboard_id = entry.dashboard_id.clone();
            let widget = entry.widget.clone();
            tokio::spawn(async move {
                let result = engine.tick_widget_inner(&dashboard_id, &widget, true).await;

                {
                    let clear_key = (dashboard_id.clone(), widget.id.clone());
                    engine.in_flight.write().await.remove(&clear_key);
                    engine
                        .last_run
                        .write()
                        .await
                        .insert(clear_key, Instant::now());
                }

                if let Err(e) = result {
                    tracing::warn!(
                        dashboard_id = %dashboard_id,
                        widget_id = %widget.id,
                        error = %e,
                        "monitor scheduler tick failed"
                    );
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::types::{
        AggregationType, ChartConfig, ChartSortBy, ChartType, DashboardLayout, RefreshMode,
        RefreshPolicy, ViewMode, WidgetLayout,
    };

    fn sample_widget(id: &str, refresh: RefreshPolicy, enabled: bool) -> DashboardWidget {
        DashboardWidget {
            id: id.into(),
            title: "Test".into(),
            workflow_id: "wf-1".into(),
            view_mode: ViewMode::Chart,
            chart_config: Some(ChartConfig {
                chart_type: ChartType::Line,
                x_axis: None,
                y_axes: vec![],
                group_by: None,
                aggregation: AggregationType::None,
                sort_by: ChartSortBy::None,
                show_legend: true,
                show_grid: true,
                show_values: false,
                color_scheme: "default".into(),
            }),
            layout: WidgetLayout {
                x: 0,
                y: 0,
                w: 4,
                h: 3,
            },
            refresh,
            alert: None,
            enabled,
        }
    }

    fn sample_dashboard(
        id: &str,
        enabled: bool,
        refresh_paused: bool,
        widgets: Vec<DashboardWidget>,
    ) -> Dashboard {
        Dashboard {
            id: id.into(),
            name: "Ops".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            layout: DashboardLayout {
                cols: 12,
                row_height: 80,
            },
            widgets,
            enabled,
            refresh_paused,
        }
    }

    #[test]
    fn build_schedule_table_only_interval_widgets() {
        let dashboards = vec![sample_dashboard(
            "d1",
            true,
            false,
            vec![
                sample_widget(
                    "w1",
                    RefreshPolicy {
                        mode: RefreshMode::Manual,
                        refresh_sec: None,
                    },
                    true,
                ),
                sample_widget(
                    "w2",
                    RefreshPolicy {
                        mode: RefreshMode::Interval,
                        refresh_sec: Some(90),
                    },
                    true,
                ),
            ],
        )];

        let table = build_schedule_table(&dashboards);
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].widget_id, "w2");
        assert_eq!(table[0].refresh_sec, 90);
    }

    #[test]
    fn build_schedule_table_skips_paused_dashboard() {
        let dashboards = vec![sample_dashboard(
            "d1",
            true,
            true,
            vec![sample_widget(
                "w1",
                RefreshPolicy {
                    mode: RefreshMode::Interval,
                    refresh_sec: Some(60),
                },
                true,
            )],
        )];
        assert!(build_schedule_table(&dashboards).is_empty());
    }

    #[test]
    fn build_schedule_table_clamps_refresh_sec() {
        let dashboards = vec![sample_dashboard(
            "d1",
            true,
            false,
            vec![sample_widget(
                "w1",
                RefreshPolicy {
                    mode: RefreshMode::Interval,
                    refresh_sec: Some(5),
                },
                true,
            )],
        )];
        let table = build_schedule_table(&dashboards);
        assert_eq!(table[0].refresh_sec, 30);
    }

    #[test]
    fn should_spawn_widget_tick_skips_when_in_flight() {
        assert!(should_spawn_widget_tick(true, false));
        assert!(!should_spawn_widget_tick(true, true));
        assert!(!should_spawn_widget_tick(false, false));
    }

    #[test]
    fn is_widget_refresh_due_respects_interval() {
        let t0 = Instant::now();
        assert!(is_widget_refresh_due(None, t0, 60));
        assert!(!is_widget_refresh_due(Some(t0), t0, 60));
        assert!(is_widget_refresh_due(
            Some(t0),
            t0 + Duration::from_secs(60),
            60
        ));
    }
}
