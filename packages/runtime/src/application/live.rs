//! 活连接**出口端口**（live connection exit port）。
//!
//! 新路径的其余部分一律不出现原始驱动/句柄引用：13 个 `ConnectionUseCases` 方法与
//! 4 个固有方法全部只回 DTO，[`crate::application`] 里没有任何一处引用
//! `Arc<dyn DatabaseDriver>` 或 `ConnectionHandle`。宿主唯一需要原始引用的地方是它的
//! 执行网关（driver command 的 `resolve`），那份引用必须**从某一处**来。本模块就是
//! 那一处，并且它是**端口**而不是垫片：runtime 声明契约，宿主适配器实现，方向永远是
//! runtime 出、宿主进。runtime 不反向引用宿主的 driver registry，也不给宿主留后门。
//!
//! 端口方向与 [`crate::registry::backend::SessionBackend`] 相反，可对照记忆：
//! `SessionBackend` 是**入向**端口（runtime 调用宿主去开/执行/关物理资源），
//! [`LiveConnectionSource`] 是**出向**端口（宿主调用 runtime 去取一个已经活着的物理引用）。
//! 两者不是同一个面，因此本端口不挂在 `SessionBackend` 上。
//!
//! # 所有权与生命周期契约（这一段就是本模块存在的理由）
//!
//! 「活连接」这个词容易读成「谁拿到谁拥有」。不是。本模块把它拆成四句话，每一句都落到
//! 下面的类型上：
//!
//! | 问题 | 答案 | 落点 |
//! | --- | --- | --- |
//! | 谁取得 | 只有 [`RuntimeConnectionUseCases`]，且只在**授权 + 会话解析 + epoch 校验**之后 | [`LiveConnections::borrow`] |
//! | 取得的是什么 | **借用**，不是所有权转移 | [`LiveConnection`] |
//! | 持有多久 | 到 [`LiveConnectionLease`] 被归还为止；显式 `release` 与 `Drop` 等价 | [`LiveConnectionLease::release`] |
//! | 谁归还 | **只能由持有者归还**；runtime 不能替它归还，也绝不会替它多借一次 | [`LiveConnectionLedger::release`] |
//!
//! ## 取得的是借用，不是所有权
//!
//! [`LiveConnection`] 里的两个字段**都不构成物理连接的所有权**：
//!
//! - `driver: Arc<dyn DatabaseDriver>` 是驱动**注册表项**的共享引用。克隆它只增加一个
//!   引用计数，不会新建物理连接，也不会触发握手；驱动内部那个连接池才是 socket 的家。
//! - `handle: ConnectionHandle` 是值拷贝的路由元数据（`{ id, pool_id }`），本身不持有
//!   任何 OS 资源。
//!
//! 所以物理资源的**所有者始终是 runtime 侧的台账**（`registry` 的会话 actor + `budget`
//! 的配额），借用者只是在一段时间内拿到一个够用的引用。持有者**不得**把它当作连接的所有权
//! 凭证去 `connect` / `close` / 建池 / 改池参数，也**不得**把它存进任何跨会话的结构里。
//!
//! ## 归还只有一条路，且必须有兜底
//!
//! 归还走 [`LiveConnectionLease`] 的 RAII：显式 [`LiveConnectionLease::release`] 与
//! [`Drop`] 落到同一个私有例程，因此**取消、超时、panic、`?` 提前返回、任务被 abort**
//! 这些「不正常」的路径与正常路径共用同一条归还逻辑——没有一条异常路径可以绕过它。
//! 这正是异常路径不漏登记的根据：漏登记的先决条件是「持有者没归还」，而漏不掉。
//!
//! ## 被回收/关闭时，旧借用立刻失效
//!
//! 空闲驱逐（[`crate::application::idle`]）或显式关闭可能发生在借用期间：台账行已经
//! 摘掉，物理资源已经发出关闭，此时持有者手里的引用指向的是一个**已经不存在的物理资源**。
//! 本模块不假装没发生，而是把该会话上的全部在借借用**作废**
//! （[`LiveConnectionLedger::revoke`]`）。作废之后 [`LiveConnectionLease::connection`]
//! 拒绝出借（[`LiveConnectionError::Revoked`]），持有者必须走正常执行路径才能发现
//! `SessionLost`，而不是拿着一个看似可用的句柄去驱动上打一发请求。
//!
//! ## 台账是「谁能写」的唯一答案
//!
//! [`LiveConnectionLedger`] 与 [`crate::directory`]、`crate::resource::ResourceManager`、
//! [`crate::budget`] 的形状一致：私有账本 + 对外只暴露登记/归还的入口 + 只读投影。
//! 引用计数只有一处，任何路径想减计数都必须过 [`LiveConnectionLedger::release`]，因此
//! 「归还恰好一次」由结构保证，而不是靠每个调用点自觉。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};

use super::results::lock;
use crate::connection::{ConnectionId, DbSessionId, SessionHandle};

/// 端口错误。
///
/// 刻意与 [`crate::connection::RuntimeError`]、`ApiError` 分开：这一层的错误**不**经
/// 请求面返回给前端，它只在 runtime 内部与宿主适配器之间流转。
#[derive(Debug, thiserror::Error)]
pub enum LiveConnectionError {
    /// 出口端口没接线（宿主还没提供适配器）。这是配置事实，不是故障。
    #[error("live connection port is not wired")]
    NotWired,
    /// 会话已经不存在于 runtime 台账中（关闭、驱逐、换 owner 之后 epoch 变了）。
    #[error("session is no longer registered: {0}")]
    SessionLost(String),
    /// 运行时纪元与登记时不一致（INV-07）。
    #[error("runtime epoch no longer matches")]
    RuntimeEpochMismatch,
    /// 借用期间会话被回收/关闭，引用已作废。
    #[error("borrow revoked: session was reclaimed while the borrow was outstanding")]
    Revoked,
    /// 宿主适配器拒绝出借。
    #[error("host refused to lend a live connection: {0}")]
    Refused(String),
}

/// 借用目的。仅用于日志与宿主侧的策略判断，不参与路由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivePurpose {
    /// 宿主执行一次 driver command 后归还。
    CommandExecution,
    /// 宿主长时间持有（流式导出、备份、迁移）。
    LongLivedHostWork,
    /// 宿主只读查看一次。
    Inspection,
}

impl LivePurpose {
    pub const fn as_str(self) -> &'static str {
        match self {
            LivePurpose::CommandExecution => "commandExecution",
            LivePurpose::LongLivedHostWork => "longLivedHostWork",
            LivePurpose::Inspection => "inspection",
        }
    }
}

/// 借出请求：runtime → 宿主。
///
/// 只有 DTO，没有任何原始引用。`handle` 携带 `runtime_epoch`，宿主适配器据此判定
/// 引用是否陈旧（INV-07：纪元不匹配必须拒绝，不得透明重建）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveConnectionRequest {
    pub handle: SessionHandle,
    pub connection_id: ConnectionId,
    pub purpose: LivePurpose,
}

/// 一次借用的结果物：**借用**，不是所有权。
///
/// 见模块头的「取得的是借用，不是所有权」。两个 `accessor` 返回的都是借用，
/// 本结构不提供任何 `into_driver` / `take` 之类把引用摘走的所有权型出口。
#[derive(Clone)]
pub struct LiveConnection {
    driver: Arc<dyn DatabaseDriver>,
    handle: ConnectionHandle,
}

impl LiveConnection {
    pub fn new(driver: Arc<dyn DatabaseDriver>, handle: ConnectionHandle) -> Self {
        Self { driver, handle }
    }

    /// 驱动注册表项的共享引用。克隆它不会新建物理连接。
    pub fn driver(&self) -> &Arc<dyn DatabaseDriver> {
        &self.driver
    }

    /// 路由元数据的值拷贝（`{ id, pool_id }`），本身不持有 OS 资源。
    pub fn handle(&self) -> &ConnectionHandle {
        &self.handle
    }
}

impl std::fmt::Debug for LiveConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveConnection")
            .field("handle_id", &self.handle.id)
            .field("pool_id", &self.handle.pool_id)
            .finish_non_exhaustive()
    }
}

/// **出向**端口：宿主适配器实现，runtime 调用。
///
/// runtime 只依赖这一层 trait，因此宿主怎么管理驱动注册表、怎么复用池子、
/// 走没走连接管理服务，对 runtime 都不构成耦合——这正是「端口」与「垫片」的区别。
#[async_trait]
pub trait LiveConnectionSource: Send + Sync {
    /// 为一个仍然登记在 runtime 台账里的会话借出活连接。
    ///
    /// 实现方**不得**在此新建物理连接：物理资源的所有权在 runtime 台账，实现方只是把
    /// 已存在的那一份交出来。
    async fn open_live(
        &self,
        request: LiveConnectionRequest,
    ) -> Result<LiveConnection, LiveConnectionError>;
}

/// 台账行：一个会话上尚未归还的借用。
struct BorrowRow {
    revocation: Arc<AtomicBool>,
    outstanding: usize,
}

/// 未归还借用的**唯一**事实源。
///
/// 私有字段 + 私有例程，对外只给登记/归还/作废三个写入口和若干只读投影——
/// 与 [`crate::directory`]、`crate::resource::ResourceManager`] 同构。
#[derive(Default)]
pub struct LiveConnectionLedger {
    rows: Mutex<BTreeMap<DbSessionId, BorrowRow>>,
    issued: AtomicU64,
}

impl LiveConnectionLedger {
    pub fn new() -> Self {
        Self {
            rows: Mutex::new(BTreeMap::new()),
            issued: AtomicU64::new(0),
        }
    }

    /// 登记一次借用，返回该会话的作废标志（凭证持有副本）。
    fn borrow(&self, id: &DbSessionId) -> Arc<AtomicBool> {
        let mut rows = lock(&self.rows);
        let row = rows.entry(id.clone()).or_insert_with(|| BorrowRow {
            revocation: Arc::new(AtomicBool::new(false)),
            outstanding: 0,
        });
        row.outstanding += 1;
        self.issued.fetch_add(1, Ordering::SeqCst);
        Arc::clone(&row.revocation)
    }

    /// 归还一次借用：计数减一，归零时摘行。
    ///
    /// 减计数只有这一处，所以「归还恰好一次」是结构性的；多减会被下面的守卫挡下，
    /// 不会把别的会话的计数算错。
    fn release(&self, id: &DbSessionId) {
        let mut rows = lock(&self.rows);
        let Some(row) = rows.get_mut(id) else {
            tracing::warn!(
                db_session_id = id.as_str(),
                "归还了一条不存在的借用登记（表项已摘除）：忽略，不影响其余行"
            );
            return;
        };
        if row.outstanding == 0 {
            tracing::warn!(
                db_session_id = id.as_str(),
                "借用计数已为 0 还被归还：忽略。这是重复归还，说明持有路径有 bug。"
            );
            return;
        }
        row.outstanding -= 1;
        if row.outstanding == 0 {
            rows.remove(id);
        }
    }

    /// 作废该会话上全部在借借用（驱逐/关闭后调用）。
    ///
    /// **不**摘表行：行要等最后一次归还才摘，否则作废本身就会造成计数泄漏。
    fn revoke(&self, id: &DbSessionId) -> usize {
        let rows = lock(&self.rows);
        match rows.get(id) {
            Some(row) => {
                row.revocation.store(true, Ordering::SeqCst);
                row.outstanding
            }
            None => 0,
        }
    }

    /// 全表在借借用总数。只读投影。
    pub fn outstanding(&self) -> usize {
        lock(&self.rows).values().map(|row| row.outstanding).sum()
    }

    /// 仍有在借借用的会话 id（用于回收扫描时记录「回收打断了借用」）。
    pub fn borrowed_sessions(&self) -> Vec<DbSessionId> {
        lock(&self.rows)
            .iter()
            .filter(|(_, row)| row.outstanding > 0)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 台账行数（登记行，不是借用数）。用于「异常路径不留登记行」断言。
    pub fn rows(&self) -> usize {
        lock(&self.rows).len()
    }

    /// 自进程起累计签发次数。
    pub fn issued(&self) -> u64 {
        self.issued.load(Ordering::SeqCst)
    }
}

/// 借用凭证。**必须**被归还；`Drop` 就是归还。
///
/// 显式 [`LiveConnectionLease::release`] 与 `Drop` 走同一个私有例程，因此取消、超时、
/// panic、提前 `?` 返回、任务 abort 全部自动归还——异常路径与正常路径没有分叉。
pub struct LiveConnectionLease {
    db_session_id: Option<DbSessionId>,
    ledger: Arc<LiveConnectionLedger>,
    revocation: Arc<AtomicBool>,
    connection: Option<LiveConnection>,
}

/// 归还结果：告诉持有者归还时这个会话是否已经不在了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveRelease {
    /// 归还时会话已被回收/关闭，持有期间拿到的是一个已作废的引用。
    pub revoked: bool,
}

impl LiveConnectionLease {
    /// 取用借用物。会话已被回收/关闭时拒绝——宁可让调用方去走执行路径拿 `SessionLost`，
    /// 也不能让它以为手里的句柄还能用。
    pub fn connection(&self) -> Result<&LiveConnection, LiveConnectionError> {
        if self.revocation.load(Ordering::SeqCst) {
            return Err(LiveConnectionError::Revoked);
        }
        self.connection
            .as_ref()
            .ok_or(LiveConnectionError::SessionLost(
                "borrow was already returned".to_string(),
            ))
    }

    /// 显式归还。`Drop` 兜底，二者等价且幂等。
    pub fn release(mut self) -> LiveRelease {
        let revoked = self.revocation.load(Ordering::SeqCst);
        self.return_inner();
        LiveRelease { revoked }
    }

    fn return_inner(&mut self) {
        // `Option::take` 让归还天然幂等：`Drop` 在显式归还之后再次触发是空操作。
        let Some(id) = self.db_session_id.take() else {
            return;
        };
        self.connection = None;
        self.ledger.release(&id);
    }
}

impl Drop for LiveConnectionLease {
    fn drop(&mut self) {
        self.return_inner();
    }
}

impl std::fmt::Debug for LiveConnectionLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveConnectionLease")
            .field(
                "db_session_id",
                &self.db_session_id.as_ref().map(DbSessionId::as_str),
            )
            .field("outstanding", &self.connection.is_some())
            .finish_non_exhaustive()
    }
}

/// 活连接门面：借出/归还的唯一写入口。
///
/// 形状对齐 [`crate::directory::InMemorySessionDirectory`]、`crate::budget` 与
/// [`crate::resource::ResourceManager`]：未接线是**配置事实**（`None`），不是错误计数；
/// 接线后所有写操作都收敛到本类型的方法上。
///
/// 克隆共享同一个台账：门面是外观，门面背后只有一本账。
#[derive(Clone)]
pub struct LiveConnections {
    source: Option<Arc<dyn LiveConnectionSource>>,
    ledger: Arc<LiveConnectionLedger>,
}

impl Default for LiveConnections {
    fn default() -> Self {
        Self::unwired()
    }
}

impl LiveConnections {
    /// 未接线。此时所有借出都得到 [`LiveConnectionError::NotWired`]。
    pub fn unwired() -> Self {
        Self {
            source: None,
            ledger: Arc::new(LiveConnectionLedger::new()),
        }
    }

    /// 接线：宿主适配器从此成为借出的唯一来源。
    pub fn wired(source: Arc<dyn LiveConnectionSource>) -> Self {
        Self {
            source: Some(source),
            ledger: Arc::new(LiveConnectionLedger::new()),
        }
    }

    pub fn is_wired(&self) -> bool {
        self.source.is_some()
    }

    /// 借出一次。**登记与签发必须同时成功或同时失败**：
    /// 宿主适配器报错时不登记，所以失败路径不会留下「已登记但没借出」的空行。
    pub async fn borrow(
        &self,
        request: LiveConnectionRequest,
    ) -> Result<LiveConnectionLease, LiveConnectionError> {
        let source = self.source.as_ref().ok_or(LiveConnectionError::NotWired)?;
        let connection = source.open_live(request.clone()).await?;
        let revocation = self.ledger.borrow(&request.handle.db_session_id);
        Ok(LiveConnectionLease {
            db_session_id: Some(request.handle.db_session_id),
            ledger: Arc::clone(&self.ledger),
            revocation,
            connection: Some(connection),
        })
    }

    /// 作废一个会话上的全部在借借用。回收扫描与显式关闭都要走它。
    ///
    /// 返回被作废的借用条数，供观测与测试使用。
    pub fn revoke(&self, db_session_id: &DbSessionId) -> usize {
        let revoked = self.ledger.revoke(db_session_id);
        if revoked > 0 {
            tracing::warn!(
                db_session_id = db_session_id.as_str(),
                borrows = revoked,
                "会话已被回收/关闭而仍有借用在外：引用即刻作废，持有者必须重新走执行路径"
            );
        }
        revoked
    }

    pub fn outstanding(&self) -> usize {
        self.ledger.outstanding()
    }

    pub fn rows(&self) -> usize {
        self.ledger.rows()
    }

    pub fn issued(&self) -> u64 {
        self.ledger.issued()
    }

    pub fn borrowed_sessions(&self) -> Vec<DbSessionId> {
        self.ledger.borrowed_sessions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Never;

    #[async_trait]
    impl LiveConnectionSource for Never {
        async fn open_live(
            &self,
            _request: LiveConnectionRequest,
        ) -> Result<LiveConnection, LiveConnectionError> {
            Err(LiveConnectionError::Refused("never lends".to_string()))
        }
    }

    fn request(id: &str) -> LiveConnectionRequest {
        LiveConnectionRequest {
            handle: SessionHandle {
                db_session_id: DbSessionId::new(id),
                runtime_epoch: crate::connection::Counter::new(1),
            },
            connection_id: ConnectionId::new("c"),
            purpose: LivePurpose::Inspection,
        }
    }

    #[tokio::test]
    async fn 未接线时借出得到配置事实错误且不留登记行() {
        let live = LiveConnections::unwired();
        let err = live
            .borrow(request("s0"))
            .await
            .expect_err("未接线不应借出");
        assert!(matches!(err, LiveConnectionError::NotWired));
        assert_eq!(live.rows(), 0);
        assert_eq!(live.outstanding(), 0);
        assert_eq!(live.issued(), 0);
    }

    #[tokio::test]
    async fn 宿主拒绝时不登记因此不留空行() {
        let live = LiveConnections::wired(Arc::new(Never));
        let err = live.borrow(request("s1")).await.expect_err("拒绝");
        assert!(matches!(err, LiveConnectionError::Refused(_)));
        assert_eq!(live.rows(), 0);
        assert_eq!(live.issued(), 0);
    }

    #[tokio::test]
    async fn 归还恰好一次且幂等() {
        let ledger = LiveConnectionLedger::new();
        let id = DbSessionId::new("s2");
        let revocation = ledger.borrow(&id);
        assert_eq!(ledger.outstanding(), 1);
        ledger.release(&id);
        ledger.release(&id);
        assert_eq!(ledger.outstanding(), 0);
        assert_eq!(ledger.rows(), 0);
        assert!(!revocation.load(Ordering::SeqCst));
        assert_eq!(ledger.issued(), 1);
    }

    #[tokio::test]
    async fn 作废只翻标志不摘行所以计数不泄漏() {
        let ledger = LiveConnectionLedger::new();
        let id = DbSessionId::new("s3");
        let revocation = ledger.borrow(&id);
        let _second = ledger.borrow(&id);
        assert_eq!(ledger.revoke(&id), 2);
        assert!(revocation.load(Ordering::SeqCst));
        assert_eq!(ledger.outstanding(), 2);
        assert_eq!(ledger.rows(), 1);
        ledger.release(&id);
        assert_eq!(ledger.outstanding(), 1);
        ledger.release(&id);
        assert_eq!(ledger.rows(), 0);
        assert_eq!(ledger.borrowed_sessions(), Vec::new());
    }

    #[tokio::test]
    async fn 归还后的借用不可再用() {
        let ledger = Arc::new(LiveConnectionLedger::new());
        let id = DbSessionId::new("s4");
        let revocation = ledger.borrow(&id);
        let lease = LiveConnectionLease {
            db_session_id: Some(id.clone()),
            ledger: Arc::clone(&ledger),
            revocation,
            connection: None,
        };
        let err = lease.connection().expect_err("空借用不可用");
        assert!(matches!(err, LiveConnectionError::SessionLost(_)));
        drop(lease);
        assert_eq!(ledger.outstanding(), 0);
    }
}
