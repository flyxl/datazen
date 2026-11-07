//! Tauri IPC command surface.

pub mod ai;
mod app_archive;
mod backup;
mod clipboard;
mod config;
pub(crate) mod connection;
mod connection_import;
mod context;
mod dashboard;
mod data;
mod data_transfer;
pub(crate) mod dialog;
pub(crate) mod driver_command;
mod encryption_key;
mod error;
mod export;
mod file;
pub(crate) mod file_jobs;
mod fk_navigation;
mod history;
mod language_packs;
pub mod mcp;
mod named_views;
mod port_call;
mod query;
pub(crate) mod query_favorites;
mod restore_gateway;
mod restore_job;
mod result_origin;
mod sample;
mod schema;
pub mod schema_diff;
mod sync;
mod theme;
mod tunnel;
mod value_search;
mod wapps;
pub(crate) mod window;
mod workflow;

pub use ai::*;
pub use app_archive::*;
pub use backup::*;
pub use clipboard::*;
pub use config::*;
pub use connection::*;
pub use connection_import::ipc::*;
pub use context::*;
pub use dashboard::*;
pub use data::*;
pub use data_transfer::*;
pub use encryption_key::*;
pub use language_packs::*;
// The dialog gateway hosts product commands (F4c picker merge) alongside the
// webdriver-only dialog injection commands. The glob must be UNCONDITIONAL:
// `generate_handler!` resolves the macro-generated `__cmd__` companions
// through this path, and a single-item `pub use dialog::pick_import_file`
// re-exports the fn but NOT its `__cmd__` sibling. The webdriver commands
// carry their own `#[cfg]` and vanish from non-webdriver builds anyway.
pub use dialog::*;
pub use driver_command::*;
pub(crate) use error::{CmdExt, CommandError};
pub use export::*;
pub use file::*;
pub use fk_navigation::*;
pub use history::*;
pub use mcp::*;
pub use named_views::*;
pub use query::*;
pub use query_favorites::*;
pub use result_origin::*;
pub use sample::*;
pub use schema::*;
pub use schema_diff::*;
pub use sync::*;
pub use theme::*;
pub use tunnel::*;
pub use value_search::*;
pub use wapps::*;
pub use window::*;
pub use workflow::*;

use crate::mcp::McpClientManager;
use crate::monitor::engine::MonitorEngine;
use crate::store::DesktopRepository;
use crate::transfer::adapter_registry::SyncAdapterRegistry;
use crate::workflow::history::WorkflowHistoryManager;
use crate::workflow::registry::WorkflowRegistry;
use crate::workflow::scheduler::WorkflowScheduler;
use datazen::ai::context::SchemaContextBuilder;
use datazen::ai::prompt_resolver::PromptResolver;
use datazen::ai::AiProviderRegistry;
use datazen::execution::runtime::driver_registry::registry::DriverRegistry;
use datazen::schema::cache::SchemaCache;
use datazen::shared::error::ApiError;
use datazen::shared::id::OrganizationId;
use datazen::shared::scope::OrganizationScope;
use datazen::workspace_apps::manager::WappManager;
use datazen_driver_api::QueryExecutionId;
use std::collections::{hash_map::Entry, HashMap};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Host-side ownership registry for live query execution handles.
///
/// The driver owns the backend target registry; this map only binds an opaque
/// execution token to its `dbSessionId` so IPC cannot cancel another session's
/// query. Entries are removed on every stream terminal path.
pub struct QueryExecutionRegistry {
    owners: RwLock<HashMap<QueryExecutionId, String>>,
}

impl QueryExecutionRegistry {
    pub fn new() -> Self {
        Self {
            owners: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(
        &self,
        execution_id: QueryExecutionId,
        db_session_id: impl Into<String>,
    ) -> Result<(), String> {
        let mut owners = self.owners.write().await;
        match owners.entry(execution_id.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(db_session_id.into());
                Ok(())
            }
            Entry::Occupied(_) => Err(format!(
                "query execution id '{}' is already registered",
                execution_id.as_str()
            )),
        }
    }

    pub async fn validate_owner(
        &self,
        execution_id: &QueryExecutionId,
        db_session_id: &str,
    ) -> Result<(), String> {
        let owners = self.owners.read().await;
        match owners.get(execution_id) {
            Some(owner) if owner == db_session_id => Ok(()),
            Some(_) => Err(format!(
                "query execution '{}' belongs to a different db session",
                execution_id.as_str()
            )),
            None => Err(format!(
                "query execution '{}' is unknown or stale",
                execution_id.as_str()
            )),
        }
    }

    pub async fn remove(&self, execution_id: &QueryExecutionId) {
        self.owners.write().await.remove(execution_id);
    }
}

impl Default for QueryExecutionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Data Sync 的存储端口实例（组装期创建一次，之后只读借用）。
///
/// 端口的实现留在共享包里：`datazen_data_sync` 仍然拥有「比较落盘」「计划
/// 认领」的规则，宿主只决定**实例从哪来**——比较存储的根目录与计划注册表本身。
/// 端口方法都带 [`OrganizationScope`]，跨组织访问按「不存在」处理。
#[derive(Clone)]
pub struct SyncRepositoryPorts {
    /// 计划注册表端口。签发 / 查看 / 绑定端点身份 / 认领。
    pub plan_store: Arc<dyn datazen::migration::data_sync::plan_store::SyncPlanRepository>,
    /// 比较存储根端口。决定比较结果写到哪个目录。
    pub comparison_root:
        Arc<dyn datazen::migration::data_sync::comparison_store::ComparisonStorageRoot>,
}

impl SyncRepositoryPorts {
    /// 桌面形态的默认接线：一个本地根 + 一个进程内计划注册表。
    ///
    /// 桌面是单租户：比较存储的根只属于本地组织这一个组织。
    pub fn desktop() -> Self {
        let organization = OrganizationId::new(crate::platform::identity::LOCAL_ORGANIZATION_ID);
        Self {
            plan_store: Arc::new(
                datazen::migration::data_sync::plan_store::InMemorySyncPlanRepository::new(),
            ),
            comparison_root: Arc::new(
                datazen::migration::data_sync::comparison_store::LocalComparisonStorageRoot::new(
                    datazen::migration::data_sync::comparison_store::default_comparison_storage_root(),
                    organization,
                ),
            ),
        }
    }
}

/// Shared application state injected into every command handler.
#[derive(Clone)]
pub struct AppState {
    pub driver_registry: Arc<DriverRegistry>,
    pub monitor_engine: Arc<MonitorEngine>,
    pub store: Arc<DesktopRepository>,
    pub schema_cache: Arc<SchemaCache>,
    pub sync_adapters: Arc<SyncAdapterRegistry>,
    pub ai_registry: Arc<AiProviderRegistry>,
    pub schema_context_builder: Arc<SchemaContextBuilder>,
    pub prompt_resolver: Arc<PromptResolver>,
    pub workflow_registry: Arc<WorkflowRegistry>,
    pub workflow_history: Arc<WorkflowHistoryManager>,
    pub mcp_client_manager: Arc<McpClientManager>,
    pub query_executions: Arc<QueryExecutionRegistry>,
    pub workflow_scheduler: Arc<WorkflowScheduler>,
    pub wapps: Arc<WappManager>,
    /// 已装语言包注册表（共享层 `datazen::language_packs`；存储根
    /// `{appData}/language-packs`，组装期扫描一次）。
    pub language_packs: Arc<datazen::language_packs::LanguagePackManager>,
    pub cancel_registry: datazen::ai::cancel::CancellationRegistry,
    /// Owner-bound editor-session grants for AI tool execution (P6 §14.4).
    pub ai_grants: Arc<crate::commands::ai::AiGrantService>,
    /// P5 Wave-1：Schema Diff 的 JobRuntime 基础设施（仓储/预算/计划库）。
    pub schema_diff_jobs: Arc<crate::commands::schema_diff::job::SchemaDiffJobInfra>,
    /// P5 本机持久化 Job facade，所有迁移 Host 共用 `{appData}/datazen.sqlite`。
    pub desktop_job_host: Arc<datazen::jobs::JobHost>,
    /// Schema Diff 评审计划的存储实例（宿主在组装期创建并注入）。
    ///
    /// 评审计划**不**再寄居在包里的进程级全局变量上：实例由宿主持有，访问按
    /// 组织作用域隔离——多租户宿主里 A 组织的计划不会被 B 组织读到或顶掉。
    /// 桌面是单租户，实现见 `commands/schema_diff/reviewed_repository.rs`。
    pub schema_diff_reviewed_plans: Arc<dyn crate::schema_diff::reviewed::ReviewedPlanRepository>,
    /// Data Sync 的存储端口实例：计划注册表 + 比较存储根。
    ///
    /// 过去计划注册表是共享包里的 `static STORE`（进程级全局可变状态），
    /// 现在由组装点创建实例并注入。
    pub sync_stores: SyncRepositoryPorts,
    /// Data Transfer 的不可变计划与恢复检查点存储端口。
    ///
    /// 端口契约与 HashMap 实现都在 `datazen-data-transfer` 包内；宿主只负责在
    /// 组装期创建**一个**实例并注入，调用点按组织作用域访问它。这样存储的
    /// 生命周期由宿主决定，而不是包内的一个进程级全局。
    pub transfer_plans: Arc<dyn crate::data_transfer::plan_store::TransferPlanRepositoryPort>,
    /// 概要 §6.2 第 8 步的窄适配根：桌面身份 + owner 作用域句柄登记表。
    ///
    /// 事务归属和资源生命周期由 Runtime session actor 管理。取不到 adapter 时用
    /// `AppState::platform_require()`，它返回明确错误而不是 `Option` 兜底。
    pub platform: crate::platform::PlatformEntry,
    /// 桌面形态的**组织作用域**：**启动期确定一次**，请求期直接取。
    ///
    /// 桌面是单租户：一份 app data 就是一个组织。组织的值是**安装级常量**
    /// （`crate::platform::LOCAL_ORGANIZATION_ID`），与「当前 OS 用户名能不能取到」
    /// **无关**——后者只决定 `principalId`。因此组织作用域**不依赖** platform
    /// adapter 是否组装成功：身份不可用时，仪表板 / workflow 命令依然可用。
    ///
    /// 作用域只在这里确定一次，请求体里的任何字段都覆盖不了它（INV-01）。
    pub organization_scope: OrganizationScope,
    /// The Tauri handle used by the AI tool-confirmation dialog. Injected by
    /// `bootstrap::run` after assembly and before `app.manage`; headless or
    /// test states leave it empty, where confirmation fails closed.
    pub(crate) app_handle: Arc<std::sync::OnceLock<tauri::AppHandle>>,
}

impl AppState {
    /// Resolve the target connection's SQL literal dialect for shared SQL-safe
    /// template rendering. Unknown connections fail safe to the standard.
    pub(crate) async fn connection_sql_dialect(
        &self,
        connection_id: &str,
    ) -> datazen::sql_template::SqlTemplateDialect {
        let Some(config) = self.store.get_connection(connection_id).await else {
            return datazen::sql_template::SqlTemplateDialect::default();
        };
        let Some(driver) = self.driver_registry.get(&config.database_type).await else {
            return datazen::sql_template::SqlTemplateDialect::default();
        };
        driver
            .sql_literal_dialect()
            .map(datazen::sql_template::SqlTemplateDialect::from)
            .unwrap_or_default()
    }

    /// 取 platform adapter；组装失败 ⇒ 返回原始 `ApiError`，不做任何回落。
    ///
    /// 命令侧的统一入口：新接线的能力一律从这里进，避免每个命令各写一遍
    /// `is_some()` 判断把失败吞成默认行为。
    pub fn platform_require(&self) -> Result<&Arc<crate::platform::PlatformAdapter>, ApiError> {
        self.platform.require()
    }

    /// 本次 IPC 调用的**组织作用域**。
    ///
    /// 作用域在**启动期**确定一次并存进 [`AppState::organization_scope`]，请求期
    /// 直接取：命令参数里没有任何字段能覆盖组织身份（INV-01）。
    ///
    /// 桌面形态下这个值永远是桌面自己那一个组织（安装级常量），因此本方法
    /// **不依赖** platform adapter 是否组装成功——「取不到当前 OS 用户名」只影响
    /// `principalId`，不影响组织边界；仪表板 / workflow 命令不会因此整体不可用。
    ///
    /// 保留 `Result` 是因为「作用域从哪来」是宿主形态的自由度：多租户宿主可能
    /// 按请求解析组织，那一步是可以失败的。桌面形态不会失败。
    pub fn organization_scope(&self) -> Result<OrganizationScope, CommandError> {
        Ok(self.organization_scope.clone())
    }

    /// Attach the production Tauri handle after assembly. Calling this more
    /// than once keeps the first handle; tests never need it.
    pub(crate) fn set_app_handle(&self, handle: tauri::AppHandle) {
        let _ = self.app_handle.set(handle);
    }

    /// 模块连接目录的作用域（D2）：连接用例归
    /// `datazen::connection`，作用域是模块自己的词汇。
    pub fn connection_scope(&self) -> Result<datazen::connection::ConnectionScope, CommandError> {
        Ok(datazen::connection::ConnectionScope::new(
            self.organization_scope.organization_id().as_str(),
        ))
    }

    /// Lazily register AI providers + load prompt templates.
    /// Does not run during GUI startup — only on first AI / prompt use.
    ///
    /// 编排体（注册 → 首次注册时恢复持久化配置 → 按语言装载模板）在
    /// `datazen::ai::api::ensure_ai_ready`；这里只把装配好的 registry、
    /// 配置端口与提示词解析器递过去（host-boundary §3 的接线形态）。
    pub async fn ensure_ai_ready(&self) {
        datazen::ai::api::ensure_ai_ready(
            &self.ai_registry,
            self.store.as_ref(),
            &self.prompt_resolver,
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn query_execution_registry_rejects_duplicate_wrong_and_stale_handles() {
        let registry = QueryExecutionRegistry::new();
        let first = QueryExecutionId::new("exec-a");
        let second = QueryExecutionId::new("exec-b");

        registry.register(first.clone(), "session-a").await.unwrap();
        registry
            .register(second.clone(), "session-b")
            .await
            .unwrap();

        let duplicate = registry.register(first.clone(), "session-b").await;
        assert!(duplicate.unwrap_err().contains("already registered"));
        assert!(registry.validate_owner(&first, "session-a").await.is_ok());
        assert!(registry
            .validate_owner(&first, "session-b")
            .await
            .unwrap_err()
            .contains("different db session"));
        assert!(registry
            .validate_owner(&QueryExecutionId::new("unknown"), "session-a")
            .await
            .unwrap_err()
            .contains("unknown or stale"));

        registry.remove(&first).await;
        assert!(registry
            .validate_owner(&first, "session-a")
            .await
            .unwrap_err()
            .contains("unknown or stale"));
    }

    #[tokio::test]
    async fn concurrent_execution_ids_are_isolated_by_session() {
        let registry = QueryExecutionRegistry::new();
        let first = QueryExecutionId::new("exec-a");
        let second = QueryExecutionId::new("exec-b");
        registry.register(first.clone(), "session-a").await.unwrap();
        registry
            .register(second.clone(), "session-b")
            .await
            .unwrap();

        let (first_ok, second_ok, cross_cancel) = tokio::join!(
            registry.validate_owner(&first, "session-a"),
            registry.validate_owner(&second, "session-b"),
            registry.validate_owner(&first, "session-b"),
        );
        assert!(first_ok.is_ok());
        assert!(second_ok.is_ok());
        assert!(cross_cancel.is_err());
    }
}

/// Host IPC test helpers for `src-tauri/tests/` pure-IPC E2E migrations.
#[cfg(feature = "test-harness")]
pub mod test_harness {
    //! B2 各域刀之后命令层不再有 `_impl` 转发层：真实命令入口的
    //! `State<'_, AppState>` 只能来自 tauri `Manager`，harness 因此自持
    //! mock app（与 `commands/*_command_test_host.rs` 同形；这里走
    //! feature 门而非 `cfg(test)`，供 `src-tauri/tests/` 集成测试二进制
    //! 消费——tauri 的 `test` feature 由本 crate 的 dev-dependency 与
    //! `test-harness` feature 一起点亮）。

    use super::AppState;
    use crate::testing::app_state_test_support::TestStateKeepalive;
    use datazen_driver_api::{CommandResult, DriverCommandDefinition};
    use tauri::Manager;

    pub use super::ExecuteDriverCommandRequest;
    pub use crate::testing::app_state_test_support::TestAppState;

    /// A [`TestAppState`] hosted behind a mock command app.
    ///
    /// `keep` holds the parts that must outlive the managed state (tempdir,
    /// store, mock driver); callers keep driving the mock through it.
    pub struct HostedApp {
        app: tauri::App<tauri::test::MockRuntime>,
        pub keep: TestStateKeepalive,
    }

    impl HostedApp {
        /// Manage a harness's state behind a fresh mock app.
        pub fn new(test: TestAppState) -> Self {
            let (state, keep) = test.into_state();
            let app = tauri::test::mock_app();
            app.manage(state);
            Self { app, keep }
        }

        /// The managed state, as a tauri `State` handle for command parameters.
        fn state(&self) -> tauri::State<'_, AppState> {
            self.app.state::<AppState>()
        }
    }

    pub async fn get_connections(
        host: &HostedApp,
    ) -> Result<Vec<datazen_driver_api::ConnectionConfig>, String> {
        super::connection::get_connections(host.state())
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn connect(
        host: &HostedApp,
        connection_id: String,
    ) -> Result<crate::commands::connection::ConnectionSessionRef, String> {
        super::connection::connect(host.state(), connection_id)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn get_connection_commands(
        host: &HostedApp,
        db_session_id: String,
    ) -> Result<Vec<DriverCommandDefinition>, String> {
        super::driver_command::get_connection_commands(host.state(), db_session_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Drive `execute_driver_command`'s orchestration core for a hosted app.
    ///
    /// 走域的跨域共享入口 [`super::driver_command::execute_driver_command_impl`]
    /// 而非 `#[tauri::command]` 命令入口：GUI 命令多收一个具体
    /// `tauri::AppHandle`（= Wry 运行时）参数，而 mock app 只能产出
    /// `AppHandle<MockRuntime>`——域规则（`run_driver_command` 的可见性
    /// 注释）明言域外调用方「一律走命令入口或跨域共享入口」，本 harness
    /// 即后者。两个入口共用同一编排核心，对不声明保存对话框的命令行为
    /// 逐字一致（含错误文案）；声明保存对话框的命令在共享入口被拒——
    /// 纯 IPC 测试本就无从弹原生对话框（与
    /// `driver_command_test_host.rs` 不托管该命令入口是同一个决定）。
    pub async fn execute_driver_command(
        host: &HostedApp,
        request: ExecuteDriverCommandRequest,
    ) -> Result<CommandResult, String> {
        super::driver_command::execute_driver_command_impl(&host.state(), request)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[path = "ipc_surface_tests.rs"]
mod ipc_surface_tests;

#[cfg(test)]
#[path = "org_scope_surface_tests.rs"]
mod org_scope_surface_tests;

#[cfg(test)]
#[path = "app_state_shape_tests.rs"]
mod app_state_shape_tests;
#[cfg(test)]
#[path = "named_views_error_tests.rs"]
mod named_views_error_tests;

// WR-C3 IPC 腿（named_views 域）：同用例双传输契约测试。共享规格在
// `datazen::use_case_contracts_test_support::named_views`，HTTP 腿在
// `crates/datazen-server/tests/use_case_contracts.rs`，scenario 逐字同名。
#[cfg(test)]
mod named_views_use_case_contract_tests;
