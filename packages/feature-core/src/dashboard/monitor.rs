//! Dashboard schedule and concurrency lifecycle; native delivery is injected.
use super::schedule_grant::MonitorGrant;
use super::types::{Dashboard, DashboardWidget, RefreshMode, WidgetRun};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
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
pub fn should_spawn_widget_tick(due: bool, in_flight: bool) -> bool {
    due && !in_flight
}

/// Returns true when `refresh_sec` has elapsed since `last_run` (or never run).
pub fn is_widget_refresh_due(last_run: Option<Instant>, now: Instant, refresh_sec: u32) -> bool {
    match last_run {
        None => true,
        Some(t) => now.duration_since(t).as_secs() >= refresh_sec as u64,
    }
}

#[async_trait::async_trait]
pub trait MonitorHost: Send + Sync {
    async fn dashboards(&self) -> Result<Vec<Dashboard>, String>;
    async fn max_concurrent_queries(&self) -> usize;
    async fn execute_widget(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<WidgetRun, String>;
    /// Backend-held authorization only; never use execute_widget as a fallback.
    async fn scheduled_authorization(
        &self,
        _dashboard_id: &str,
        _widget: &DashboardWidget,
    ) -> Result<MonitorGrant, String> {
        Err("monitor scheduled service grant unavailable".into())
    }
    async fn execute_scheduled_widget(
        &self,
        _dashboard_id: &str,
        _widget: &DashboardWidget,
        _grant: &MonitorGrant,
    ) -> Result<WidgetRun, String> {
        Err("monitor scheduled service adapter unavailable".into())
    }
    async fn deliver_run(&self, dashboard_id: &str, widget: &DashboardWidget, run: &WidgetRun);
}
#[derive(Clone)]
struct Entry {
    dashboard_id: String,
    widget: DashboardWidget,
    refresh_sec: u32,
}
pub struct MonitorEngine {
    host: Arc<dyn MonitorHost>,
    schedule: RwLock<Vec<Entry>>,
    active: AtomicUsize,
    semaphore: RwLock<Arc<Semaphore>>,
    workflow_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    last_run: RwLock<HashMap<(String, String), Instant>>,
    in_flight: Mutex<HashSet<(String, String)>>,
    cancel: CancellationToken,
}
struct TickGuard {
    engine: Arc<MonitorEngine>,
    key: (String, String),
}
impl Drop for TickGuard {
    fn drop(&mut self) {
        self.engine
            .in_flight
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.key);
    }
}
impl MonitorEngine {
    pub fn new(host: Arc<dyn MonitorHost>) -> Arc<Self> {
        Arc::new(Self {
            host,
            schedule: RwLock::new(vec![]),
            active: AtomicUsize::new(0),
            semaphore: RwLock::new(Arc::new(Semaphore::new(2))),
            workflow_locks: Mutex::new(HashMap::new()),
            last_run: RwLock::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            cancel: CancellationToken::new(),
        })
    }
    pub fn start(self: &Arc<Self>) {
        let engine = self.clone();
        tokio::spawn(async move {
            if let Err(error) = engine.reload().await {
                tracing::warn!(%error, "monitor initial reload failed");
            }
            loop {
                tokio::select! {
                    () = engine.cancel.cancelled() => break,
                    () = tokio::time::sleep(Duration::from_secs(1)) => engine.poll().await,
                }
            }
        });
    }
    pub fn is_monitoring_active(&self) -> bool {
        self.active.load(Ordering::Relaxed) > 0
    }
    pub async fn reload(&self) -> Result<(), String> {
        let dashboards = self.host.dashboards().await?;
        let mut schedule = vec![];
        for dashboard in dashboards {
            if !dashboard.enabled || dashboard.refresh_paused {
                continue;
            }
            for widget in dashboard.widgets {
                if !widget.enabled || widget.refresh.mode != RefreshMode::Interval {
                    continue;
                }
                if let Some(refresh_sec) = widget.refresh.interval_secs() {
                    schedule.push(Entry {
                        dashboard_id: dashboard.id.clone(),
                        widget,
                        refresh_sec,
                    });
                }
            }
        }
        self.active.store(schedule.len(), Ordering::Relaxed);
        *self.schedule.write().await = schedule;
        *self.semaphore.write().await = Arc::new(Semaphore::new(
            self.host.max_concurrent_queries().await.max(1),
        ));
        Ok(())
    }
    pub async fn tick_widget(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<WidgetRun, String> {
        let sem = self.semaphore.read().await.clone();
        let _permit = sem
            .acquire()
            .await
            .map_err(|_| "Monitor semaphore closed")?;
        let lock = self
            .workflow_locks
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(widget.workflow_id.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        let run = self.host.execute_widget(dashboard_id, widget).await?;
        self.host.deliver_run(dashboard_id, widget, &run).await;
        Ok(run)
    }
    async fn current_scheduled_grant(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<MonitorGrant, String> {
        let dashboards = self.host.dashboards().await?;
        let dashboard = dashboards
            .iter()
            .find(|dashboard| dashboard.id == dashboard_id)
            .ok_or("dashboard missing")?;
        if !dashboard.enabled
            || dashboard.refresh_paused
            || !widget.enabled
            || widget.refresh.mode != RefreshMode::Interval
            || !dashboard.widgets.iter().any(|current| current == widget)
        {
            return Err("monitor schedule disabled or changed".into());
        }
        let grant = self
            .host
            .scheduled_authorization(dashboard_id, widget)
            .await?;
        if grant.dashboard_id != dashboard_id
            || grant.widget_id != widget.id
            || grant.workflow_id != widget.workflow_id
        {
            return Err("monitor grant scope mismatch".into());
        }
        grant
            .validate(&grant.context(), chrono::Utc::now())
            .map_err(str::to_owned)?;
        Ok(grant)
    }
    pub async fn tick_scheduled_widget(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<WidgetRun, String> {
        // Admission freezes approval, workflow revision and explicit target scopes.
        let frozen = self.current_scheduled_grant(dashboard_id, widget).await?;
        let sem = self.semaphore.read().await.clone();
        let _permit = sem
            .acquire()
            .await
            .map_err(|_| "Monitor semaphore closed")?;
        let lock = self
            .workflow_locks
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(widget.workflow_id.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        if self.current_scheduled_grant(dashboard_id, widget).await? != frozen {
            return Err("monitor grant changed before dispatch".into());
        }
        let run = self
            .host
            .execute_scheduled_widget(dashboard_id, widget, &frozen)
            .await?;
        // Do not reinterpret an already dispatched result after revoke/disable.
        self.host.deliver_run(dashboard_id, widget, &run).await;
        Ok(run)
    }
    async fn poll(self: &Arc<Self>) {
        for entry in self.schedule.read().await.clone() {
            let key = (entry.dashboard_id.clone(), entry.widget.id.clone());
            if !is_widget_refresh_due(
                self.last_run.read().await.get(&key).copied(),
                Instant::now(),
                entry.refresh_sec,
            ) {
                continue;
            }
            if !self
                .in_flight
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(key.clone())
            {
                continue;
            }
            let engine = self.clone();
            tokio::spawn(async move {
                let _guard = TickGuard {
                    engine: engine.clone(),
                    key: key.clone(),
                };
                let result = engine
                    .tick_scheduled_widget(&entry.dashboard_id, &entry.widget)
                    .await;
                engine.last_run.write().await.insert(key, Instant::now());
                if let Err(error) = result {
                    tracing::warn!(%error, "monitor widget execution failed");
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
