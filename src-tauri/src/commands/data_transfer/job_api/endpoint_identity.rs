//! 数据迁移 Job 端点的真实身份：端点重叠检测与 §6.2 原子预算预留共用同一身份。
//!
//! # 为什么这里不能是常量
//!
//! 端点重叠检测要回答的问题是「读端和写端是不是同一台物理服务器」。这只能由**用户
//! 实际选中的连接配置**回答：迁移 A 库的 `public.users` 到 B 库的 `public.users` 是
//! 本功能最常见的场景，源端与目标端除了表名之外没有任何共同点，只有一个写死的常量
//! 把它们焊死成「同一服务」，于是同名跨库拷贝在 Job 路径上被整体拒绝——相对 legacy
//! `execute_data_transfer` 是功能回退。把常量拆成 `"data-transfer:source"` /
//! `"data-transfer:target"` 两个固定字符串同样不成立：不同 Job 访问不同物理端点时会
//! 共用同一个键，原子预留于是要么误冲突、要么漏冲突。
//!
//! 因此每个端点交出**两类**身份证据（见 [`EndpointIdentity`]）：
//!
//! * `connection_id` —— 真实的持久化 `connectionId`（`ConnectionConfig::id`）。
//!   §6.2 的 `ensure_service` 按它注册，检测与预留因此对齐到同一身份。
//! * `service_key` —— 端点**位置**的摘要，不是配置 id。两个指向同一台服务器的不同
//!   连接配置共享它，而同一配置指向不同库时它随位置改变。
//!
//! # 摘要而非明文
//!
//! `physical_identity` 的字段集与 `datazen_schema_diff::reviewed::same_endpoint`
//! 完全一致（同样剔除 `user`、同样把方言归一化后写入 `driver`）——两处对「同一物理
//! 端点」的理解一旦分叉，端点重叠检测就会和结构比对给出相反结论。但这里取 SHA-256
//! 摘要而不是明文：`options` / `ssh_tunnel` 可能夹带凭据与令牌，摘要允许把
//! `service_key` 放进错误信息而不泄漏任何连接配置内容。

use datazen_platform_api::id::ConnectionId;
use datazen_schema_diff::normalize_dialect;
use sha2::{Digest, Sha256};

use crate::db::ConnectionConfig;

/// `service_key` 的命名空间前缀，让传输 Job 的位置键不会和别的子系统撞车。
const SERVICE_KEY_PREFIX: &str = "data-transfer";

/// 一个端点交给 runtime 的身份。
///
/// 两个字段都是「同一物理服务」的比较键，缺一不可，故都不能是装饰性的常量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EndpointIdentity {
    /// 真实持久化连接配置 id —— §6.2 预算预留按它注册，端点检测也按它比较。
    pub(crate) connection_id: ConnectionId,
    /// 端点位置的稳定摘要。位置无法指认（既无 host 也无 database）时留空，
    /// 交给 runtime 的 fail-closed 分支，而不是编造一个不证明任何事的摘要。
    pub(crate) service_key: String,
}

impl EndpointIdentity {
    /// 位置不可指认时交出的身份：没有位置键，只剩配置 id 这一条证据。
    #[cfg(test)]
    pub(crate) fn unlocatable(config: &ConnectionConfig) -> Self {
        Self {
            connection_id: ConnectionId::new(config.id.clone()),
            service_key: String::new(),
        }
    }
}

/// 从真实连接配置导出端点身份。
pub(crate) fn identify(config: &ConnectionConfig) -> EndpointIdentity {
    EndpointIdentity {
        connection_id: ConnectionId::new(config.id.clone()),
        service_key: names_a_location(config)
            .then(|| canonical_json(&physical_identity(config)))
            .filter(|canonical| !canonical.is_empty())
            .map(|canonical| {
                let digest = hex_digest(&Sha256::digest(canonical.as_bytes()));
                format!("{SERVICE_KEY_PREFIX}:{digest}")
            })
            .unwrap_or_default(),
    }
}

/// `same_endpoint` 的比较基准：归一化方言 + 位置字段，剔除 `user`。
///
/// 字段集刻意与 `datazen_schema_diff::reviewed::same_endpoint` 保持一致，见模块文档。
/// `schema` 必须在其中：同库跨 schema 的搬运（`schema_a.users` → `schema_b.users`）
/// legacy 是接受的，漏掉 schema 会把它误判成自覆盖。
fn physical_identity(config: &ConnectionConfig) -> serde_json::Value {
    serde_json::json!({
        "driver": normalize_dialect(&config.database_type),
        "host": normalized(config.host.as_deref()),
        "port": config.port,
        "database": normalized(config.database.as_deref()),
        "schema": normalized(config.schema.as_deref()),
        "options": config.options,
        "tunnel": config.ssh_tunnel,
    })
}

/// 该配置是否指认了一个可定位的服务器。
///
/// 既无 host 又无 database 时，摘要证明不了任何位置相同——两个各自独立的本地库
/// 会得到同样的「全 `None` 摘要」。这种情况交出空 `service_key`，让 runtime 在
/// 存在 writer 时 fail-closed，而不是断言一个假的同一性。
fn names_a_location(config: &ConnectionConfig) -> bool {
    normalized(config.host.as_deref()).is_some() || normalized(config.database.as_deref()).is_some()
}

/// 归一化可空字符串：去空白，空白等同缺省。
///
/// 与 `datazen_schema_diff::reviewed::normalized_scope` 同一套规则，让驱动回传的
/// 带空格值与用户填写的干净值不会摘要成两个不同端点。
fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// 确定性 JSON 序列化：同一个 `Value` 经同一函数必然得到同一字节串。
fn canonical_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

const HEX: [u8; 16] = *b"0123456789abcdef";

fn hex_digest(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{SshTunnelConfig, SslMode};
    use serde_json::json;

    fn config() -> ConnectionConfig {
        ConnectionConfig {
            id: "conn-a".into(),
            name: "Primary".into(),
            database_type: "PostgreSQL".into(),
            host: Some("db-a.example.com".into()),
            port: Some(5432),
            database: Some("app".into()),
            schema: Some("public".into()),
            username: Some("someone".into()),
            password: Some("s3cret".into()),
            ssl_mode: SslMode::Prefer,
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    #[test]
    fn identity_is_derived_from_the_real_connection_config() {
        let identity = identify(&config());
        assert_eq!(identity.connection_id.as_str(), "conn-a");
        assert!(identity.service_key.starts_with("data-transfer:"));
        assert_ne!(identity.service_key, "data-transfer");
    }

    #[test]
    fn service_key_never_carries_connection_config_text() {
        let identity = identify(&config());
        for secret in ["db-a.example.com", "app", "public", "someone", "s3cret"] {
            assert!(
                !identity.service_key.contains(secret),
                "service_key must be a digest, not an echo of {secret}"
            );
        }
    }

    #[test]
    fn two_configs_on_one_server_share_a_service_key() {
        let mut alias = config();
        alias.id = "conn-b".into();
        alias.name = "Alias".into();
        let identities = [identify(&config()), identify(&alias)];
        assert_eq!(identities[0].service_key, identities[1].service_key);
        assert_ne!(
            identities[0].connection_id, identities[1].connection_id,
            "aliases are still two distinct configs"
        );
    }

    #[test]
    fn different_physical_endpoints_do_not_share_a_service_key() {
        let mut other = config();
        other.id = "conn-c".into();
        other.host = Some("db-b.example.com".into());
        assert_ne!(
            identify(&config()).service_key,
            identify(&other).service_key
        );
    }

    #[test]
    fn cross_schema_move_on_one_server_is_not_a_self_overwrite() {
        let mut other = config();
        other.id = "conn-d".into();
        other.schema = Some("archive".into());
        assert_ne!(
            identify(&config()).service_key,
            identify(&other).service_key
        );
    }

    #[test]
    fn connection_id_alone_never_decides_the_service_key() {
        let mut moved = config();
        moved.database = Some("other".into());
        assert_eq!(
            identify(&config()).connection_id,
            identify(&moved).connection_id
        );
        assert_ne!(
            identify(&config()).service_key,
            identify(&moved).service_key
        );
    }

    #[test]
    fn dialect_aliases_of_one_server_share_a_service_key() {
        let mut alias = config();
        alias.id = "conn-e".into();
        alias.database_type = "postgresql".into();
        assert_eq!(
            identify(&config()).service_key,
            identify(&alias).service_key
        );
    }

    #[test]
    fn unlocatable_config_yields_no_service_key() {
        let mut local = config();
        local.host = None;
        local.database = None;
        let identity = identify(&local);
        assert_eq!(identity.service_key, "");
        assert_eq!(identity.connection_id.as_str(), "conn-a");
        assert_eq!(identity, EndpointIdentity::unlocatable(&local));
    }

    #[test]
    fn blank_host_and_database_are_treated_as_absent() {
        let mut blank = config();
        blank.host = Some("   ".into());
        blank.database = Some("  ".into());
        assert_eq!(identify(&blank).service_key, "");
    }

    #[test]
    fn service_key_is_stable_across_calls() {
        assert_eq!(
            identify(&config()).service_key,
            identify(&config()).service_key
        );
    }

    #[test]
    fn physical_identity_omits_the_user_name() {
        let mut other = config();
        other.username = Some("someone-else".into());
        assert_eq!(physical_identity(&config()), physical_identity(&other));
    }

    #[test]
    fn physical_identity_keeps_the_tunnel_and_options() {
        let mut tunneled = config();
        tunneled.ssh_tunnel = Some(SshTunnelConfig {
            enabled: true,
            host: "bastion".into(),
            port: 22,
            username: "jump".into(),
            auth_method: "password".into(),
            password: None,
            private_key_path: None,
            passphrase: None,
            jump: None,
        });
        assert_ne!(physical_identity(&config()), physical_identity(&tunneled));

        let mut tuned = config();
        tuned.options = json!({"routing": "replica"}).as_object().cloned();
        assert_ne!(physical_identity(&config()), physical_identity(&tuned));
    }
}
