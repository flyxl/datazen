//! Tauri IPC command surface.

pub mod ai;
mod app_archive;
mod backup;
mod clipboard;
mod config;
mod connection;
mod connection_import;
mod context;
mod dashboard;
mod data;
mod data_session;
mod data_transfer;
pub(crate) mod dialog;
pub(crate) mod driver_command;
mod encryption_key;
mod error;
mod export;
mod file;
pub(crate) mod file_jobs;
mod history;
pub mod mcp;
mod native_tool;
mod query;
pub(crate) mod query_favorites;
mod restore_gateway;
mod restore_job;
mod sample;
mod schema;
pub mod schema_diff;
mod sync;
mod theme;
mod tunnel;
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
// The dialog module is private; its commands are only re-exported for webdriver.
#[cfg(feature = "webdriver")]
pub use dialog::*;
pub use driver_command::*;
pub(crate) use error::{CmdExt, CommandError};
pub use export::*;
pub use file::*;
pub use history::*;
pub use mcp::*;
pub use native_tool::*;
pub use query::*;
pub use query_favorites::*;
pub use sample::*;
pub use schema::*;
pub use schema_diff::*;
pub use sync::*;
pub use theme::*;
pub use tunnel::*;
pub use wapps::*;
pub use window::*;
pub use workflow::*;

use crate::mcp::McpClientManager;
use crate::monitor::engine::MonitorEngine;
use crate::store::Store;
use crate::transfer::adapter_registry::SyncAdapterRegistry;
use crate::workflow::history::WorkflowHistoryManager;
use crate::workflow::registry::WorkflowRegistry;
use crate::workflow::scheduler::WorkflowScheduler;
use datazen_ai_providers::AiProviderRegistry;
use datazen_application::error::ApiError;
use datazen_driver_api::QueryExecutionId;
use datazen_feature_core::ai::context::SchemaContextBuilder;
use datazen_feature_core::ai::prompt_resolver::PromptResolver;
use datazen_platform_api::id::OrganizationId;
use datazen_platform_api::ports::scope::OrganizationScope;
use datazen_runtime::driver_registry::registry::DriverRegistry;
use datazen_runtime::schema_cache::SchemaCache;
use datazen_workspace_apps::manager::WappManager;
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
pub struct SyncStorePorts {
    /// 计划注册表端口。签发 / 查看 / 绑定端点身份 / 认领。
    pub plan_store: Arc<dyn datazen_data_sync::plan_store::SyncPlanStore>,
    /// 比较存储根端口。决定比较结果写到哪个目录。
    pub comparison_root: Arc<dyn datazen_data_sync::comparison_store::ComparisonStoreRoot>,
}

impl SyncStorePorts {
    /// 桌面形态的默认接线：一个本地根 + 一个进程内计划注册表。
    ///
    /// 桌面是单租户：比较存储的根只属于本地组织这一个组织。
    pub fn desktop() -> Self {
        let organization = OrganizationId::new(crate::platform::identity::LOCAL_ORGANIZATION_ID);
        Self {
            plan_store: Arc::new(datazen_data_sync::plan_store::InMemorySyncPlanStore::new()),
            comparison_root: Arc::new(
                datazen_data_sync::comparison_store::LocalComparisonStoreRoot::new(
                    datazen_data_sync::comparison_store::default_comparison_store_root(),
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
    pub store: Arc<Store>,
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
    pub cancel_registry: datazen_feature_core::ai::cancel::CancellationRegistry,
    /// Owner-bound editor-session grants for AI tool execution (P6 §14.4).
    pub ai_grants: Arc<crate::commands::ai::AiGrantService>,
    /// P5 Wave-1：Schema Diff 的 JobRuntime 基础设施（仓储/预算/计划库）。
    pub schema_diff_jobs: Arc<crate::commands::schema_diff::job::SchemaDiffJobInfra>,
    /// P5 本机持久化 Job facade，所有迁移 Host 共用 `{appData}/datazen.sqlite`。
    pub desktop_job_host: Arc<datazen_runtime::job::JobHost>,
    /// Data Sync 的存储端口实例：计划注册表 + 比较存储根。
    ///
    /// 过去计划注册表是共享包里的 `static STORE`（进程级全局可变状态），
    /// 现在由组装点创建实例并注入。
    pub sync_stores: SyncStorePorts,
    /// 概要 §6.2 第 8 步的窄适配根：桌面身份 + owner 作用域句柄登记表。
    ///
    /// 事务归属和资源生命周期由 Runtime session actor 管理。取不到 adapter 时用
    /// `AppState::platform_require()`，它返回明确错误而不是 `Option` 兜底。
    pub platform: crate::platform::PlatformEntry,
}

impl AppState {
    /// 取 platform adapter；组装失败 ⇒ 返回原始 `ApiError`，不做任何回落。
    ///
    /// 命令侧的统一入口：新接线的能力一律从这里进，避免每个命令各写一遍
    /// `is_some()` 判断把失败吞成默认行为。
    pub fn platform_require(&self) -> Result<&Arc<crate::platform::PlatformAdapter>, ApiError> {
        self.platform.require()
    }

    /// 本次调用的**组织作用域**。
    ///
    /// 作用域只表达组织维度，且只能由 adapter 组装的身份上下文推导（INV-01）：
    /// [`OrganizationScope::from_context`]。桌面是单租户——组织维度是进程常量
    /// `LOCAL_ORGANIZATION_ID`，与「主体是谁」无关。
    ///
    /// 因此桌面身份组装失败（拿不到当前登录用户）时**不**让存储访问整体失效：
    /// 变的是主体，不是「哪个组织」。此时按同一个常量给出作用域，端口侧的
    /// 跨组织判定（越权访问按「不存在」处理）依然成立。
    pub fn organization_scope(&self) -> OrganizationScope {
        match self.platform_require() {
            Ok(adapter) => OrganizationScope::from_context(adapter.begin_call().context()),
            Err(error) => {
                tracing::warn!(
                    code = ?error.code,
                    "桌面身份不可用：组织作用域沿用本地组织常量"
                );
                OrganizationScope::new(OrganizationId::new(
                    crate::platform::identity::LOCAL_ORGANIZATION_ID,
                ))
            }
        }
    }

    /// Lazily register AI providers + load prompt templates.
    /// Does not run during GUI startup — only on first AI / prompt use.
    pub async fn ensure_ai_ready(&self) {
        if self.ai_registry.ensure_registered().await {
            if let Some(ai_config) = self.store.get_ai_config().await {
                self.ai_registry.restore_config(&ai_config).await;
            }
        }
        let lang = self.store.get_settings().await.language;
        self.prompt_resolver.ensure_ready(&lang).await;
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
    use super::{
        connect_impl, execute_driver_command_impl, get_connection_commands_impl,
        get_connections_impl, AppState,
    };
    use datazen_driver_api::{CommandResult, DriverCommandDefinition};

    pub use super::ExecuteDriverCommandRequest;
    pub use crate::testing::app_state::TestAppState;

    pub async fn get_connections(
        state: &AppState,
    ) -> Result<Vec<datazen_driver_api::ConnectionConfig>, String> {
        get_connections_impl(state).await.map_err(|e| e.to_string())
    }

    pub async fn connect(
        state: &AppState,
        connection_id: String,
    ) -> Result<crate::commands::connection::ConnectionSessionRef, String> {
        connect_impl(state, connection_id)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn get_connection_commands(
        state: &AppState,
        db_session_id: String,
    ) -> Result<Vec<DriverCommandDefinition>, String> {
        get_connection_commands_impl(state, db_session_id)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn execute_driver_command(
        state: &AppState,
        request: ExecuteDriverCommandRequest,
    ) -> Result<CommandResult, String> {
        execute_driver_command_impl(state, request)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[path = "ipc_surface_tests.rs"]
mod ipc_surface_tests;

#[cfg(test)]
#[path = "tunnel_probe_tests.rs"]
mod tunnel_probe_tests;

#[cfg(test)]
#[path = "tunnel_summary_tests.rs"]
mod tunnel_summary_tests;
