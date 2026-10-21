//! 活连接**出向端口**的桌面适配器：把一个仍登记在 runtime 台账里的会话，
//! 映射到它**已经开着**的那份物理资源。
//!
//! ## 这个文件为什么必须存在
//!
//! `LiveConnectionSource`（`application/live.rs`）是 runtime 的出向端口：宿主实现它，
//! runtime 在执行前调它，取回 `(Arc<dyn DatabaseDriver>, ConnectionHandle)`。
//! 在桌面侧此前**没有任何实现**，于是端口恒为未接线：`borrow_live_connection`
//! 每次都在 `LiveConnectionError::NotWired` 上失败，宿主执行网关一条路都不通。
//!
//! ## 三件它做不到的事，以及为什么必须拒绝而不是变通
//!
//! | 做不到的事 | 为什么 | 这里的回答 |
//! | --- | --- | --- |
//! | 从 `SessionHandle` 反查物理资源 | `DesktopSessionBackend::resources` 的 key 是不透明 `resource_id`（uuid），而端口只带 `{ dbSessionId, runtimeEpoch }` | 由 backend 的 `by_session` 反向索引补上，key 来自 `OpenResource::db_session_id` |
//! | 新建一个连接交给调用方 | 端口契约逐字禁止：实现方不得在此新建物理连接，物理所有权在 runtime 台账 | 一律 `Refused`，宁可不借 |
//! | 从 `ResourceProvider` 拿回 `Arc<dyn DatabaseDriver>` | provider 是 `dyn ResourceProvider`，没有 generic 的 provider → driver 接缝 | 见下面「驱动引用从哪来」 |
//!
//! ## 驱动引用从哪来（本文件最需要交代的一处）
//!
//! `LiveConnection` 需要 `Arc<dyn DatabaseDriver>`。[`DriverRegistry`] 是**唯一**
//! 能不违反「不新建物理连接」交出它的地方：
//!
//! * `DriverRegistry::get` 走它自己那张 `drivers` 表，命中即返回**同一个** `Arc`。
//!   克隆它只加一次引用计数，不触发握手。
//! * path 驱动的 `factory.create()` 与 `factory.resource_provider()` 共享同一个
//!   进程内单例（postgres：`POSTGRES_DRIVER` / `POSTGRES_PROVIDER` 两个 `OnceLock`
//!   指向同一个 `PostgresDriver`），因此这里拿到的驱动与 `session_backend` 当初
//!   `acquire_resource` 用的是**同一个实例、同一批池子**。
//!
//! 这一点无法由 `dyn DatabaseDriver` 的方法签名证明，所以它落在测试上而不是
//! 落在注释上：`同一驱动实例被重复取出` 一例钉住「取两次得到同一个 `Arc`」。
//! 若某个驱动不满足（每次 `create()` 返回新实例），它就不能接这个端口——
//! 那是驱动缺陷，不是这里的变通空间。
//!
//! ## 反向索引由谁填
//!
//! backend 自己的 `open`。为此 `OpenResource` 加了 `db_session_id` 字段——它是
//! actor 从 `OpenRequest::db_session_id` 透传的同一个 id，**不是新事实**，只是第一次
//! 越过接缝到达后端。少了它，宿主在 `open` 里认不出「这一份资源将来归哪个会话」，
//! 于是要么新建连接、要么永远拒绝。
//!
//! ## `ConnectionHandle` 从哪来
//!
//! 从 backend 那一行 [`Resource`] 调用 provider 的 `connection_handle()` 取得。Runtime 的
//! `dr::ResourceHandle` 是不透明的所有权凭证，`resource_key` 和 `provider_id` 不能直接
//! 充当驱动自己的池路由字段；provider 负责验证资源仍然存活并返回它实际持有的句柄。
//!
//! ## 反向索引由谁填
//!
//! 不是 backend 的 `open`。`SessionBackend::open` 收到的 `OpenResource` 里
//! **没有** `db_session_id`（只有 `connection_id` + `runtime_epoch`），而 epoch 也
//! 没有公开的反查表。所以绑定由知道 `dbSessionId` 的那一侧——[`super::PlatformAdapter`]
//! ——在建会话成功后显式调用。backend 只负责「按 `dbSessionId + epoch` 查」与「关会话时摘」。
//! 代价是绑定晚于 `open` 一个指令：那个窗口里借出会得到 `SessionLost` 而不是一份
//! 真资源，这是**正确**的失败方向（宁可拒绝，不能交出一份对不上的引用）。

use async_trait::async_trait;
use std::sync::Arc;

use datazen_runtime::application::{
    LiveConnection, LiveConnectionError, LiveConnectionRequest, LiveConnectionSource,
};

use super::session_backend::DesktopSessionBackend;
use crate::db::DriverRegistry;
use crate::store::Store;

/// 桌面侧出向端口实现。
pub struct DesktopLiveConnectionSource {
    registry: Arc<DriverRegistry>,
    store: Arc<Store>,
    backend: Arc<DesktopSessionBackend>,
}

impl DesktopLiveConnectionSource {
    pub fn new(
        registry: Arc<DriverRegistry>,
        store: Arc<Store>,
        backend: Arc<DesktopSessionBackend>,
    ) -> Self {
        Self {
            registry,
            store,
            backend,
        }
    }

    /// 解析这个会话**此刻**对应的 driver type。
    ///
    /// 走 `store.platform_profile` 而不是 registry 的反查：会话的 `connectionId`
    /// 是持久化配置的 id，driver type 只有配置里那一份是权威的。取不到 ⇒ 会话已
    /// 被删除/禁用，这时**必须**拒绝，不允许回落到「随便挑一个驱动」。
    async fn driver_type_of(&self, connection_id: &str) -> Option<String> {
        self.store
            .platform_profile(connection_id)
            .await
            .map(|(config, _)| config.database_type.as_str().to_owned())
    }
}

#[async_trait]
impl LiveConnectionSource for DesktopLiveConnectionSource {
    async fn open_live(
        &self,
        request: LiveConnectionRequest,
    ) -> Result<LiveConnection, LiveConnectionError> {
        let epoch = request.handle.runtime_epoch.get();
        let db_session_id = request.handle.db_session_id.as_str();

        // 1. 反查物理资源。句柄不认识 ⇒ 会话已关/已驱逐，或绑定还没填上；
        //    epoch 不匹配 ⇒ 上一个纪元的残留行（INV-07）。两种都不新建、不透明替换。
        let resource = self
            .backend
            .live_resource_for_session(db_session_id, epoch)
            .await
            .ok_or_else(|| {
                LiveConnectionError::SessionLost(format!(
                    "no live physical resource is bound to session {db_session_id} at epoch {epoch}"
                ))
            })?;

        // 2. 解析权威 driver type。
        let driver_type = self
            .driver_type_of(request.connection_id.as_str())
            .await
            .ok_or_else(|| {
                LiveConnectionError::Refused(format!(
                    "connection profile {} is unavailable; refusing to guess a driver",
                    request.connection_id.as_str()
                ))
            })?;

        // 3. 取驱动引用。这一步可能真的加载驱动（首次），但加载的是**驱动注册表项**，
        //    不是物理连接：驱动内部的池子才是 socket 的家，而那个池子此刻已经开着
        //    上面那份资源。因此这一步不违反「不得新建物理连接」。
        let driver = self.registry.get(&driver_type).await.ok_or_else(|| {
            LiveConnectionError::Refused(format!(
                "driver {driver_type} is not loaded; refusing to substitute another driver"
            ))
        })?;

        // 4. Provider validates the live resource and lends its actual driver handle.
        let handle = resource.driver_handle().map_err(|_| {
            LiveConnectionError::Refused(
                "the runtime resource provider cannot lend its live driver handle".into(),
            )
        })?;
        Ok(LiveConnection::new(driver, handle))
    }
}

/// 测试夹具与用例放在单独文件：本文件的模块头已经很长，而用例的价值在于
/// 「三条判据」而不是「放在哪个文件里」。
#[cfg(test)]
#[path = "live_source_tests.rs"]
mod tests;
