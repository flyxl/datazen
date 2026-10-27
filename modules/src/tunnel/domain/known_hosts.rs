//! `KnownHostsRepository`：SSH known-hosts 的**存储端口**。
//!
//! # 为什么在契约包，而不是隧道的机制包
//!
//! 这个端口原先住在 `modules/src/tunnel`（与「隧道机制」同处一包），理由是「端口住在需要它的
//! 那个包里」。它在这个位置有两个代价，都是在 `ddd-refactor-plan` 的 R3（端口只在内层）
//! 下暴露出来的：
//!
//! * **端口族从此不可清单化**：本包的 [`inventory::PORT_TRAITS`](super::inventory::PORT_TRAITS)
//!   只收 `src/ports/` 下的 `pub trait`，住在领域包里的端口既不进清单也不进 §6.4 表，
//!   于是「有哪些端口、谁实现了它」这件事无法机器核对（`ddd-refactor-plan` 附录 A.5
//!   统计的「无外部实现的端口 12/24」也因此只覆盖本包）。
//! * **词汇表随机制走**：`KnownHostEntry` 是这条端口签名里的类型，却被定义在机制包里。
//!   宿主实现端口就必须同时依赖机制包——而宿主依赖机制包的理由本该是「要用隧道机制」，
//!   不是「要填一个端口」。
//!
//! 搬家后 `modules/src/tunnel` 仍是机制与**纯逻辑**（指纹、TOFU 判定、错误文案）的家，
//! 只是把「存哪儿」的契约交出来，与本包其它端口同住（规模由 [`inventory`] 派生，不在任何模块头手写）。
//!
//! # 这条端口与其它端口的两处形状差异（都是刻意的）
//!
//! * **同步**：SSH 主机密钥校验发生在协议回调里（`russh` 的 `check_server_key`），现状就是
//!   `std::fs`；把它改成异步只会让协议回调变形，不会让它更安全。
//! * **`load` 不失败**：读不到（文件缺失 / 损坏 / 不可读）一律按**空集合**处理。TOFU 的
//!   语义就是「没有记录 ⇒ 首次使用即信任」，一份坏文件不得阻断隧道启动。实现负责把这件事
//!   记进日志（桌面实现记的是既有那两条 warn 文案）。
//!
//! 反过来，[`KnownHostsRepository::save`] 失败**必须**如实报错：它意味着「这台主机的指纹
//! 没能记下来」，下一次连接会被当成首次使用。失败一律 `PortError::BackendUnavailable`——
//! 这条端口不表达业务拒绝（越权在 known-hosts 这层根本不存在：条目按 `host:port` 键）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::shared::error::PortError;

/// 一条已知主机记录：指纹、算法与 OpenSSH 公钥文本。
///
/// 落盘形状（字段名与 JSON 结构）由**实现方**决定，桌面实现沿用
/// `{appData}/ssh_known_hosts.json` 的既有格式。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnownHostEntry {
    pub fingerprint: String,
    pub algorithm: String,
    pub public_key: String,
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
    fn save(&self, entries: &HashMap<String, KnownHostEntry>) -> Result<(), PortError>;
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// 形态中立、无副作用、无传输的进程内原语：内存实现随端口一起提供
    /// （先例：平台文档 §4 的 `execution::application::budget::pool_ledger`）。
    #[derive(Default)]
    struct MemoryKnownHosts {
        entries: Mutex<HashMap<String, KnownHostEntry>>,
    }

    impl KnownHostsRepository for MemoryKnownHosts {
        fn load(&self) -> HashMap<String, KnownHostEntry> {
            self.entries
                .lock()
                .map(|entries| entries.clone())
                .unwrap_or_default()
        }

        fn save(&self, entries: &HashMap<String, KnownHostEntry>) -> Result<(), PortError> {
            let mut guard = self
                .entries
                .lock()
                .map_err(|_| PortError::BackendUnavailable("known hosts lock poisoned".into()))?;
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

    /// 端口的最小契约：写进去能读出来；空仓储读出空集合（`load` 不失败）。
    #[test]
    fn the_port_round_trips_entries_and_loads_empty_by_default() {
        let store = MemoryKnownHosts::default();
        assert!(
            store.load().is_empty(),
            "空仓储必须读出空集合，而不是 panic"
        );

        let mut entries = HashMap::new();
        entries.insert("example.com:22".to_string(), entry());
        store.save(&entries).expect("保存必须成功");
        assert_eq!(store.load(), entries);
    }
}
