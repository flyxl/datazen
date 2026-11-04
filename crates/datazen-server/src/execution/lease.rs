//! WR-F7：runtime 租约端口（`RuntimeLeaseSource`）——执行域的中立归属。
//!
//! 原 `mcp::runtime` 内定义（随 WR-F2 引入）；WR-F7 真租约源落地后，执行
//! 域路由的用例适配器（`lease_use_cases.rs`）与 MCP 后端共用同一端口——
//! 端口归执行域，MCP 传输与 HTTP 域路由都是它的消费者。语义不变：
//! **每调用现取，禁止缓存**（runtime 可能被吊销）。

use std::sync::Arc;

use async_trait::async_trait;

use datazen::execution::application::RuntimeConnectionUseCases;

/// runtime 租约端口：每次调用现取当前 runtime（可能被吊销，禁止缓存）。
#[async_trait::async_trait]
pub trait RuntimeLeaseSource: Send + Sync {
    async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String>;
}

/// WR-F7：runtime 租约源的真实现（server 形态）。
///
/// 三件套全部由 bootstrap 显式递交：`profiles` = `storage::ServerProfiles`
/// （`ProfileRepository` 端口）、`policy` = 实现了 modules
/// `execution::application::policy::PolicyService` 的适配器
/// （`crate::execution::policy_adapter`）、`backend` =
/// `execution::driver_exec::ServerDriverExecBackend`（`SessionBackend` 端口
/// 的 server v1 实现）。
///
/// **每调用现取，禁止缓存**：`resolve()` 每次现构造
/// `RuntimeConnectionUseCases::new_with_parts(..)`——runtime 可能被吊销，
/// 端口语义不允许把用例面缓存在租约源里；共享的 `RuntimeResultSink` 是
/// 例外（结果必须落进准入执行时的那一个 sink，见模块文档）。
pub struct ServerRuntimeLeaseSource {
    profiles: Arc<dyn datazen::connection::domain::ProfileRepository>,
    policy: Arc<dyn datazen::execution::application::policy::PolicyService>,
    backend: Arc<dyn datazen::execution::runtime::registry::backend::SessionBackend>,
    /// 跨 resolve 共享的结果 sink（WR-F7 publisher 接线的对偶）。
    sink: datazen::execution::application::orchestration::RuntimeResultSink,
    /// 跨 resolve 共享的会话目录（sessions 域可见性闸的归属事实源——
    /// 与用例实现持有**同一**实例；域文件模块头的接线登记在此落地）。
    directory: Arc<datazen::execution::runtime::directory::InMemorySessionDirectory>,
}

impl ServerRuntimeLeaseSource {
    /// 构造（bootstrap 装配入口）。sink 缺省为独立实例——只有当主线同时
    /// 接了 [`crate::execution::publisher::execution_result_publisher`] 时
    /// 才必须走 [`Self::new_with_sink`] 保持同实例。
    pub fn new(
        profiles: Arc<dyn datazen::connection::domain::ProfileRepository>,
        policy: Arc<dyn datazen::execution::application::policy::PolicyService>,
        backend: Arc<dyn datazen::execution::runtime::registry::backend::SessionBackend>,
    ) -> Self {
        Self::new_with_shared_parts(
            profiles,
            policy,
            backend,
            datazen::execution::application::orchestration::RuntimeResultSink::default(),
            Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new()),
        )
    }

    /// 构造（共享形态）：`sink` 交给发布钩子、`directory` 交给 sessions 域
    /// 可见性闸——两者都必须与租约源是**同一实例**（跨 resolve 共享）。
    pub fn new_with_shared_parts(
        profiles: Arc<dyn datazen::connection::domain::ProfileRepository>,
        policy: Arc<dyn datazen::execution::application::policy::PolicyService>,
        backend: Arc<dyn datazen::execution::runtime::registry::backend::SessionBackend>,
        sink: datazen::execution::application::orchestration::RuntimeResultSink,
        directory: Arc<datazen::execution::runtime::directory::InMemorySessionDirectory>,
    ) -> Self {
        Self {
            profiles,
            policy,
            backend,
            sink,
            directory,
        }
    }

    /// sessions 域可见性闸用的目录出口（与每次 resolve 注入用例的是同一
    /// 实例；bootstrap 把它递给 `domains::SessionsState.directory`）。
    pub fn session_directory(
        &self,
    ) -> Arc<datazen::execution::runtime::directory::InMemorySessionDirectory> {
        self.directory.clone()
    }
}

#[async_trait::async_trait]
impl RuntimeLeaseSource for ServerRuntimeLeaseSource {
    async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String> {
        Ok(Arc::new(RuntimeConnectionUseCases::new_with_parts(
            self.profiles.clone(),
            self.policy.clone(),
            self.backend.clone(),
            Arc::new(
                datazen::execution::application::orchestration::SystemRuntimeClock(
                    std::time::Instant::now(),
                ),
            ) as Arc<dyn datazen::execution::runtime::gateway::MonotonicClock>,
            self.sink.clone(),
            self.directory.clone(),
        )))
    }
}
