//! Desktop adapter for the shared monitor scheduler.
use crate::dashboard::{
    execute::{execute_widget_once, DashboardExecuteError},
    store::{
        list_dashboards, load_monitor_settings, set_dashboard_refresh_paused, DashboardStoreError,
    },
    types::{Dashboard, DashboardWidget, WidgetRun},
};
use crate::monitor::channels::{dispatch_notifications, AlertChannelState};
use crate::{commands::AppState, store::Store};
use datazen_feature_core::dashboard::monitor::{MonitorEngine as CoreEngine, MonitorHost};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
struct DesktopMonitorHost {
    store: Arc<Store>,
    state: Mutex<Option<Arc<AppState>>>,
    app: Mutex<Option<AppHandle>>,
    channels: tokio::sync::Mutex<AlertChannelState>,
}
pub struct MonitorEngine {
    core: Arc<CoreEngine>,
    host: Arc<DesktopMonitorHost>,
}
impl MonitorEngine {
    pub fn new(store: Arc<Store>) -> Arc<Self> {
        let host = Arc::new(DesktopMonitorHost {
            store,
            state: Mutex::new(None),
            app: Mutex::new(None),
            channels: tokio::sync::Mutex::new(AlertChannelState::new()),
        });
        Arc::new(Self {
            core: CoreEngine::new(host.clone()),
            host,
        })
    }
    pub fn attach_app_state(self: &Arc<Self>, state: Arc<AppState>) {
        *self
            .host
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(state);
    }
    pub fn start(self: &Arc<Self>, app: AppHandle) {
        *self
            .host
            .app
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(app);
        start_core_on_desktop_runtime(&self.core);
    }
    pub fn app_handle(&self) -> Option<AppHandle> {
        self.host
            .app
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
    pub fn is_monitoring_active(&self) -> bool {
        self.core.is_monitoring_active()
    }
    pub async fn reload_from_store(&self) -> Result<(), DashboardStoreError> {
        self.core
            .reload()
            .await
            .map_err(DashboardStoreError::Parse)?;
        if let Some(app) = self.app_handle() {
            crate::tray::sync_tray_async(&app).await;
        }
        Ok(())
    }
    pub fn is_paused(&self) -> bool {
        list_dashboards(&self.host.store.app_db())
            .map(|dashboards| {
                !dashboards.is_empty()
                    && dashboards.iter().all(|dashboard| dashboard.refresh_paused)
            })
            .unwrap_or(false)
    }
    pub fn set_paused(&self, paused: bool) {
        let db = self.host.store.app_db();
        if let Ok(dashboards) = list_dashboards(&db) {
            for dashboard in dashboards {
                if let Err(error) = set_dashboard_refresh_paused(&db, &dashboard.id, paused) {
                    tracing::warn!(%error, "Unable to update dashboard pause");
                }
            }
        }
        let core = self.core.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = core.reload().await {
                tracing::warn!(%error, "Unable to reload monitor");
            }
        });
        if let Some(app) = self.app_handle() {
            crate::tray::sync_tray(&app);
        }
    }
    #[cfg(feature = "webdriver")]
    pub(crate) async fn webdriver_scheduled_tick(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<WidgetRun, String> {
        self.core.tick_scheduled_widget(dashboard_id, widget).await
    }
    pub async fn tick_widget(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<WidgetRun, DashboardExecuteError> {
        self.core
            .tick_widget(dashboard_id, widget)
            .await
            .map_err(DashboardExecuteError::Workflow)
    }
}
// Tauri setup/tray callbacks are synchronous and need not have an ambient Tokio
// context. Enter the desktop-owned runtime only while core.start registers its
// worker; the shared scheduler remains independent of Tauri.
fn start_core_on_desktop_runtime(core: &Arc<CoreEngine>) {
    let runtime = tauri::async_runtime::handle();
    let _entered = runtime.inner().enter();
    core.start();
}

#[async_trait::async_trait]
impl MonitorHost for DesktopMonitorHost {
    async fn dashboards(&self) -> Result<Vec<Dashboard>, String> {
        list_dashboards(&self.store.app_db()).map_err(|error| error.to_string())
    }
    async fn max_concurrent_queries(&self) -> usize {
        load_monitor_settings(&self.store.get_settings().await).max_concurrent_queries as usize
    }
    async fn execute_widget(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<WidgetRun, String> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .ok_or("Monitor app state unavailable")?;
        execute_widget_once(
            &state,
            &self.store.app_db(),
            &self.store.get_settings().await,
            dashboard_id,
            widget,
        )
        .await
        .map_err(|error| error.to_string())
    }
    async fn scheduled_authorization(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<datazen_feature_core::dashboard::schedule_grant::MonitorGrant, String> {
        #[cfg(feature = "webdriver")]
        super::evidence::note(
            dashboard_id,
            &widget.id,
            super::evidence::Stage::GrantChecking,
        );
        let grant = self
            .store
            .app_db()
            .widget_monitor_grant(dashboard_id, &widget.id)
            .map_err(|_| "monitor grant lookup failed")?
            .ok_or("explicit schedule authorization required")?;
        let state = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .ok_or("Monitor app state unavailable")?;
        if let Err(error) = super::scheduled::validate(&state, &grant).await {
            #[cfg(feature = "webdriver")]
            super::evidence::note(
                dashboard_id,
                &widget.id,
                super::evidence::Stage::GrantRejected,
            );
            return Err(error);
        }
        #[cfg(feature = "webdriver")]
        super::evidence::note(
            dashboard_id,
            &widget.id,
            super::evidence::Stage::GrantValidated,
        );
        Ok(grant)
    }
    async fn execute_scheduled_widget(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
        grant: &datazen_feature_core::dashboard::schedule_grant::MonitorGrant,
    ) -> Result<WidgetRun, String> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .ok_or("Monitor app state unavailable")?;
        super::scheduled::execute(&state, dashboard_id, widget, grant).await
    }
    async fn deliver_run(&self, dashboard_id: &str, widget: &DashboardWidget, run: &WidgetRun) {
        let app = self
            .app
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let name = list_dashboards(&self.store.app_db())
            .ok()
            .and_then(|dashboards| {
                dashboards
                    .into_iter()
                    .find(|dashboard| dashboard.id == dashboard_id)
            })
            .map(|dashboard| dashboard.name)
            .unwrap_or_else(|| dashboard_id.into());
        let (notifications, client) = {
            let mut channels = self.channels.lock().await;
            (
                channels.process_run_state(dashboard_id, &name, widget, run),
                channels.http_client().clone(),
            )
        };
        dispatch_notifications(
            app.as_ref(),
            &load_monitor_settings(&self.store.get_settings().await),
            &client,
            &notifications,
        )
        .await;
        if let Some(app) = app {
            let _ = app.emit("dashboard:run-updated", serde_json::json!({ "dashboardId": dashboard_id, "widgetId": widget.id, "run": run }));
        }
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    struct StartupHost(mpsc::Sender<()>);
    #[async_trait::async_trait]
    impl MonitorHost for StartupHost {
        async fn dashboards(&self) -> Result<Vec<Dashboard>, String> {
            self.0.send(()).map_err(|error| error.to_string())?;
            Ok(Vec::new())
        }
        async fn max_concurrent_queries(&self) -> usize {
            1
        }
        async fn execute_widget(&self, _: &str, _: &DashboardWidget) -> Result<WidgetRun, String> {
            Err("empty startup schedule must not execute widgets".into())
        }
        async fn deliver_run(&self, _: &str, _: &DashboardWidget, _: &WidgetRun) {}
    }

    #[test]
    fn sync_setup_starts_monitor_without_ambient_tokio_context() {
        // A fresh synchronous thread reproduces Tauri setup after its preceding
        // block_on has returned. A #[tokio::test] would mask this regression.
        std::thread::spawn(|| {
            assert!(tokio::runtime::Handle::try_current().is_err());
            let (send, received) = mpsc::channel();
            let core = CoreEngine::new(Arc::new(StartupHost(send)));
            start_core_on_desktop_runtime(&core);
            assert!(tokio::runtime::Handle::try_current().is_err());
            received
                .recv_timeout(Duration::from_secs(3))
                .expect("monitor initial reload ran on the desktop runtime");
            assert!(!core.is_monitoring_active());
        })
        .join()
        .expect("sync setup must not panic");
    }
}

#[cfg(test)]
#[path = "scheduled_timer_tests.rs"]
mod scheduled_timer_tests;
