//! SSH known-hosts 的**纯逻辑**：条目形状、指纹、TOFU 判定与错误文案。
//!
//! 这一层是形态中立的：它不知道条目存在哪儿。**存哪儿**是端口
//! [`datazen_platform_api::ports::known_hosts::KnownHostsRepository`] 的事——桌面实现读
//! `{appData}/ssh_known_hosts.json`，将来的 web 形态由服务端 host 实现
//! （服务端存储目录 / 组织级存储）。
//!
//! 换句话说：**共享包不再读本机文件**。

pub use super::domain::KnownHostEntry;
use russh::keys::ssh_key::{HashAlg, PublicKey};

/// 主机密钥判定结果（TOFU：首次使用即信任，之后必须一致）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyDecision {
    AcceptFirstUse { fingerprint: String },
    AcceptMatch,
    RejectMismatch { expected: String, received: String },
}

/// 已知主机的键：`host:port`。
pub fn host_key_id(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

/// 公钥指纹（OpenSSH 的 SHA256 形式）。
pub fn format_public_key_fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// 由观测到的公钥造一条已知主机记录。
pub fn known_host_entry_from_key(key: &PublicKey) -> Result<KnownHostEntry, String> {
    Ok(KnownHostEntry {
        fingerprint: format_public_key_fingerprint(key),
        algorithm: key.key_data().algorithm().as_str().to_string(),
        public_key: key
            .to_openssh()
            .map_err(|e| format!("encode SSH public key: {e}"))?,
    })
}

/// TOFU 判定：没有记录就接受并记下指纹；有记录就必须一致。
pub fn verify_host_key(
    stored: Option<&KnownHostEntry>,
    observed: &KnownHostEntry,
) -> HostKeyDecision {
    match stored {
        None => HostKeyDecision::AcceptFirstUse {
            fingerprint: observed.fingerprint.clone(),
        },
        Some(stored) if stored.fingerprint == observed.fingerprint => HostKeyDecision::AcceptMatch,
        Some(stored) => HostKeyDecision::RejectMismatch {
            expected: stored.fingerprint.clone(),
            received: observed.fingerprint.clone(),
        },
    }
}

/// 主机密钥不匹配时的错误文案。
pub fn mismatch_error_message(host_id: &str, expected: &str, received: &str) -> String {
    format!(
        "SSH host key changed for {host_id} (possible MITM). Expected {expected}, got {received}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_shape_is_the_frozen_on_disk_shape() {
        // 落盘字段名被桌面实现直接复用：改这里等于改 `{appData}/ssh_known_hosts.json`
        // 的格式。字段名是 **snake_case**（`public_key`，不是 `publicKey`）——
        // 这条钉住的就是它。
        let entry = KnownHostEntry {
            fingerprint: "SHA256:abc".into(),
            algorithm: "ssh-ed25519".into(),
            public_key: "ssh-ed25519 AAAA".into(),
        };
        let json = serde_json::to_value(&entry).expect("serialize entry");
        assert_eq!(json["fingerprint"], "SHA256:abc");
        assert_eq!(json["algorithm"], "ssh-ed25519");
        assert_eq!(json["public_key"], "ssh-ed25519 AAAA");
        assert!(
            json.get("publicKey").is_none(),
            "known_hosts.json 的字段名不得改成 camelCase：{json}"
        );
    }
}
