//! `KnownHostsRepository`：SSH known-hosts 的**存储端口**。
//!
//! # 为什么是端口，而不是共享包里的一个函数
//!
//! 隧道机制是形态中立的共享件，但**已知主机存在哪儿**是形态的事：桌面读
//! `{appData}/ssh_known_hosts.json`，将来的 web 形态由服务端 host 实现（服务端
//! 存储目录 / 组织级存储）。把「读本机文件」留在共享包里，等于让 web 形态也背
//! 一个它填不起的存储形态——所以文件 I/O 抽成端口，**纯逻辑**（指纹、TOFU 判定、
//! 错误文案）留在 [`crate::ssh_known_hosts`]。
//!
//! # 这是一个**领域包端口**，不在 platform-api 的端口家族里
//!
//! 先例：`schema-diff` 的 `ReviewedPlanRepository`、`data-transfer` 的
//! `TransferPlanRepositoryPort`、`data-sync` 的 `ComparisonStorageRoot`——端口住在**需要
//! 它的那个包**里，词汇表也跟着住在一起。
//!
//! 与 platform-api 家族的差别只有两处，都是这份端口的形状决定的：
//!
//! * **同步**：SSH 主机密钥校验发生在协议回调里（`russh` 的
//!   `check_server_key`），现状就是 `std::fs`；把它改成异步只会让协议回调变形，
//!   不会让它更安全。
//! * **自带错误类型**：失败只有一个含义——「存不下」。它不表达业务拒绝，也不
//!   表达越权（越权在 known-hosts 这一层根本不存在：条目按 `host:port` 键）。
//!
//! # 降级语义属于契约，不属于实现
//!
//! [`KnownHostsRepository::load`] **不失败**：读不到（文件缺失 / 损坏 / 不可读）一律
//! 按**空集合**处理。TOFU 的语义就是「没有记录 ⇒ 首次使用即信任」，一份坏文件
//! 不得阻断隧道启动。实现负责把这件事**记进日志**（桌面实现记的是既有那两条
//! warn 文案）。
//!
//! 反过来，[`KnownHostsRepository::save`] 失败**必须**如实报错：它意味着「这台主机的
//! 指纹没能记下来」，下一次连接会被当成首次使用。

use std::collections::HashMap;

use crate::ssh_known_hosts::KnownHostEntry;

/// known-hosts 存储不可用。
///
/// 只有一种含义：**存不下**。文案由实现给出，调用方原样记进日志。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KnownHostsRepositoryError {
    #[error("{0}")]
    Unavailable(String),
}

/// SSH known-hosts 的存储端口。
///
/// 实现必须能安全共享：宿主在组装期创建**一个**实例并注入，SSH 隧道在每次建连
/// 时读它、在首次接受主机密钥时写它。
pub trait KnownHostsRepository: Send + Sync + 'static {
    /// 读出全部条目（键是 `host:port`）。
    ///
    /// **不失败**：读不到一律按空集合返回（见模块头的「降级语义属于契约」）。
    fn load(&self) -> HashMap<String, KnownHostEntry>;

    /// 保存全部条目。
    fn save(
        &self,
        entries: &HashMap<String, KnownHostEntry>,
    ) -> Result<(), KnownHostsRepositoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 形态中立、无副作用、无传输的进程内原语：内存实现随端口一起提供
    /// （先例：平台文档 §4 的 `ports::budget::pool_ledger`）。
    #[derive(Default)]
    struct MemoryKnownHosts {
        entries: std::sync::Mutex<HashMap<String, KnownHostEntry>>,
    }

    impl KnownHostsRepository for MemoryKnownHosts {
        fn load(&self) -> HashMap<String, KnownHostEntry> {
            self.entries
                .lock()
                .map(|entries| entries.clone())
                .unwrap_or_default()
        }

        fn save(
            &self,
            entries: &HashMap<String, KnownHostEntry>,
        ) -> Result<(), KnownHostsRepositoryError> {
            let mut guard = self.entries.lock().map_err(|_| {
                KnownHostsRepositoryError::Unavailable("known hosts lock poisoned".into())
            })?;
            *guard = entries.clone();
            Ok(())
        }
    }

    fn entry() -> KnownHostEntry {
        KnownHostEntry {
            fingerprint: "SHA256:abc".into(),
            algorithm: "ssh-ed25519".into(),
            public_key: "ssh-ed25519 AAAA".into(),
        }
    }

    #[test]
    fn the_roundtrip_goes_through_the_port_not_through_a_file() {
        let store = MemoryKnownHosts::default();
        assert!(store.load().is_empty());

        let mut map = HashMap::new();
        map.insert("host.example:22".to_string(), entry());
        store.save(&map).expect("save known hosts");

        let loaded = store.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get("host.example:22"), Some(&entry()));
    }

    #[test]
    fn the_store_error_carries_the_hosts_own_message() {
        let error = KnownHostsRepositoryError::Unavailable("write /tmp/x.json: boom".into());
        assert_eq!(error.to_string(), "write /tmp/x.json: boom");
    }
}
