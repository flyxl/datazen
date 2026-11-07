//! 注入的存储端口的**行为**证据。
//!
//! `lib.rs` 里的几条守卫是**形状**证据：字段在、类型是 `platform-api` 的那一份、
//! 构造参数必填。本模块是**行为**证据：形态中立的用例层**真的能跑用例**，
//! 而且跑的是**注入进来的那一份**存储。
//!
//! 「那一份」由 `Arc::ptr_eq` 比地址判定：如果聚合内部另造了一份默认实现，
//! 或者用例走的是别的路径，地址就不相等，测试立刻红。

use std::sync::{Arc, Mutex};

use crate::application::migration_profile::MigrationProfileReader;
use crate::execution::application::idempotency::{IdempotentOperation, SubmissionToken};
use crate::execution::application::EventSubscription;
use crate::execution::target::{TargetNamespaceShape, TargetRequirements};
use crate::execution::EventSequence;
use crate::execution::{
    CancelReceipt, CloseReceipt, ContextChangeReceipt, OpenSessionReceipt, SessionHandle,
    SessionView,
};
use crate::execution::{ExecutionReceipt, ExecutionView};
use crate::migration::schema_diff::{
    SchemaDiffProfile, SchemaDiffProfileRepository, SchemaDiffProfileRepositoryError,
};
use crate::shared::context::RequestContext;
use crate::shared::error::PortError;
use crate::shared::id::{ConnectionId, ExecutionId, StreamId};
use crate::shared::scope::OrganizationScope;
use crate::tunnel::TunnelRepository;
use crate::tunnel::{KnownHostEntry, KnownHostsRepository};
use datazen_driver_api::{SavedTunnel, SavedTunnelSummary, TunnelUsage};

use crate::application::identity_policy::IdentityPolicy;
use crate::application::sessions::ConnectionUseCases;
use crate::application::target::{TargetMetadataProvider, TargetResolver};
use crate::application::ApplicationServices;
use crate::connection::dto::ProfileView;
use crate::execution::application::requests::{
    AttachmentRequest, CloseSessionRequest, ExecuteAtTargetRequest, ExecuteInSessionRequest,
    OpenSessionRequest, SetSessionContextRequest,
};
use crate::shared::error::{ApiError, ApiErrorCode};

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

/// Schema Diff profile repository: this module only exercises construction of
/// the aggregate, so reaching this port is a test failure rather than a benign
/// empty result.
struct EmptySchemaDiffProfiles;

#[async_trait::async_trait]
impl SchemaDiffProfileRepository for EmptySchemaDiffProfiles {
    async fn list(
        &self,
        _scope: &OrganizationScope,
    ) -> Result<Vec<SchemaDiffProfile>, SchemaDiffProfileRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写 Schema Diff profile")
    }

    async fn save(
        &self,
        _scope: &OrganizationScope,
        _profile: SchemaDiffProfile,
    ) -> Result<(), SchemaDiffProfileRepositoryError> {
        unimplemented!("storage_ports_tests 不应写入 Schema Diff profile")
    }

    async fn delete(
        &self,
        _scope: &OrganizationScope,
        _profile_id: &str,
    ) -> Result<(), SchemaDiffProfileRepositoryError> {
        unimplemented!("storage_ports_tests 不应删除 Schema Diff profile")
    }
}

struct EmptyMigrationProfiles;

#[async_trait::async_trait]
impl MigrationProfileReader for EmptyMigrationProfiles {
    async fn list_transfer_profiles(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<crate::application::migration_profile::MigrationProfileSummary>, PortError>
    {
        unimplemented!("storage_ports_tests 不应读取迁移 profile 投影")
    }

    async fn list_sync_profiles(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<crate::application::migration_profile::MigrationProfileSummary>, PortError>
    {
        unimplemented!("storage_ports_tests 不应读取迁移 profile 投影")
    }

    async fn list_schema_diff_profiles(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<crate::application::migration_profile::MigrationProfileSummary>, PortError>
    {
        unimplemented!("storage_ports_tests 不应读取迁移 profile 投影")
    }
}

/// 组装一个带注入端口的聚合。
///
/// 目标解析器的元数据供给恒为 `None`——本模块不测目标计算。
fn services_with(tunnels: Arc<dyn TunnelRepository>) -> ApplicationServices {
    services_with_known_hosts(tunnels, Arc::new(MemoryKnownHosts::default()))
}

/// 同上，但 known-hosts 端口由调用方给——用来证明「持有的是注入的那一份」。
fn services_with_known_hosts(
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
        tunnels,
        known_hosts,
        Arc::new(EmptyDashboards),
        Arc::new(EmptyDashboardRuns),
        Arc::new(EmptySchemaDiffProfiles),
        Arc::new(EmptyMigrationProfiles),
    )
}

// ── 断言 ───────────────────────────────────────────────────────────────────

/// 应用层**持有**注入的存储端口：地址相等 ⇒ 是同一个实例，不是另造的一份。
#[test]
fn the_aggregate_holds_the_injected_instances_themselves() {
    let tunnels: Arc<dyn TunnelRepository> = Arc::new(EmptyTunnelRepository);
    let services = services_with(tunnels.clone());

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
    let services = services_with_known_hosts(Arc::new(EmptyTunnelRepository), known_hosts.clone());

    assert!(
        Arc::ptr_eq(&services.known_hosts, &known_hosts),
        "聚合必须持有注入的那一个 known-hosts 仓储实例"
    );
}

/// 写路径**先校验再落盘**：非法的枚举取值不得进入存储。
///
/// 这一条是「用例在中间、端口在边上」的证据：校验发生在应用层，落盘发生在
/// 仪表盘仓储的空实现：本文件的断言只碰设置、隧道与 known-hosts。
///
/// 不做「返回空列表」这类温和替身——那样会让别的测试误以为仪表盘可用；
/// 一旦有测试真的走到这里，应当立刻失败并露出「该测试需要自己的仓储」。
struct EmptyDashboards;

impl crate::dashboard::repository::DashboardRepository for EmptyDashboards {
    fn list_dashboard_rows(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
    ) -> Result<
        Vec<crate::dashboard::records::DashboardRecord>,
        crate::dashboard::repository::DashboardRepositoryError,
    > {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn dashboard_row(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _id: &str,
    ) -> Result<
        crate::dashboard::records::DashboardRecord,
        crate::dashboard::repository::DashboardRepositoryError,
    > {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn upsert_dashboard_row(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _row: &crate::dashboard::records::DashboardRecord,
    ) -> Result<(), crate::dashboard::repository::DashboardRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn delete_dashboard_row(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _id: &str,
    ) -> Result<(), crate::dashboard::repository::DashboardRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn set_dashboard_refresh_paused(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _id: &str,
        _paused: bool,
    ) -> Result<(), crate::dashboard::repository::DashboardRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn list_widget_rows(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _dashboard_id: &str,
    ) -> Result<
        Vec<crate::dashboard::records::WidgetRecord>,
        crate::dashboard::repository::DashboardRepositoryError,
    > {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn widget_row(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _id: &str,
    ) -> Result<
        crate::dashboard::records::WidgetRecord,
        crate::dashboard::repository::DashboardRepositoryError,
    > {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn upsert_widget_row(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _row: &crate::dashboard::records::WidgetRecord,
    ) -> Result<(), crate::dashboard::repository::DashboardRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn delete_widget_row(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _id: &str,
    ) -> Result<(), crate::dashboard::repository::DashboardRepositoryError> {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }

    fn find_workflow_refs(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _workflow_id: &str,
    ) -> Result<
        Vec<crate::dashboard::types::DashboardWorkflowRef>,
        crate::dashboard::repository::DashboardRepositoryError,
    > {
        unimplemented!("storage_ports_tests 不应读写仪表盘")
    }
}

/// 运行历史仓储的空实现：理由同 [`EmptyDashboards`]。
struct EmptyDashboardRuns;

impl crate::dashboard::runs::DashboardRunRepository for EmptyDashboardRuns {
    fn write_run_row(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _run: crate::dashboard::records::WidgetRunRecord,
        _retention_count: u32,
        _retention_days: u32,
    ) -> Result<(), crate::dashboard::runs::DashboardRunsError> {
        unimplemented!("storage_ports_tests 不应读写运行历史")
    }

    fn list_run_rows(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _widget_id: &str,
        _limit: u32,
    ) -> Result<
        Vec<crate::dashboard::records::WidgetRunRecord>,
        crate::dashboard::runs::DashboardRunsError,
    > {
        unimplemented!("storage_ports_tests 不应读写运行历史")
    }

    fn run_row(
        &self,
        _scope: &crate::shared::scope::OrganizationScope,
        _run_id: &str,
    ) -> Result<
        crate::dashboard::records::WidgetRunRecord,
        crate::dashboard::runs::DashboardRunsError,
    > {
        unimplemented!("storage_ports_tests 不应读写运行历史")
    }
}
