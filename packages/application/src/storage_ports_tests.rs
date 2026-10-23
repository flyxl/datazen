//! 注入的存储端口的**行为**证据。
//!
//! `lib.rs` 里的几条守卫是**形状**证据：字段在、类型是 `platform-api` 的那一份、
//! 构造参数必填。本模块是**行为**证据：形态中立的用例层**真的能跑用例**，
//! 而且跑的是**注入进来的那一份**存储。
//!
//! 「那一份」由 `Arc::ptr_eq` 比地址判定：如果聚合内部另造了一份默认实现，
//! 或者用例走的是别的路径，地址就不相等，测试立刻红。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use datazen_driver_api::{SavedTunnel, SavedTunnelSummary, TunnelUsage};
use datazen_feature_core::services::named_views::{NamedView, NamedViewError, NamedViewRepository};
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::event::EventSequence;
use datazen_platform_api::dto::execution::{ExecutionReceipt, ExecutionView};
use datazen_platform_api::dto::idempotency::{IdempotentOperation, SubmissionToken};
use datazen_platform_api::dto::session::{
    CancelReceipt, CloseReceipt, ContextChangeReceipt, OpenSessionReceipt, SessionHandle,
    SessionView,
};
use datazen_platform_api::id::{ConnectionId, ExecutionId, StreamId};
use datazen_platform_api::ports::event::EventSubscription;
use datazen_platform_api::ports::known_hosts::{KnownHostEntry, KnownHostsRepository};
use datazen_platform_api::ports::scope::OrganizationScope;
use datazen_platform_api::ports::{SettingsRepository, TunnelRepository};
use datazen_platform_api::settings::{
    ExecutionSettings, PortableThemePreference, SharedSettings, SqlFormatOptions, UserPreferences,
};
use datazen_platform_api::target::{TargetNamespaceShape, TargetRequirements};
use datazen_platform_api::PortError;

use crate::dto::profile::ProfileView;
use crate::dto::requests::{
    AttachmentRequest, CloseSessionRequest, ExecuteAtTargetRequest, ExecuteInSessionRequest,
    OpenSessionRequest, SetSessionContextRequest,
};
use crate::error::{ApiError, ApiErrorCode};
use crate::identity_policy::IdentityPolicy;
use crate::sessions::ConnectionUseCases;
use crate::target::{TargetMetadataProvider, TargetResolver};
use crate::ApplicationServices;

// ── 设置仓储替身 ───────────────────────────────────────────────────────────

/// 记录读写的设置仓储替身：读一次记一次，写一次记一次。
struct RecordingSettingsRepository {
    stored: Mutex<SharedSettings>,
    reads: AtomicUsize,
    writes: AtomicUsize,
}

impl RecordingSettingsRepository {
    fn new(initial: SharedSettings) -> Self {
        Self {
            stored: Mutex::new(initial),
            reads: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
        }
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }

    fn writes(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl SettingsRepository for RecordingSettingsRepository {
    async fn read_settings(&self) -> Result<SharedSettings, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let stored = self
            .stored
            .lock()
            .map_err(|_| PortError::BackendUnavailable("测试仓储锁已中毒".into()))?;
        Ok(stored.clone())
    }

    async fn write_settings(&self, settings: SharedSettings) -> Result<SharedSettings, PortError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        let mut stored = self
            .stored
            .lock()
            .map_err(|_| PortError::BackendUnavailable("测试仓储锁已中毒".into()))?;
        *stored = settings.clone();
        Ok(settings)
    }
}

// ── 隧道仓储替身 ───────────────────────────────────────────────────────────

/// 隧道仓储替身：只提供形状，一律返回「空 / 无」。
struct EmptyTunnelRepository;

#[async_trait::async_trait]
impl TunnelRepository for EmptyTunnelRepository {
    async fn list(&self, _scope: &OrganizationScope) -> Result<Vec<SavedTunnelSummary>, PortError> {
        Ok(Vec::new())
    }

    async fn get(
        &self,
        _scope: &OrganizationScope,
        _tunnel_id: &str,
    ) -> Result<Option<SavedTunnel>, PortError> {
        Ok(None)
    }

    async fn save(
        &self,
        _scope: &OrganizationScope,
        _tunnel: SavedTunnel,
    ) -> Result<(), PortError> {
        Ok(())
    }

    async fn delete(&self, _scope: &OrganizationScope, _tunnel_id: &str) -> Result<(), PortError> {
        Ok(())
    }

    async fn usage(
        &self,
        _scope: &OrganizationScope,
        _tunnel_id: &str,
    ) -> Result<TunnelUsage, PortError> {
        Ok(TunnelUsage {
            connection_ids: Vec::new(),
            connection_names: Vec::new(),
        })
    }
}

// ── 连接用例面替身 ─────────────────────────────────────────────────────────

/// 连接用例面的**测试替身**：只提供形状，一律**失败关闭**。
///
/// 方法体返回 `CapabilityUnsupported` 而不是 `unimplemented!()`：误调用也是
/// 显式失败，不是 panic——本仓库的失败关闭纪律不因「只是测试替身」而破例。
struct UnusedConnectionUseCases;

impl UnusedConnectionUseCases {
    fn unused() -> ApiError {
        ApiError::new(
            ApiErrorCode::CapabilityUnsupported,
            "测试替身：连接用例面不参与本模块的断言",
        )
    }
}

#[async_trait::async_trait]
impl ConnectionUseCases for UnusedConnectionUseCases {
    async fn list_connections(&self, _ctx: &RequestContext) -> Result<Vec<ProfileView>, ApiError> {
        Err(Self::unused())
    }

    async fn issue_submission_token(
        &self,
        _ctx: &RequestContext,
        _operation: IdempotentOperation,
        _handle: Option<&SessionHandle>,
    ) -> Result<SubmissionToken, ApiError> {
        Err(Self::unused())
    }

    async fn open_session(
        &self,
        _ctx: &RequestContext,
        _request: OpenSessionRequest,
    ) -> Result<OpenSessionReceipt, ApiError> {
        Err(Self::unused())
    }

    async fn attach_session(
        &self,
        _ctx: &RequestContext,
        _request: AttachmentRequest,
    ) -> Result<SessionView, ApiError> {
        Err(Self::unused())
    }

    async fn detach_session(
        &self,
        _ctx: &RequestContext,
        _request: AttachmentRequest,
    ) -> Result<SessionView, ApiError> {
        Err(Self::unused())
    }

    async fn get_session(
        &self,
        _ctx: &RequestContext,
        _handle: SessionHandle,
    ) -> Result<SessionView, ApiError> {
        Err(Self::unused())
    }

    async fn execute_in_session(
        &self,
        _ctx: &RequestContext,
        _request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, ApiError> {
        Err(Self::unused())
    }

    async fn execute_at_target(
        &self,
        _ctx: &RequestContext,
        _request: ExecuteAtTargetRequest,
    ) -> Result<ExecutionReceipt, ApiError> {
        Err(Self::unused())
    }

    async fn set_session_context(
        &self,
        _ctx: &RequestContext,
        _request: SetSessionContextRequest,
    ) -> Result<ContextChangeReceipt, ApiError> {
        Err(Self::unused())
    }

    async fn close_session(
        &self,
        _ctx: &RequestContext,
        _request: CloseSessionRequest,
    ) -> Result<CloseReceipt, ApiError> {
        Err(Self::unused())
    }

    async fn get_execution(
        &self,
        _ctx: &RequestContext,
        _execution_id: ExecutionId,
    ) -> Result<ExecutionView, ApiError> {
        Err(Self::unused())
    }

    async fn cancel_execution(
        &self,
        _ctx: &RequestContext,
        _execution_id: ExecutionId,
    ) -> Result<CancelReceipt, ApiError> {
        Err(Self::unused())
    }

    async fn subscribe_events(
        &self,
        _ctx: &RequestContext,
        _stream_id: StreamId,
        _after_sequence: Option<EventSequence>,
    ) -> Result<EventSubscription, ApiError> {
        Err(Self::unused())
    }
}

/// 一份内存 known-hosts：端口的 `load` 不失败，`save` 只在本进程内生效。
///
/// 用例层不消费这个端口（消费者是 SSH 隧道宿主），它在这里的作用只有一个：
/// 证明聚合**持有**注入的那一个实例，而不是内部另造一份。
#[derive(Default)]
struct MemoryKnownHosts {
    entries: Mutex<std::collections::HashMap<String, KnownHostEntry>>,
}

impl KnownHostsRepository for MemoryKnownHosts {
    fn load(&self) -> std::collections::HashMap<String, KnownHostEntry> {
        self.entries
            .lock()
            .map(|entries| entries.clone())
            .unwrap_or_default()
    }

    fn save(
        &self,
        entries: &std::collections::HashMap<String, KnownHostEntry>,
    ) -> Result<(), PortError> {
        let mut guard = self
            .entries
            .lock()
            .map_err(|_| PortError::BackendUnavailable("known hosts lock poisoned".into()))?;
        *guard = entries.clone();
        Ok(())
    }
}

// ── 夹具 ───────────────────────────────────────────────────────────────────

/// 一份**合法**的跨形态设置：枚举取值必须过 `SettingsService::normalize`。
fn sample_settings() -> SharedSettings {
    SharedSettings {
        preferences: UserPreferences {
            theme: PortableThemePreference {
                mode: "dark".into(),
                pack_id: None,
            },
            language: "zh-CN".into(),
            editor_font_size: 14,
            editor_font_family: "monospace".into(),
            confirm_on_delete: true,
            auto_commit: false,
            confirm_dangerous_execution: true,
            default_page_size: 100,
            auto_chart_on_query: false,
            wapp_settings: serde_json::Map::new(),
            editor_completion_quote_policy: "unquoted".into(),
            editor_completion_include_table_prefix: false,
            enable_fk_prediction: true,
            keymap_preset: "default".into(),
            custom_keymap: std::collections::HashMap::new(),
            sql_execution_strategy: "current_statement".into(),
            sql_syntax_theme: None,
            sql_format_options: SqlFormatOptions::default(),
            sql_snippets: Vec::new(),
            workflow_step_result_order: "asc".into(),
        },
        execution: ExecutionSettings {
            limit_select_results: true,
            query_result_limit: 1000,
            safe_mode: false,
            connection_pool_size: 10,
            mcp_disabled_tools: Vec::new(),
            mcp_permission_mode: "safe_write".into(),
            mcp_allowed_connection_ids: Vec::new(),
            driver_settings: serde_json::Map::new(),
            ai_strict_egress: true,
        },
    }
}

/// 空集合的命名视图存储。
///
/// 本模块只证明「聚合持有的是注入的那一份」，所以这个 fake 只需要存在，
/// 行为在 `feature-core` 的用例测试里覆盖。
#[derive(Default)]
struct EmptyNamedViews;

#[async_trait::async_trait]
impl NamedViewRepository for EmptyNamedViews {
    async fn load_views(&self) -> Result<Vec<NamedView>, NamedViewError> {
        Ok(Vec::new())
    }

    async fn save_views(&self, _views: &[NamedView]) -> Result<(), NamedViewError> {
        Ok(())
    }
}

/// 组装一个带注入端口的聚合。
///
/// 目标解析器的元数据供给恒为 `None`——本模块不测目标计算。
fn services_with(
    settings: Arc<dyn SettingsRepository>,
    tunnels: Arc<dyn TunnelRepository>,
) -> ApplicationServices {
    services_with_known_hosts(settings, tunnels, Arc::new(MemoryKnownHosts::default()))
}

/// 同上，但 known-hosts 端口由调用方给——用来证明「持有的是注入的那一份」。
fn services_with_known_hosts(
    settings: Arc<dyn SettingsRepository>,
    tunnels: Arc<dyn TunnelRepository>,
    known_hosts: Arc<dyn KnownHostsRepository>,
) -> ApplicationServices {
    let metadata: TargetMetadataProvider = Arc::new(
        |_connection: &ConnectionId,
         _command: &str|
         -> Option<(TargetNamespaceShape, Option<TargetRequirements>)> { None },
    );
    ApplicationServices::new(
        TargetResolver::new(metadata),
        IdentityPolicy::new(),
        Arc::new(UnusedConnectionUseCases),
        settings,
        tunnels,
        known_hosts,
        Arc::new(EmptyNamedViews),
        Arc::new(EmptyDashboards),
        Arc::new(EmptyDashboardRuns),
    )
}

// ── 断言 ───────────────────────────────────────────────────────────────────

/// 应用层**持有**注入的存储端口：地址相等 ⇒ 是同一个实例，不是另造的一份。
#[test]
fn the_aggregate_holds_the_injected_instances_themselves() {
    let repository = Arc::new(RecordingSettingsRepository::new(sample_settings()));
    let settings: Arc<dyn SettingsRepository> = repository.clone();
    let tunnels: Arc<dyn TunnelRepository> = Arc::new(EmptyTunnelRepository);
    let services = services_with(settings.clone(), tunnels.clone());

    assert!(
        Arc::ptr_eq(&services.settings, &settings),
        "聚合必须持有注入的那一个设置仓储实例"
    );
    assert!(
        Arc::ptr_eq(&services.tunnels, &tunnels),
        "聚合必须持有注入的那一个隧道仓储实例"
    );
}

/// 第六个端口（known-hosts）同样**必填且被持有**。
///
/// 这条在 `lib.rs` 的形状守卫之外补行为证据：`ApplicationServices::new` 的参数
/// 顺序里它是最后一个，漏传是编译错误；这里证明传进来的那一份就是字段里的那一份。
#[test]
fn the_aggregate_holds_the_injected_known_hosts_port() {
    let known_hosts: Arc<dyn KnownHostsRepository> = Arc::new(MemoryKnownHosts::default());
    let services = services_with_known_hosts(
        Arc::new(RecordingSettingsRepository::new(sample_settings())),
        Arc::new(EmptyTunnelRepository),
        known_hosts.clone(),
    );

    assert!(
        Arc::ptr_eq(&services.known_hosts, &known_hosts),
        "聚合必须持有注入的那一个 known-hosts 仓储实例"
    );
}

/// 形态中立的设置用例**跑在注入的端口上**：读到的是注入实例里的事实。
#[tokio::test]
async fn the_settings_use_case_runs_on_the_injected_port() {
    let repository = Arc::new(RecordingSettingsRepository::new(sample_settings()));
    let settings: Arc<dyn SettingsRepository> = repository.clone();
    let tunnels: Arc<dyn TunnelRepository> = Arc::new(EmptyTunnelRepository);
    let services = services_with(settings, tunnels);

    let read = services
        .settings_service()
        .read()
        .await
        .expect("设置用例必须能在没有 Tauri 的进程里跑");
    assert_eq!(read, sample_settings());
    assert_eq!(repository.reads(), 1, "读必须落到注入的端口上");
    assert_eq!(repository.writes(), 0);

    let mut written = sample_settings();
    written.preferences.language = "en-US".into();
    let saved = services
        .settings_service()
        .save(written.clone())
        .await
        .expect("写设置必须成功");
    assert_eq!(saved, written);
    assert_eq!(repository.writes(), 1, "写必须落到注入的端口上");
}

/// 写路径**先校验再落盘**：非法的枚举取值不得进入存储。
///
/// 这一条是「用例在中间、端口在边上」的证据：校验发生在应用层，落盘发生在
/// 注入的端口上，两者不是同一件事。
#[tokio::test]
async fn the_settings_use_case_validates_before_it_writes_to_the_port() {
    let repository = Arc::new(RecordingSettingsRepository::new(sample_settings()));
    let settings: Arc<dyn SettingsRepository> = repository.clone();
    let tunnels: Arc<dyn TunnelRepository> = Arc::new(EmptyTunnelRepository);
    let services = services_with(settings, tunnels);

    let mut invalid = sample_settings();
    invalid.preferences.theme.mode = "neon".into();
    let error = services
        .settings_service()
        .save(invalid)
        .await
        .expect_err("非法主题必须被用例层拒绝");
    assert!(
        matches!(error, crate::settings::SettingsError::Validation(_)),
        "非法取值必须以校验错误失败：{error:?}"
    );
    assert_eq!(repository.writes(), 0, "校验失败不得落盘");
}

/// 仪表盘仓储的空实现：本文件的断言只碰设置、隧道与 known-hosts。
///
/// 不做「返回空列表」这类温和替身——那样会让别的测试误以为仪表盘可用；
/// 一旦有测试真的走到这里，应当立刻失败并露出「该测试需要自己的仓储」。
struct EmptyDashboards;

impl datazen_feature_core::dashboard::repository::DashboardRepository for EmptyDashboards {
    fn list_dashboard_rows(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
    ) -> Result<
        Vec<datazen_feature_core::dashboard::records::DashboardRecord>,
        datazen_feature_core::dashboard::repository::DashboardRepositoryError,
    > {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn dashboard_row(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _id: &str,
    ) -> Result<
        datazen_feature_core::dashboard::records::DashboardRecord,
        datazen_feature_core::dashboard::repository::DashboardRepositoryError,
    > {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn upsert_dashboard_row(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _row: &datazen_feature_core::dashboard::records::DashboardRecord,
    ) -> Result<(), datazen_feature_core::dashboard::repository::DashboardRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn delete_dashboard_row(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _id: &str,
    ) -> Result<(), datazen_feature_core::dashboard::repository::DashboardRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn set_dashboard_refresh_paused(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _id: &str,
        _paused: bool,
    ) -> Result<(), datazen_feature_core::dashboard::repository::DashboardRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn list_widget_rows(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _dashboard_id: &str,
    ) -> Result<
        Vec<datazen_feature_core::dashboard::records::WidgetRecord>,
        datazen_feature_core::dashboard::repository::DashboardRepositoryError,
    > {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn widget_row(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _id: &str,
    ) -> Result<
        datazen_feature_core::dashboard::records::WidgetRecord,
        datazen_feature_core::dashboard::repository::DashboardRepositoryError,
    > {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn upsert_widget_row(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _row: &datazen_feature_core::dashboard::records::WidgetRecord,
    ) -> Result<(), datazen_feature_core::dashboard::repository::DashboardRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn delete_widget_row(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _id: &str,
    ) -> Result<(), datazen_feature_core::dashboard::repository::DashboardRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn find_workflow_refs(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _workflow_id: &str,
    ) -> Result<
        Vec<datazen_feature_core::dashboard::types::DashboardWorkflowRef>,
        datazen_feature_core::dashboard::repository::DashboardRepositoryError,
    > {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }
}

/// 运行历史仓储的空实现：理由同 [`EmptyDashboards`]。
struct EmptyDashboardRuns;

impl datazen_feature_core::dashboard::runs::DashboardRunRepository for EmptyDashboardRuns {
    fn write_run_row(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _run: datazen_feature_core::dashboard::records::WidgetRunRecord,
        _retention_count: u32,
        _retention_days: u32,
    ) -> Result<(), datazen_feature_core::dashboard::runs::DashboardRunsError> {
        unimplemented!("storage_ports_tests 不应读写运行历史")
    }

    fn list_run_rows(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _widget_id: &str,
        _limit: u32,
    ) -> Result<
        Vec<datazen_feature_core::dashboard::records::WidgetRunRecord>,
        datazen_feature_core::dashboard::runs::DashboardRunsError,
    > {
        unimplemented!("storage_ports_tests 不应读写运行历史")
    }

    fn run_row(
        &self,
        _scope: &datazen_platform_api::ports::scope::OrganizationScope,
        _run_id: &str,
    ) -> Result<
        datazen_feature_core::dashboard::records::WidgetRunRecord,
        datazen_feature_core::dashboard::runs::DashboardRunsError,
    > {
        unimplemented!("storage_ports_tests 不应读写运行历史")
    }
}
