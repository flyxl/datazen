//! 桌面形态的 SSH known-hosts 存储：读 / 写 `{appData}/ssh_known_hosts.json`。
//!
//! # 这一层为什么在宿主
//!
//! 共享 crate `packages/tunnel` 只留**规则**（指纹、TOFU 判定、错误文案）与
//! **存储端口** [`KnownHostsRepository`]；**文件 I/O** 是形态的事，因此住在宿主：
//! 桌面读 `{appData}/ssh_known_hosts.json`，将来的 web 形态由服务端 host 实现
//! （服务端存储目录 / 组织级存储）。
//!
//! # 行为逐字不变
//!
//! 本文件是端口化之前 `packages/tunnel/src/ssh_known_hosts.rs` 里那两段
//! `std::fs` 逻辑的**原样搬迁**：同样的文件、同样的 JSON 形状（`to_string_pretty`
//! 的对象）、同样的两条 warn 文案、同样的三条错误文案。共享包那边只把
//! 「读本机文件」这件事交了出来。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use datazen_tunnel::{KnownHostEntry, KnownHostsRepository, KnownHostsRepositoryError};

/// 桌面形态的 known-hosts 端口实例（`{appData}/ssh_known_hosts.json`）。
///
/// 组装根用它创建那**一份**实例，再注入会话后端与 `AppState`。
pub fn desktop_known_hosts_repository(data_dir: &Path) -> Arc<dyn KnownHostsRepository> {
    Arc::new(DesktopKnownHostsRepository::new(
        data_dir.join("ssh_known_hosts.json"),
    ))
}

/// 桌面形态的 known-hosts 存储。
///
/// 实例由宿主在**组装期**创建一次并注入（不新增进程级全局）：隧道机制在每次
/// 建连时读它，在首次接受主机密钥时写它。
pub struct DesktopKnownHostsRepository {
    path: PathBuf,
}

impl DesktopKnownHostsRepository {
    /// 指向 `{appData}/ssh_known_hosts.json`。
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 已知主机文件的位置（诊断与测试用）。
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl KnownHostsRepository for DesktopKnownHostsRepository {
    /// 读出全部条目。
    ///
    /// **读不到一律按空集合处理**：文件缺失不记日志（那是「还没记过」），文件损坏
    /// 与读失败各记一条 warn（既有文案逐字保留）。TOFU 的语义就是「没有记录 ⇒
    /// 首次使用即信任」，一份坏文件不得阻断隧道启动。
    fn load(&self) -> HashMap<String, KnownHostEntry> {
        let path = &self.path;
        match std::fs::read_to_string(path) {
            Ok(raw) => match serde_json::from_str(&raw) {
                Ok(map) => map,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Ignoring corrupt ssh_known_hosts.json");
                    HashMap::new()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to read ssh_known_hosts.json");
                HashMap::new()
            }
        }
    }

    /// 保存全部条目。
    ///
    /// 错误文案逐字保留（`create known hosts dir …` / `serialize ssh_known_hosts.json: …`
    /// / `write …`），调用方把它们原样记进日志。
    fn save(
        &self,
        entries: &HashMap<String, KnownHostEntry>,
    ) -> Result<(), KnownHostsRepositoryError> {
        let path = &self.path;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                KnownHostsRepositoryError::Unavailable(format!(
                    "create known hosts dir {}: {e}",
                    parent.display()
                ))
            })?;
        }
        let json = serde_json::to_string_pretty(entries).map_err(|e| {
            KnownHostsRepositoryError::Unavailable(format!("serialize ssh_known_hosts.json: {e}"))
        })?;
        std::fs::write(path, json).map_err(|e| {
            KnownHostsRepositoryError::Unavailable(format!("write {}: {e}", path.display()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> KnownHostEntry {
        KnownHostEntry {
            fingerprint: "SHA256:abc".into(),
            algorithm: "ssh-ed25519".into(),
            public_key: "ssh-ed25519 AAAA".into(),
        }
    }

    #[test]
    fn the_roundtrip_keeps_the_frozen_on_disk_shape() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ssh_known_hosts.json");
        let store = DesktopKnownHostsRepository::new(path.clone());

        assert!(store.load().is_empty(), "a missing file reads as empty");

        let mut map = HashMap::new();
        map.insert("host.example:22".to_string(), entry());
        store.save(&map).expect("save known hosts");

        // 落盘形状：`to_string_pretty` 的对象，字段名 snake_case。
        let raw = std::fs::read_to_string(&path).expect("read back");
        assert!(
            raw.contains("\"public_key\""),
            "字段名不得变成 camelCase: {raw}"
        );
        assert!(raw.contains("\"host.example:22\""), "键是 host:port: {raw}");

        let loaded = store.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get("host.example:22"), Some(&entry()));
    }

    #[test]
    fn a_missing_file_reads_as_empty_without_a_warning() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DesktopKnownHostsRepository::new(dir.path().join("missing.json"));
        assert!(store.load().is_empty());
    }

    #[test]
    fn a_corrupt_file_reads_as_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ssh_known_hosts.json");
        std::fs::write(&path, "{ not json").expect("write invalid json");
        let store = DesktopKnownHostsRepository::new(path);
        assert!(store.load().is_empty());
    }

    #[test]
    fn the_save_error_keeps_the_hosts_own_wording() {
        let dir = tempfile::tempdir().expect("tempdir");
        // 目录本身当文件写：写入必然失败，错误文案必须仍是「write <path>: …」。
        let path = dir.path().join("ssh_known_hosts.json");
        std::fs::create_dir_all(&path).expect("create dir in place of the file");
        let store = DesktopKnownHostsRepository::new(path.clone());
        let error = store.save(&HashMap::new()).expect_err("write must fail");
        assert!(
            error
                .to_string()
                .starts_with(&format!("write {}: ", path.display())),
            "unexpected error text: {error}"
        );
    }
}
