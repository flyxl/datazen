//! Background scheduler for dashboard widget refresh.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use tokio::sync::{RwLock, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::dashboard::execute::{execute_widget_once, DashboardExecuteError};
use crate::dashboard::store::list_dashboards;
use crate::dashboard::types::{clamp_refresh_sec, Dashboard, DashboardWidget, WidgetRun};
use crate::monitor::MonitorConnectionRegistry;
use crate::store::Store;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledWidget {
    pub dashboard_id: String,
    pub widget_id: String,
    pub config_id: String,
    pub refresh_sec: u32,
}

/// Build the monitor schedule from persisted dashboards (no DB I/O).
pub fn build_schedule_table(dashboards: &[Dashboard]) -> Vec<ScheduledWidget> {
    let mut entries = Vec::new();
    for dashboard in dashboards {
        if !dashboard.enabled {
            continue;
        }
        for widget in &dashboard.widgets {
            if !widget.enabled {
                continue;
            }
            entries.push(ScheduledWidget {
                dashboard_id: dashboard.id.clone(),
                widget_id: widget.id.clone(),
                config_id: widget.config_id.clone(),
                refresh_sec: clamp_refresh_sec(widget.refresh_sec),
            });
        }
    }
    entries
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
    monitor_connections: Arc<MonitorConnectionRegistry>,
    app_handle: Mutex<Option<AppHandle>>,
    paused: AtomicBool,
    schedule: RwLock<Vec<ScheduleEntry>>,
    last_run: RwLock<HashMap<(String, String), Instant>>,
    config_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    semaphore: RwLock<Arc<Semaphore>>,
    cancel_token: CancellationToken,
}

impl MonitorEngine {
    pub fn new(store: Arc<Store>, monitor_connections: Arc<MonitorConnectionRegistry>) -> Arc<Self> {
        Arc::new(Self {
            store,
            monitor_connections,
            app_handle: Mutex::new(None),
            paused: AtomicBool::new(false),
            schedule: RwLock::new(Vec::new()),
            last_run: RwLock::new(HashMap::new()),
            config_locks: Mutex::new(HashMap::new()),
            semaphore: RwLock::new(Arc::new(Semaphore::new(2))),
            cancel_token: CancellationToken::new(),
        })
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
                        if engine.paused.load(Ordering::Relaxed) {
                            continue;
                        }
                        engine.poll_due_widgets().await;
                    }
                }
            }
        });
    }

    pub async fn reload_from_store(&self) -> Result<(), crate::dashboard::store::DashboardStoreError> {
        let data_dir = self.store.data_dir();
        let dashboards = list_dashboards(data_dir)?;
        let table = build_schedule_table(&dashboards);

        let mut schedule = Vec::with_capacity(table.len());
        for dashboard in &dashboards {
            if !dashboard.enabled {
                continue;
            }
            for widget in &dashboard.widgets {
                if !widget.enabled {
                    continue;
                }
                schedule.push(ScheduleEntry {
                    dashboard_id: dashboard.id.clone(),
                    widget: widget.clone(),
                    refresh_sec: clamp_refresh_sec(widget.refresh_sec),
                });
            }
        }
        *self.schedule.write().await = schedule;

        let settings = self.store.get_settings().await;
        let max = crate::dashboard::store::load_monitor_settings(&settings)
            .max_concurrent_queries
            .max(1) as usize;
        *self.semaphore.write().await = Arc::new(Semaphore::new(max));

        tracing::debug!(count = table.len(), "monitor schedule reloaded");
        Ok(())
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        tracing::info!(paused, "monitor engine pause state changed");
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
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
        let sem = self.semaphore.read().await.clone();
        let _permit = sem
            .acquire()
            .await
            .map_err(|_| DashboardExecuteError::Driver(crate::db::DriverError::QueryFailed(
                "monitor semaphore closed".into(),
            )))?;

        let config_lock = {
            let mut locks = self.config_locks.lock().expect("config_locks mutex");
            locks
                .entry(widget.config_id.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _config_guard = config_lock.lock().await;

        let settings = self.store.get_settings().await;
        let run = execute_widget_once(
            &self.monitor_connections,
            self.store.data_dir(),
            &settings,
            dashboard_id,
            widget,
        )
        .await?;

        if emit_event {
            self.emit_run_updated(dashboard_id, &widget.id, &run);
        }

        Ok(run)
    }

    fn emit_run_updated(&self, dashboard_id: &str, widget_id: &str, run: &WidgetRun) {
        let handle = self.app_handle.lock().expect("monitor app_handle lock").clone();
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
            let due = {
                let last = self.last_run.read().await;
                match last.get(&key) {
                    None => true,
                    Some(t) => now.duration_since(*t).as_secs() >= entry.refresh_sec as u64,
                }
            };
            if !due {
                continue;
            }

            self.last_run.write().await.insert(key, now);

            let engine = Arc::clone(self);
            let dashboard_id = entry.dashboard_id.clone();
            let widget = entry.widget.clone();
            tokio::spawn(async move {
                if let Err(e) = engine
                    .tick_widget_inner(&dashboard_id, &widget, true)
                    .await
                {
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
        AggregationType, ChartConfig, ChartSortBy, ChartType, DashboardLayout, WidgetLayout,
    };

    fn sample_widget(id: &str, refresh_sec: u32, enabled: bool) -> DashboardWidget {
        DashboardWidget {
            id: id.into(),
            title: "Test".into(),
            config_id: "cfg-1".into(),
            sql: "SELECT 1".into(),
            chart_config: ChartConfig {
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
            },
            layout: WidgetLayout {
                x: 0,
                y: 0,
                w: 4,
                h: 3,
            },
            refresh_sec,
            alert: None,
            enabled,
        }
    }

    fn sample_dashboard(id: &str, enabled: bool, widgets: Vec<DashboardWidget>) -> Dashboard {
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
        }
    }

    #[test]
    fn build_schedule_table_skips_disabled_dashboards_and_widgets() {
        let dashboards = vec![
            sample_dashboard(
                "d1",
                false,
                vec![sample_widget("w1", 60, true)],
            ),
            sample_dashboard(
                "d2",
                true,
                vec![
                    sample_widget("w2", 60, false),
                    sample_widget("w3", 90, true),
                ],
            ),
        ];

        let table = build_schedule_table(&dashboards);
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].dashboard_id, "d2");
        assert_eq!(table[0].widget_id, "w3");
    }

    #[test]
    fn build_schedule_table_clamps_refresh_sec() {
        let dashboards = vec![sample_dashboard(
            "d1",
            true,
            vec![sample_widget("w1", 5, true)],
        )];

        let table = build_schedule_table(&dashboards);
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].refresh_sec, 30);
    }

    #[test]
    fn build_schedule_table_includes_all_enabled_widgets() {
        let dashboards = vec![sample_dashboard(
            "d1",
            true,
            vec![
                sample_widget("w1", 60, true),
                sample_widget("w2", 120, true),
            ],
        )];

        let table = build_schedule_table(&dashboards);
        assert_eq!(table.len(), 2);
        let ids: Vec<_> = table.iter().map(|e| e.widget_id.as_str()).collect();
        assert!(ids.contains(&"w1"));
        assert!(ids.contains(&"w2"));
        assert_eq!(table[0].config_id, "cfg-1");
    }
}
