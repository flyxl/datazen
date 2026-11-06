//! WR-F7：runtime 租约端口（`RuntimeLeaseSource`）——执行域的中立归属。
//!
//! 原 `mcp::runtime` 内定义（随 WR-F2 引入）；WR-F7 真租约源落地后，执行
//! 域路由的用例适配器（`lease_use_cases.rs`）与 MCP 后端共用同一端口——
//! 端口归执行域，MCP 传输与 HTTP 域路由都是它的消费者。
//!
//! ## 「每调用现取，禁止缓存」约束的是消费者（CM-63 落地后的语义）
//!
//! 端口契约 `resolve()` 的硬规则面向**消费者**：`LeaseConnectionUseCases`、
//! `ServerTargetRunner`、`ServerResourceExecutionHost`、MCP 后端都不得把
//! 用例面缓存跨请求持有——每次用例调用前必须向租约源索要，从而尊重
//! 「当前实例可能被吊销」。这条纪律与历史版本一致，消费者侧零改动。
//!
//! 租约源**自身**则是「当前实例」的唯一归属者：CM-63 起
//! `ServerRuntimeLeaseSource` 在内部持有一个当前 `RuntimeConnectionUseCases`
//! 单元（`current` cell），`resolve()` 未吊销时恒返同一 `Arc`。cell 是被
//! 认可的归属——「禁止缓存」禁令不再约束租约源本身。跨 resolve 的会话、
//! 令牌、幂等回执因此连续（CM-63：此前每 resolve 现构造新实例，A 请求开的
//! 会话对 B 请求必然 `SessionNotFound`）。
//!
//! 吊销语义：[`ServerRuntimeLeaseSource::revoke`] 置空 cell，下一次
//! `resolve()` 重建新实例。旧实例整体作废——旧会话按既有 `SessionLost`
//! 语义失效（会话账随旧实例不可达，等价于「从未存在」的 `SessionNotFound`；
//! 结果不受影响，结果落共享 sink）。v1 只暴露 API，bootstrap 不接线：
//! profiles/policy/backend 都是长生命周期 `Arc`，配置漂移由每请求 CAS 拒绝，
//! 不存在「实例整体过期」的现实触发条件。
//!
//! ## 活连接端口随实例装配（W8 刀2）
//!
//! `RuntimeConnectionUseCases::new_with_parts` 构造的实例自带未接线的
//! `LiveConnections`（`LiveConnections::unwired()`）——借出路径在未接线时
//! 得到 `NotWired` → `convert::live` 映射 `ServiceUnavailable`（诚实失败）。
//! server 的 runtime 出生地在租约源慢路（cell 懒构造），宿主能摸到新生实例
//! 的唯一位置就是 `resolve()` 自己——接线因此写在慢路里：`live_source`
//! 字段（[`Self::with_live_source`] 注入）为 `Some` 时，每个新造实例构造后
//! 立即 `with_live_connections` 接线。`with_live_connections` 是**覆盖**语义，
//! 慢路重入无副作用；吊销→重建的下一个实例**天然带端口**（外部「resolve
//! 一次再接线」的形状会在重建窗口后退化回 NotWired）。缺省 `None` 保持
//! unwired——既有构造器与全部既有测试零改动，行为与刀2 前逐字节一致。

use std::sync::Arc;

use async_trait::async_trait;

use datazen::execution::application::RuntimeConnectionUseCases;
use datazen::execution::application::orchestration::LiveConnectionSource;

/// runtime 租约端口：消费者每次调用前现取当前 runtime（可能被吊销，
/// 禁止跨请求缓存——见模块头「约束的是消费者」）。
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
/// **当前实例归属（CM-63）**：`resolve()` 快路返回 cell 里的当前实例；
/// 慢路（未建/已吊销）在锁内现构造
/// `RuntimeConnectionUseCases::new_with_parts(..)` 并存入 cell——构建全同步
/// （Arc 克隆 + 小 map + 随机 secret），无 await 点，std::sync::Mutex 即可
/// 且不跨 await 持锁。共享的 `RuntimeResultSink` 与会话目录跨 resolve/
/// 吊销保持同一实例（结果必须落进准入执行时的那一个 sink；目录是 sessions
/// 域可见性闸的归属事实源）。
pub struct ServerRuntimeLeaseSource {
    profiles: Arc<dyn datazen::connection::domain::ProfileRepository>,
    policy: Arc<dyn datazen::execution::application::policy::PolicyService>,
    backend: Arc<dyn datazen::execution::runtime::registry::backend::SessionBackend>,
    /// 跨 resolve 共享的结果 sink（WR-F7 publisher 接线的对偶）。
    sink: datazen::execution::application::orchestration::RuntimeResultSink,
    /// 跨 resolve 共享的会话目录（sessions 域可见性闸的归属事实源——
    /// 与用例实现持有**同一**实例；域文件模块头的接线登记在此落地）。
    directory: Arc<datazen::execution::runtime::directory::InMemorySessionDirectory>,
    /// 当前 runtime 单元（CM-63）：`Some` 即「未吊销的当前实例」，`resolve()`
    /// 恒返同一 `Arc`；`None` 触发下次 resolve 重建。吊销走 [`Self::revoke`]。
    current: std::sync::Mutex<Option<Arc<RuntimeConnectionUseCases>>>,
    /// 活连接出向端口适配器（W8 刀2）：`Some` 时每个新造 runtime 实例构造
    /// 后立即 `with_live_connections` 接线（覆盖语义，慢路重入无副作用）；
    /// `None` 保持 unwired（fail-closed：借出路径得 `NotWired` → 503）。
    live_source: Option<Arc<dyn LiveConnectionSource>>,
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
            current: std::sync::Mutex::new(None),
            live_source: None,
        }
    }

    /// 注入活连接出向端口适配器（W8 刀2）：`resolve()` 慢路构造的每个新
    /// 实例构造后立即 `with_live_connections` 接线。builder 形状——既有
    /// 构造器（`new` / `new_with_shared_parts`）与不调用本方法的实例零改动
    /// （缺省 unwired，fail-closed）。
    pub fn with_live_source(mut self, source: Arc<dyn LiveConnectionSource>) -> Self {
        self.live_source = Some(source);
        self
    }

    /// sessions 域可见性闸用的目录出口（与每次 resolve 注入用例的是同一
    /// 实例；bootstrap 把它递给 `domains::SessionsState.directory`）。
    pub fn session_directory(
        &self,
    ) -> Arc<datazen::execution::runtime::directory::InMemorySessionDirectory> {
        self.directory.clone()
    }

    /// 吊销当前 runtime 单元（CM-63）：cell 置空，下一次 `resolve()` 重建
    /// 新实例。旧实例整体作废，旧会话按既有 `SessionLost` 语义失效；结果
    /// 落共享 sink，不受吊销影响。v1 只暴露 API，bootstrap 不接线（模块头
    /// 「吊销语义」登记了触发点裁决）。
    pub fn revoke(&self) {
        *self
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

#[async_trait::async_trait]
impl RuntimeLeaseSource for ServerRuntimeLeaseSource {
    /// 快路：cell 有当前实例 → clone `Arc` 返回（O(1)，无重建）。慢路：
    /// 锁内 `new_with_parts(...)`（与 CM-63 前的逐参构造一致，
    /// `SystemRuntimeClock(Instant::now())` 语义不变）→ 存入 cell → 返回。
    /// 构建全同步且无 I/O，锁内重建窗口不产生「半成品」实例；锁不跨 await。
    async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String> {
        if let Some(runtime) = self
            .current
            .lock()
            .map(|cell| cell.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
        {
            return Ok(runtime);
        }
        let mut cell = self
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // 双检：并发慢路里先到者已建好，后来者直接复用（单元仍只建一次）。
        if let Some(runtime) = cell.clone() {
            return Ok(runtime);
        }
        let runtime = Arc::new(RuntimeConnectionUseCases::new_with_parts(
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
        ));
        // 活连接端口随实例装配（W8 刀2）：接线写在慢路里，吊销→重建的
        // 下一个实例天然带端口；`with_live_connections` 覆盖语义，重入无副作用。
        if let Some(live) = &self.live_source {
            runtime.with_live_connections(live.clone());
        }
        *cell = Some(Arc::clone(&runtime));
        Ok(runtime)
    }
}
