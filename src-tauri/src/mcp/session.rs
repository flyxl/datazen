//! MCP 有状态工具的会话协议（consumer-adapters §4.2）。
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

use datazen::execution::SessionHandle;
use datazen::shared::id::Counter;
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
    pub(crate) disconnected: std::sync::atomic::AtomicBool,
    pub(crate) worker: std::sync::atomic::AtomicBool,
    pub(crate) wake: tokio::sync::Notify,
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

    pub(crate) fn revoke(&self, reference: &str, client: &str) {
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

    pub(crate) fn pending_cleanup(&self) -> Vec<SessionBinding> {
        self.cleanup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    pub(crate) fn confirm_cleanup(&self, reference: &str) {
        self.cleanup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(reference);
    }

    pub(crate) fn next_expiry(&self) -> Option<Instant> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::shared::id::{DbSessionId, RuntimeEpoch};

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
}
