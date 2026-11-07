//! driverType 直取（无连接执行）的 ephemeral 会话注册表（W8-b ②）。
//!
//! 桌面的 driverType 直取给驱动一个**空句柄**（`unbound_handle()`：
//! `{ id: "", pool_id: "" }`——驱动自行 mint 真身）。服务端形态不能照抄：
//! CM-05 要求无连接执行的资源事实**绑定请求主体**（`RequestContext` 的
//! organization/principal），且句柄必须有界（TTL），否则「驱动自建会话」
//! 就成了无归属、无期限的内存面。本注册表就是这个最小生命周期面：
//!
//! * **铸造**（`mint`）：CSPRNG 句柄 id（仿 [`crate::http::sse::
//!   EventStreamRegistry::mint_stream_id`] 的熵形）+ 归属盖章
//!   （organization/principal）+ 单调时刻。表满拒绝（fail-closed，不逐出
//!   ——未消费的句柄不是可牺牲的缓存行，与事件注册表同一条登记口径）。
//! * **消费**（`take`）：一次有效——命中时核对归属（不一致 = `None`，
//!   不区分「不存在」与「不是你的」，防枚举口径与 CM-05 的 404 合并同源）
//!   并即刻删除；执行编排要么用掉它要么丢弃它，不留悬挂句柄。
//! * **TTL**：[`EPHEMERAL_TTL`] = 30 分钟，对齐会话目录的业务空闲期限
//!   （`datazen::execution::application::orchestration::idle` 的
//!   `DEFAULT_DEADLINE`，§9.2「业务空闲 30 分钟」）。过期条目在
//!   `mint`/`take` 时惰性清扫（与事件注册表同一把锁内 prune 的先例）。
//! * **清理语义**：无后台清扫线程——条目只在 `take`（消费即删）与
//!   `mint`（惰性 prune）两个路径上消失；进程退出即全丢（与
//!   `SessionDirectory` 的「纯内存」纪律同一条：禁止磁盘持久化）。
//!
//! 句柄本身是 [`datazen_driver_api::ConnectionHandle`]（驱动的执行入口就吃
//! 这个形状，桌面同一类型）；id 前缀 `eph-` 让驱动侧日志能认出直取面。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use datazen_driver_api::ConnectionHandle;
use rand::RngCore;

use datazen::shared::error::PortError;

/// ephemeral 句柄 TTL：对齐会话目录的业务空闲期限（§9.2，30 分钟）。
pub const EPHEMERAL_TTL: Duration = Duration::from_secs(30 * 60);

/// 注册表容量（未消费的句柄是内存攻击面；与事件注册表同量级的保守上界）。
pub const EPHEMERAL_CAPACITY: usize = 1_024;

#[derive(Debug, Clone)]
struct EphemeralEntry {
    organization_id: String,
    principal_id: String,
    created_at_unix: u64,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<String, EphemeralEntry>,
}

/// driverType 直取的内存 ephemeral 会话注册表。进程内单例（v1 单实例）。
pub struct EphemeralSessions {
    ttl_secs: u64,
    capacity: usize,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for EphemeralSessions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EphemeralSessions")
            .field("ttl_secs", &self.ttl_secs)
            .field("capacity", &self.capacity)
            .field("inner", &"entries")
            .finish()
    }
}

impl EphemeralSessions {
    pub fn new() -> Self {
        Self {
            ttl_secs: EPHEMERAL_TTL.as_secs(),
            capacity: EPHEMERAL_CAPACITY,
            inner: Mutex::new(Inner::default()),
        }
    }

    fn prune(inner: &mut Inner, now: u64, ttl_secs: u64) {
        inner
            .entries
            .retain(|_, entry| now.saturating_sub(entry.created_at_unix) <= ttl_secs);
    }

    /// 铸造一个绑定请求主体的 ephemeral 句柄。表满 = fail-closed 拒绝。
    pub fn mint(
        &self,
        organization_id: &str,
        principal_id: &str,
        now_unix: u64,
    ) -> Result<ConnectionHandle, PortError> {
        let mut inner = self.inner.lock().expect("ephemeral registry lock");
        Self::prune(&mut inner, now_unix, self.ttl_secs);
        if inner.entries.len() >= self.capacity {
            return Err(PortError::QuotaExceeded(
                "ephemeral session registry is full".into(),
            ));
        }
        let id = Self::mint_handle_id();
        inner.entries.insert(
            id.clone(),
            EphemeralEntry {
                organization_id: organization_id.to_string(),
                principal_id: principal_id.to_string(),
                created_at_unix: now_unix,
            },
        );
        Ok(ConnectionHandle {
            id,
            pool_id: String::new(),
        })
    }

    /// 消费一个句柄：归属核对（不一致/不存在/过期统一 `None`，不区分——
    /// 防枚举）+ 一次有效（命中即删）。
    pub fn take(
        &self,
        handle: &ConnectionHandle,
        organization_id: &str,
        principal_id: &str,
        now_unix: u64,
    ) -> Option<()> {
        let mut inner = self.inner.lock().expect("ephemeral registry lock");
        Self::prune(&mut inner, now_unix, self.ttl_secs);
        let entry = inner.entries.remove(&handle.id)?;
        if entry.organization_id != organization_id || entry.principal_id != principal_id {
            // 不是你的句柄：塞回去（条目本身仍归属原主体，不能因为一次
            // 冒领就把它抹掉——抹掉等于给攻击者一个打掉他人句柄的通道）。
            inner.entries.insert(handle.id.clone(), entry);
            return None;
        }
        Some(())
    }

    fn mint_handle_id() -> String {
        let mut bytes = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        use base64::Engine as _;
        format!(
            "eph-{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    #[test]
    fn minted_handles_carry_the_ephemeral_prefix_and_unique_ids() {
        let registry = EphemeralSessions::new();
        let first = registry
            .mint("org-1", "user-1", now())
            .expect("mint under capacity");
        let second = registry
            .mint("org-1", "user-1", now())
            .expect("second mint");
        assert!(first.id.starts_with("eph-"), "{first:?}");
        assert_ne!(first.id, second.id, "CSPRNG 句柄不得重复");
    }

    #[test]
    fn take_is_one_shot_for_the_right_owner() {
        let registry = EphemeralSessions::new();
        let handle = registry.mint("org-1", "user-1", now()).expect("mint");
        assert!(
            registry.take(&handle, "org-1", "user-1", now()).is_some(),
            "归属一致的一次消费成立"
        );
        assert!(
            registry.take(&handle, "org-1", "user-1", now()).is_none(),
            "消费即删：第二次取同一句柄必须落空"
        );
    }

    #[test]
    fn take_refuses_a_mismatched_owner_without_destroying_the_entry() {
        let registry = EphemeralSessions::new();
        let handle = registry.mint("org-1", "user-1", now()).expect("mint");
        // 越组织冒领：拒绝，且不销毁原条目（打掉他人句柄不是能力）。
        assert!(registry.take(&handle, "org-2", "user-1", now()).is_none());
        assert!(
            registry.take(&handle, "org-1", "user-1", now()).is_some(),
            "被冒领后的原句柄仍归原主体消费"
        );
        // 越主体同理。
        let handle = registry.mint("org-1", "user-1", now()).expect("mint");
        assert!(registry.take(&handle, "org-1", "user-9", now()).is_none());
    }

    #[test]
    fn expired_handles_stop_consuming() {
        let registry = EphemeralSessions::new();
        let handle = registry.mint("org-1", "user-1", now()).expect("mint");
        // TTL 30 分钟：把 now 拨过期限再取，必须落空（惰性 prune）。
        let after_ttl = now() + EPHEMERAL_TTL.as_secs() + 1;
        assert!(
            registry
                .take(&handle, "org-1", "user-1", after_ttl)
                .is_none(),
            "过期句柄不可消费"
        );
    }

    #[test]
    fn mint_fails_closed_when_the_registry_is_full() {
        let registry = EphemeralSessions::new();
        let start = now();
        for index in 0..EPHEMERAL_CAPACITY {
            registry
                .mint("org-1", "user-1", start + index as u64)
                .expect("each mint under capacity");
        }
        let error = registry
            .mint("org-1", "user-1", start + EPHEMERAL_CAPACITY as u64)
            .expect_err("full registry must refuse");
        assert!(
            matches!(error, PortError::QuotaExceeded(_)),
            "表满按 QuotaExceeded 拒绝：{error}"
        );
    }
}
