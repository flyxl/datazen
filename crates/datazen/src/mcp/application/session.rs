//! MCP 有状态工具的会话映射语义（consumer-adapters §4.2；M2 归位共享层）。
//!
//! 有状态工具不猜目标：客户端显式 open / use / close，服务端对客户端只回一个
//! 不透明的 clientSessionRef。
//!
//! reference 到活会话句柄的映射只存在于 MCP 适配层内存，不进 Workflow 定义、
//! 执行历史，也不落数据库配置。每条映射绑定主体、后端生成的 clientInstanceId、
//! 活句柄、建立时的 contextVersion 与 TTL。
//!
//! 映射丢失后 use 返回 SessionLost，绝不用同一个 reference 重建：重建等于让
//! 上一个客户端的句柄指向下一个客户端的会话。
//!
//! 归属校验、TTL 归一化与过期/清理登记（`resourceRelease=Confirmed` 才删
//! 记录）、清理的退避与清扫判定（M2 主体归位）是共享语义；worker 的
//! CAS/spawn/select 循环与传输生命周期驱动留宿主。

use crate::execution::application::requests::{CloseMode, CloseSessionRequest};
use crate::execution::application::RuntimeConnectionUseCases;
// 清扫经 `ConnectionUseCases`（close_session 是该 trait 的方法）。
use crate::execution::application::use_cases::ConnectionUseCases;
use crate::execution::ResourceRelease;
use crate::execution::SessionHandle;
use crate::shared::context::RequestContext;
use crate::shared::id::Counter;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// 有状态映射的默认存活时长。
pub const DEFAULT_TTL: Duration = Duration::from_secs(300);
/// 存活时长上限：客户端可以要短，不能突破上限要长。
pub const MAX_TTL: Duration = Duration::from_secs(3600);

/// 归一化客户端请求的 TTL。
pub fn sanitize_ttl(requested: Option<Duration>) -> Duration {
    match requested {
        None => DEFAULT_TTL,
        Some(ttl) if ttl >= MAX_TTL => MAX_TTL,
        Some(ttl) if ttl.is_zero() => DEFAULT_TTL,
        Some(ttl) => ttl,
    }
}

/// clientSessionRef 不可用的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLostReason {
    /// 没有这条映射：从未打开、已关闭，或已过期回收。
    Unknown,
    /// 映射存在，但属于另一个客户端实例。
    ///
    /// 内部可区分，对客户端不可区分：调用方不持有的 reference 一律按不存在处理。
    ForeignOwner,
}

impl SessionLostReason {
    /// 唯一的对外文案：不泄漏这个 reference 是否曾经存在。
    pub const CLIENT_MESSAGE: &'static str = "SessionLost: the client session reference is no longer usable (closed, expired, or held by another client). Open a new session with open_session; do not reuse this reference";

    /// 对外文案入口。两种原因给同一条消息：调用方不持有的引用一律按不存在处理。
    pub fn client_message(self) -> &'static str {
        Self::CLIENT_MESSAGE
    }
}

/// 一条活的有状态映射。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBinding {
    /// 对客户端不透明的引用。
    pub reference: String,
    /// 后端生成的 MCP clientInstanceId（§4.1：不由客户端提供）。
    pub client_instance_id: String,
    pub connection_id: String,
    pub handle: SessionHandle,
    /// 建立时的 contextVersion；执行时用作 CAS，冲突走 ContextConflict。
    pub context_revision: Counter,
    pub expires_at: Instant,
}

impl SessionBinding {
    fn is_expired_at(&self, now: Instant) -> bool {
        now >= self.expires_at
    }
}

/// MCP 有状态会话映射表（仅内存）。
#[derive(Default)]
pub struct McpSessionRegistry {
    entries: Mutex<HashMap<String, SessionBinding>>,
    cleanup: Mutex<HashMap<String, SessionBinding>>,
    pub disconnected: std::sync::atomic::AtomicBool,
    pub worker: std::sync::atomic::AtomicBool,
    pub wake: tokio::sync::Notify,
}

impl McpSessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<String, SessionBinding>> {
        // 锁中毒只说明别处 panic 时持过锁；映射本身是普通数据，恢复守卫是安全的。
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 登记一条映射。reference 由后端生成，不含 connectionId 等可推断信息。
    pub fn open(
        &self,
        client_instance_id: &str,
        connection_id: &str,
        handle: SessionHandle,
        context_revision: Counter,
        ttl: Duration,
        now: Instant,
    ) -> SessionBinding {
        let binding = SessionBinding {
            reference: uuid::Uuid::new_v4().to_string(),
            client_instance_id: client_instance_id.to_owned(),
            connection_id: connection_id.to_owned(),
            handle,
            context_revision,
            expires_at: now + sanitize_ttl(Some(ttl)),
        };
        self.entries()
            .insert(binding.reference.clone(), binding.clone());
        binding
    }

    /// 取用映射：先校验归属，再校验存活。
    ///
    /// 归属不符时不删对方映射：外来客户端既拿不到，也不能顺手回收别人的会话。
    pub fn lookup(
        &self,
        reference: &str,
        client_instance_id: &str,
        now: Instant,
    ) -> Result<SessionBinding, SessionLostReason> {
        let mut entries = self.entries();
        let Some(binding) = entries.get(reference) else {
            return Err(SessionLostReason::Unknown);
        };
        if binding.client_instance_id != client_instance_id {
            return Err(SessionLostReason::ForeignOwner);
        }
        if binding.is_expired_at(now) {
            if let Some(expired) = entries.remove(reference) {
                self.retain_cleanup(expired);
            }
            return Err(SessionLostReason::Unknown);
        }
        Ok(binding.clone())
    }

    /// 关闭一条映射。幂等：已关闭或不存在返回 None，且不碰他人的映射。
    pub fn close(&self, reference: &str, client_instance_id: &str) -> Option<SessionBinding> {
        let mut entries = self.entries();
        if entries.get(reference)?.client_instance_id != client_instance_id {
            return None;
        }
        let closed = entries.remove(reference);
        if closed.is_some() {
            // A TTL worker may be sleeping until this mapping's original expiry.
            // Wake it so an empty registry does not retain AppState for that TTL.
            self.wake.notify_one();
        }
        closed
    }

    pub fn revoke(&self, reference: &str, client: &str) {
        if let Some(binding) = self.close(reference, client) {
            self.retain_cleanup(binding);
        }
    }

    /// 回收某个客户端实例的全部映射。
    ///
    /// 只按 clientInstanceId 过滤，不按 connectionId：同一个连接上别的消费者的
    /// 会话不属于本客户端，断连清理不得连坐。
    pub fn close_client(&self, client_instance_id: &str) -> Vec<SessionBinding> {
        let mut entries = self.entries();
        let owned: Vec<String> = entries
            .iter()
            .filter(|(_, binding)| binding.client_instance_id == client_instance_id)
            .map(|(reference, _)| reference.clone())
            .collect();
        let bindings: Vec<_> = owned
            .iter()
            .filter_map(|reference| entries.remove(reference))
            .collect();
        for binding in &bindings {
            self.retain_cleanup(binding.clone());
        }
        bindings
    }

    /// 撤销已过期映射并保留 cleanup owner；返回值仅供观测，忽略它不会丢失句柄。
    pub fn purge_expired(&self, now: Instant) -> Vec<SessionBinding> {
        let mut entries = self.entries();
        let expired: Vec<String> = entries
            .iter()
            .filter(|(_, binding)| binding.is_expired_at(now))
            .map(|(reference, _)| reference.clone())
            .collect();
        let bindings: Vec<_> = expired
            .iter()
            .filter_map(|reference| entries.remove(reference))
            .collect();
        for binding in &bindings {
            self.retain_cleanup(binding.clone());
        }
        bindings
    }

    fn retain_cleanup(&self, binding: SessionBinding) {
        self.cleanup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(binding.reference.clone(), binding);
        self.wake.notify_one();
    }

    pub fn pending_cleanup(&self) -> Vec<SessionBinding> {
        self.cleanup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    pub fn confirm_cleanup(&self, reference: &str) {
        self.cleanup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(reference);
    }

    pub fn next_expiry(&self) -> Option<Instant> {
        self.entries()
            .values()
            .map(|binding| binding.expires_at)
            .min()
    }

    pub fn len(&self) -> usize {
        self.entries().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 清理 worker 的调度语义（纯计算，无 I/O）：退避步长、重试时刻与一轮
/// tick 的睡眠时长判定。worker 的 CAS/spawn/唤醒 select 循环是传输驱动
/// 层，留宿主；宿主每轮向本结构问三个问题：现在能扫吗、罚多重、睡多久。
pub struct CleanupScheduler {
    backoff: Duration,
    next_retry: Instant,
}

impl CleanupScheduler {
    /// 初始退避步长。
    pub const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
    /// 退避上限。
    pub const MAX_BACKOFF: Duration = Duration::from_secs(30);
    /// 无待清理登记时的空闲轮询间隔。
    pub const IDLE_POLL: Duration = Duration::from_secs(3600);

    /// worker 启动：第一轮清扫立即放行，退避从初始步长起。
    pub fn begin(now: Instant) -> Self {
        Self {
            backoff: Self::INITIAL_BACKOFF,
            next_retry: now,
        }
    }

    /// 现在是否到了允许清扫的时刻。
    pub fn ready(&self, now: Instant) -> bool {
        now >= self.next_retry
    }

    /// 清扫后仍有待清理登记：安排下一次重试并放大退避（指数翻倍，
    /// 封顶 [`CleanupScheduler::MAX_BACKOFF`]）。失败句柄不能被忙轮询
    /// 烧穿，也不能被无限退避搁置。
    pub fn penalize(&mut self, now: Instant) {
        self.next_retry = now + self.backoff;
        self.backoff = (self.backoff * 2).min(Self::MAX_BACKOFF);
    }

    /// 本轮睡眠时长；无 pending 时退避回到初始步长。有 pending 睡到下次
    /// 重试，无 pending 用空闲轮询；最近过期时间早于两者时被截断——
    /// TTL 到点是登记事件，不能睡过头。
    pub fn next_delay(
        &mut self,
        now: Instant,
        has_pending: bool,
        next_expiry: Option<Instant>,
    ) -> Duration {
        if !has_pending {
            self.backoff = Self::INITIAL_BACKOFF;
        }
        let delay = if has_pending {
            self.next_retry.saturating_duration_since(now)
        } else {
            Self::IDLE_POLL
        };
        next_expiry
            .map(|at| at.saturating_duration_since(now).min(delay))
            .unwrap_or(delay)
    }
}

/// 本轮该由这个客户端清理的登记：只按 clientInstanceId 过滤，不按
/// connectionId——同一个连接上别的消费者的登记不属于本客户端。
fn own_pending(pending: Vec<SessionBinding>, client_instance_id: &str) -> Vec<SessionBinding> {
    pending
        .into_iter()
        .filter(|binding| binding.client_instance_id == client_instance_id)
        .collect()
}

/// 一轮清扫：把本客户端登记的待清理句柄 RollbackAndClose。Runtime 在原
/// 资源上回滚已登记句柄并物理关闭；未知效果保持未知。
/// `resourceRelease=Confirmed` 才撤销登记；失败/Pending/Quarantined 保留
/// cleanup owner。这里不记 SQL、参数、凭据或驱动错误文本。
/// 非本客户端的登记原样保留，等它自己的 worker。
pub async fn sweep_pending(
    runtime: &RuntimeConnectionUseCases,
    context: &RequestContext,
    registry: &McpSessionRegistry,
    client_instance_id: &str,
    pending: Vec<SessionBinding>,
) {
    for binding in own_pending(pending, client_instance_id) {
        let result = runtime
            .close_session(
                context,
                CloseSessionRequest::new(binding.handle, CloseMode::RollbackAndClose),
            )
            .await;
        if matches!(result, Ok(receipt) if receipt.resource_release == ResourceRelease::Confirmed) {
            registry.confirm_cleanup(&binding.reference);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::id::{DbSessionId, RuntimeEpoch};

    fn handle(id: &str) -> SessionHandle {
        SessionHandle::new(DbSessionId::new(id), RuntimeEpoch::new("1"))
    }

    fn open_for(
        registry: &McpSessionRegistry,
        client: &str,
        connection: &str,
        session: &str,
        now: Instant,
    ) -> SessionBinding {
        registry.open(
            client,
            connection,
            handle(session),
            Counter::new(3),
            Duration::from_secs(60),
            now,
        )
    }

    #[test]
    fn a_mapping_opened_by_one_client_is_invisible_to_another() {
        let now = Instant::now();
        let registry = McpSessionRegistry::new();
        let binding = open_for(&registry, "mcp-a", "conn-1", "sess-1", now);

        assert_eq!(
            registry.lookup(&binding.reference, "mcp-b", now),
            Err(SessionLostReason::ForeignOwner)
        );
        // 外来客户端也不能顺手关掉别人的映射。
        assert!(registry.close(&binding.reference, "mcp-b").is_none());
        assert!(registry.lookup(&binding.reference, "mcp-a", now).is_ok());
    }

    #[test]
    fn the_client_sees_one_message_whether_the_mapping_is_gone_or_foreign() {
        assert_eq!(
            SessionLostReason::Unknown.client_message(),
            SessionLostReason::ForeignOwner.client_message()
        );
    }

    #[test]
    fn a_reference_is_never_rebuilt_after_its_mapping_is_gone() {
        let now = Instant::now();
        let registry = McpSessionRegistry::new();
        let first = open_for(&registry, "mcp-a", "conn-1", "sess-1", now);
        assert!(registry.close(&first.reference, "mcp-a").is_some());

        // 关闭后同一个 reference 对所有人都是丢失，谁都不重建。
        assert_eq!(
            registry.lookup(&first.reference, "mcp-a", now),
            Err(SessionLostReason::Unknown)
        );
        let second = open_for(&registry, "mcp-a", "conn-1", "sess-2", now);
        assert_ne!(first.reference, second.reference);
        assert_eq!(
            registry.lookup(&first.reference, "mcp-a", now),
            Err(SessionLostReason::Unknown)
        );
    }

    #[test]
    fn close_is_idempotent() {
        let now = Instant::now();
        let registry = McpSessionRegistry::new();
        let binding = open_for(&registry, "mcp-a", "conn-1", "sess-1", now);

        assert!(registry.close(&binding.reference, "mcp-a").is_some());
        assert!(registry.close(&binding.reference, "mcp-a").is_none());
        assert!(registry.is_empty());
    }

    #[test]
    fn an_expired_mapping_is_reclaimed_and_never_reused() {
        let now = Instant::now();
        let registry = McpSessionRegistry::new();
        let binding = registry.open(
            "mcp-a",
            "conn-1",
            handle("sess-1"),
            Counter::new(1),
            Duration::from_secs(30),
            now,
        );
        let after = now + Duration::from_secs(30);

        assert_eq!(
            registry.lookup(&binding.reference, "mcp-a", after),
            Err(SessionLostReason::Unknown)
        );
        assert!(registry.is_empty());
        // lookup 已撤销访问，但清理owner仍保留，purge不重复排队。
        assert_eq!(registry.pending_cleanup(), vec![binding]);
        assert!(registry.purge_expired(after).is_empty());
    }

    #[test]
    fn purging_expired_returns_the_handles_that_still_need_closing() {
        let now = Instant::now();
        let registry = McpSessionRegistry::new();
        let short = registry.open(
            "mcp-a",
            "conn-1",
            handle("sess-short"),
            Counter::new(1),
            Duration::from_secs(10),
            now,
        );
        let long = registry.open(
            "mcp-a",
            "conn-1",
            handle("sess-long"),
            Counter::new(1),
            Duration::from_secs(600),
            now,
        );

        let reclaimed = registry.purge_expired(now + Duration::from_secs(60));
        assert_eq!(reclaimed.len(), 1);
        assert_eq!(reclaimed[0].reference, short.reference);
        assert_eq!(registry.len(), 1);
        assert!(registry.lookup(&long.reference, "mcp-a", now).is_ok());
    }

    #[test]
    fn disconnect_reclaims_only_that_clients_mappings() {
        let now = Instant::now();
        let registry = McpSessionRegistry::new();
        let a1 = open_for(&registry, "mcp-a", "conn-1", "sess-a1", now);
        let a2 = open_for(&registry, "mcp-a", "conn-2", "sess-a2", now);
        let b1 = open_for(&registry, "mcp-b", "conn-1", "sess-b1", now);

        let mut refs: Vec<String> = registry
            .close_client("mcp-a")
            .iter()
            .map(|binding| binding.reference.clone())
            .collect();
        refs.sort();
        let mut expected = vec![a1.reference.clone(), a2.reference.clone()];
        expected.sort();
        assert_eq!(refs, expected);

        // 同一个 connectionId，不同消费者：必须存活。
        assert!(registry.lookup(&b1.reference, "mcp-b", now).is_ok());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn the_requested_lifetime_is_clamped_to_the_supported_range() {
        assert_eq!(sanitize_ttl(None), DEFAULT_TTL);
        assert_eq!(sanitize_ttl(Some(Duration::ZERO)), DEFAULT_TTL);
        assert_eq!(
            sanitize_ttl(Some(Duration::from_secs(90))),
            Duration::from_secs(90)
        );
        assert_eq!(
            sanitize_ttl(Some(MAX_TTL + Duration::from_secs(1))),
            MAX_TTL
        );
    }

    #[test]
    fn the_reference_does_not_leak_the_connection_it_belongs_to() {
        let now = Instant::now();
        let registry = McpSessionRegistry::new();
        let binding = open_for(&registry, "mcp-a", "production-postgres", "sess-1", now);

        assert!(!binding.reference.contains("production-postgres"));
        assert!(!binding.reference.contains("sess-1"));
        assert!(!binding.reference.contains("mcp-a"));
    }

    #[test]
    fn the_cleanup_backoff_doubles_up_to_the_cap_and_resets_when_idle() {
        let t = Instant::now();
        let mut scheduler = CleanupScheduler::begin(t);
        assert!(scheduler.ready(t), "the first sweep is immediately allowed");

        // 第一步惩罚：100ms 后才允许下一次清扫。
        scheduler.penalize(t);
        assert!(!scheduler.ready(t + Duration::from_millis(99)));
        assert!(scheduler.ready(t + Duration::from_millis(100)));
        assert_eq!(
            scheduler.next_delay(t, true, None),
            Duration::from_millis(100)
        );

        // 第二步：翻倍到 200ms。
        let t2 = t + Duration::from_millis(100);
        scheduler.penalize(t2);
        assert_eq!(
            scheduler.next_delay(t2, true, None),
            Duration::from_millis(200)
        );

        // 反复惩罚步长封顶 30s，不会无限翻倍。
        let mut at = t2 + Duration::from_millis(200);
        for _ in 0..12 {
            scheduler.penalize(at);
            at += Duration::from_secs(31);
        }
        // 12 轮翻倍早已触顶；delay 问的是「距下次重试还剩多久」，此刻
        // 必须在惩罚的当下问——刚安排的重试间隔就是 30s 上限。若在
        // 无限翻倍，这里会是 200ms·2¹²。
        scheduler.penalize(at);
        assert_eq!(
            scheduler.next_delay(at, true, None),
            CleanupScheduler::MAX_BACKOFF
        );

        // 无 pending：退避回到初始步长，空闲轮询用长间隔。
        assert_eq!(
            scheduler.next_delay(at, false, None),
            CleanupScheduler::IDLE_POLL
        );
        scheduler.penalize(at);
        assert_eq!(
            scheduler.next_delay(at, true, None),
            CleanupScheduler::INITIAL_BACKOFF
        );

        // 最近过期时间早于两者时截断睡眠：TTL 到点是登记事件，不能睡过头。
        assert_eq!(
            scheduler.next_delay(at, false, Some(at + Duration::from_secs(5))),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn a_cleanup_sweep_only_takes_this_clients_registrations() {
        let now = Instant::now();
        let registry = McpSessionRegistry::new();
        let mine = open_for(&registry, "mcp-a", "conn-1", "sess-a", now);
        let theirs = open_for(&registry, "mcp-b", "conn-1", "sess-b", now);
        registry.revoke(&mine.reference, "mcp-a");
        registry.revoke(&theirs.reference, "mcp-b");

        assert_eq!(own_pending(registry.pending_cleanup(), "mcp-a"), vec![mine]);
        // 他人的登记仍在等它自己的 worker；扫别人的队列不会动它。
        assert_eq!(
            own_pending(registry.pending_cleanup(), "mcp-b"),
            vec![theirs]
        );
        assert_eq!(registry.pending_cleanup().len(), 2);
    }
}
