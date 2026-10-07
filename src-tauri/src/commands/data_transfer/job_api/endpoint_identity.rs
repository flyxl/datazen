//! 数据迁移 Job 端点的真实身份：端点重叠检测与 runtime 的原子预算预留共用同一身份。
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
//! # 两个身份，两个用途，不能混用
//!
//! `connection_id` 与 `service_key` **不是**同一件事，混用正是「一条已保存连接跨库拷贝
//! 被误拒」这个缺陷的根因：
//!
//! * `connection_id` —— 真实的持久化 `connectionId`（`ConnectionConfig::id`）。它回答
//!   「这些配额记到哪条已保存连接上」，是 runtime `EndpointRef::ensure_service` 原子预算
//!   预留的记账键。**同一条连接
//!   可以承载任意多个库**（`connect_dedicated` 只覆盖 `effective_config.database`，
//!   `id` 不变），所以它对「是不是同一个物理端点」没有发言权：拿它当重叠键，会把
//!   `staging.users` → `prod.users` 这类合法跨库拷贝判成自覆盖，而且报出的
//!   「同一物理端点」是假的。
//! * `service_key` —— 端点**位置**的摘要，不是配置 id。两个指向同一台服务器的不同
//!   连接配置共享它，而同一配置指向不同库时它随位置改变。重叠检测**只**用它。
//!
//! # 位置必须先解析默认值再摘要
//!
//! `connect` 在驱动 crate 内部把缺省的 host/port 替换成该驱动的默认端口与地址，所以
//! 「host 留空」和「host 显式写成那个默认值」在物理上是同一台服务器。若直接摘要配置
//! 原值，两者会得到两个不同摘要，真实的自覆盖就会被静默漏掉。因此 [`identify`] 先向
//! 驱动问出它的默认值（`DatabaseDriver::default_host` / `default_port`），把**解析后**
//! 的位置写进摘要。宿主不按 Driver 类型硬编码——默认值是驱动自己的知识，由驱动声明。
//!
//! 残留的不可判定项（`localhost` 与 `127.0.0.1` 这类别名、DNS 大小写）只有在真正
//! 建连之后才能分辨，准入期无从得知，故不纳入摘要；需要绝对判定时应由调用方在
//! 同一台服务器上只建一个目标库会话。
//!
//! # 摘要而非明文
//!
//! `physical_identity` 的字段集与 `datazen_schema_diff::reviewed::same_endpoint`
//! 完全一致（同样剔除 `user`、同样把方言归一化后写入 `driver`）——两处对「同一物理
//! 端点」的理解一旦分叉，端点重叠检测就会和结构比对给出相反结论。但这里取 SHA-256
//! 摘要而不是明文：`options` / `ssh_tunnel` 可能夹带凭据与令牌，摘要允许把
//! `service_key` 放进错误信息而不泄漏任何连接配置内容。

use datazen_driver_api::DatabaseDriver;
use datazen_platform_api::id::ConnectionId;
use datazen_schema_diff::normalize_dialect;
use sha2::{Digest, Sha256};
use std::borrow::Cow;

use crate::db::ConnectionConfig;

/// `service_key` 的命名空间前缀，让传输 Job 的位置键不会和别的子系统撞车。
const SERVICE_KEY_PREFIX: &str = "data-transfer";

/// 一个端点交给 runtime 的身份。
///
/// 两个字段各司其职，缺一不可：缺 `connection_id` 就没有配额记账的归属，缺
/// `service_key` 就没有重叠检测的物理依据。两者都不是装饰性的常量，但**只有
/// `service_key` 参与重叠比较**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EndpointIdentity {
    /// 真实持久化连接配置 id —— runtime 的原子预算预留按它注册。**不**参与重叠检测。
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
///
/// `driver` 是该端点实际使用的驱动实例：它回答「host / port 留空时驱动会连到哪里」，
/// 使缺省写法与显式默认值摘要成同一个物理端点。用错驱动（例如把源端配置的 driver
/// 配成目标端驱动）会重开「缺省 host 漏检自覆盖」的缺口。
pub(crate) fn identify(config: &ConnectionConfig, driver: &dyn DatabaseDriver) -> EndpointIdentity {
    let location = ResolvedLocation::of(config, driver);
    EndpointIdentity {
        connection_id: ConnectionId::new(config.id.clone()),
        service_key: location
            .names_a_location(config)
            .then(|| canonical_json(&physical_identity(config, &location)))
            .filter(|canonical| !canonical.is_empty())
            .map(|canonical| {
                let digest = hex_digest(&Sha256::digest(canonical.as_bytes()));
                format!("{SERVICE_KEY_PREFIX}:{digest}")
            })
            .unwrap_or_default(),
    }
}

/// 配置里写着的地址，加上驱动在留空时会替上的默认值。
///
/// 只解析 host / port：这两个字段是驱动唯一会隐式补齐的位置信息，`database` 留空
/// 就是「连到该驱动的默认库」，各驱动差异过大且已由 `database` 自身参与摘要。
///
/// `driver.default_host()` 返回 `None` 的含义是「该驱动不施加隐式 host」，不是
/// 「不知道」：据此仍解析不出 host 的配置只能靠 `database` 指认位置。真正无法证明
/// 位置的情形由 [`ResolvedLocation::names_a_location`] 判空，交回 runtime 的
/// fail-closed 分支，而不是摘要出一个证明不了任何事的键。
struct ResolvedLocation<'a> {
    host: Option<Cow<'a, str>>,
    port: Option<u16>,
}

impl<'a> ResolvedLocation<'a> {
    fn of(config: &'a ConnectionConfig, driver: &dyn DatabaseDriver) -> Self {
        Self {
            host: normalized(config.host.as_deref())
                .map(Cow::Borrowed)
                .or_else(|| driver.default_host().map(Cow::Borrowed)),
            port: config.port.or_else(|| driver.default_port()),
        }
    }

    /// 该配置是否指认了一个可定位的位置。
    ///
    /// 既无 host（解析后仍无）又无 database 时，摘要证明不了任何位置相同——两个
    /// 各自独立的本地库会得到同样的「全 `None` 摘要」。这种情况交出空 `service_key`，
    /// 让 runtime 在存在 writer 时 fail-closed，而不是断言一个假的同一性。
    ///
    /// 只认 `database` 兜底是必要的：SQLite / DuckDB 的连接配置把文件路径写在
    /// `database` 里、host 恒为空，它们靠这一条就能被正确区分。
    fn names_a_location(&self, config: &ConnectionConfig) -> bool {
        self.host.is_some() || normalized(config.database.as_deref()).is_some()
    }
}

/// `same_endpoint` 的比较基准：归一化方言 + **解析后**的位置字段，剔除 `user`。
///
/// 字段集刻意与 `datazen_schema_diff::reviewed::same_endpoint` 保持一致，见模块文档。
/// `schema` 必须在其中：同库跨 schema 的搬运（`schema_a.users` → `schema_b.users`）
/// legacy 是接受的，漏掉 schema 会把它误判成自覆盖。host / port 取 [`ResolvedLocation`]
/// 而不是配置原值，否则缺省写法与显式默认值会被摘要成两个不同端点。
fn physical_identity(
    config: &ConnectionConfig,
    location: &ResolvedLocation<'_>,
) -> serde_json::Value {
    serde_json::json!({
        "driver": normalize_dialect(&config.database_type),
        "host": location.host.as_deref(),
        "port": location.port,
        "database": normalized(config.database.as_deref()),
        "schema": normalized(config.schema.as_deref()),
        "options": config.options,
        "tunnel": config.ssh_tunnel,
    })
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
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
    use serde_json::json;
    use std::sync::Arc;

    /// 一个**不施加**隐式 host/port 的驱动（trait 默认实现）。
    fn driver() -> Arc<MockDriver> {
        MockDriver::new("postgres", MockDriverOptions::default())
    }

    /// 一个**声明了**隐式 host/port 的驱动，模拟 postgres/mysql 这类 `connect` 会
    /// 代填默认值的驱动。
    fn driver_with_defaults() -> Arc<MockDriver> {
        MockDriver::new(
            "postgres",
            MockDriverOptions {
                default_host: Some("db-a.example.com"),
                default_port: Some(5432),
                ..MockDriverOptions::default()
            },
        )
    }

    /// 一个只声明了默认端口、**没有**默认 host 的驱动，模拟 sqlserver 这类
    /// host 必填、端口可省略的驱动。
    fn driver_with_default_port_only() -> Arc<MockDriver> {
        MockDriver::new(
            "sqlserver",
            MockDriverOptions {
                default_port: Some(1433),
                ..MockDriverOptions::default()
            },
        )
    }

    fn location(config: &ConnectionConfig) -> ResolvedLocation<'_> {
        ResolvedLocation::of(config, &*driver())
    }
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
        let identity = identify(&config(), &*driver());
        assert_eq!(identity.connection_id.as_str(), "conn-a");
        assert!(identity.service_key.starts_with("data-transfer:"));
        assert_ne!(identity.service_key, "data-transfer");
    }

    #[test]
    fn service_key_never_carries_connection_config_text() {
        let identity = identify(&config(), &*driver());
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
        let identities = [
            identify(&config(), &*driver()),
            identify(&alias, &*driver()),
        ];
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
            identify(&config(), &*driver()).service_key,
            identify(&other, &*driver()).service_key
        );
    }

    #[test]
    fn cross_schema_move_on_one_server_is_not_a_self_overwrite() {
        let mut other = config();
        other.id = "conn-d".into();
        other.schema = Some("archive".into());
        assert_ne!(
            identify(&config(), &*driver()).service_key,
            identify(&other, &*driver()).service_key
        );
    }

    #[test]
    fn connection_id_alone_never_decides_the_service_key() {
        let mut moved = config();
        moved.database = Some("other".into());
        assert_eq!(
            identify(&config(), &*driver()).connection_id,
            identify(&moved, &*driver()).connection_id
        );
        assert_ne!(
            identify(&config(), &*driver()).service_key,
            identify(&moved, &*driver()).service_key
        );
    }

    #[test]
    fn dialect_aliases_of_one_server_share_a_service_key() {
        let mut alias = config();
        alias.id = "conn-e".into();
        alias.database_type = "postgresql".into();
        assert_eq!(
            identify(&config(), &*driver()).service_key,
            identify(&alias, &*driver()).service_key
        );
    }

    #[test]
    fn unlocatable_config_yields_no_service_key() {
        let mut local = config();
        local.host = None;
        local.database = None;
        let identity = identify(&local, &*driver());
        assert_eq!(identity.service_key, "");
        assert_eq!(identity.connection_id.as_str(), "conn-a");
        assert_eq!(identity, EndpointIdentity::unlocatable(&local));
    }

    #[test]
    fn blank_host_and_database_are_treated_as_absent() {
        let mut blank = config();
        blank.host = Some("   ".into());
        blank.database = Some("  ".into());
        assert_eq!(identify(&blank, &*driver()).service_key, "");
    }

    #[test]
    fn service_key_is_stable_across_calls() {
        assert_eq!(
            identify(&config(), &*driver()).service_key,
            identify(&config(), &*driver()).service_key
        );
    }

    #[test]
    fn physical_identity_omits_the_user_name() {
        let mut other = config();
        other.username = Some("someone-else".into());
        assert_eq!(
            physical_identity(&config(), &location(&config())),
            physical_identity(&other, &location(&other))
        );
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
        assert_ne!(
            physical_identity(&config(), &location(&config())),
            physical_identity(&tunneled, &location(&tunneled))
        );

        let mut tuned = config();
        tuned.options = json!({"routing": "replica"}).as_object().cloned();
        assert_ne!(
            physical_identity(&config(), &location(&config())),
            physical_identity(&tuned, &location(&tuned))
        );
    }

    /// 留空 `host` 与把它写成驱动的默认 host 指向**同一台机器**，必须摘要成
    /// 同一个 service_key，否则同一张表的自覆盖在 admission 就漏判了。
    ///
    /// 反向断言同样重要：驱动**没有**声明默认 host 时，留空 host 就是「未知位置」，
    /// 不能借一个不存在的默认值来补。
    #[test]
    fn an_omitted_host_digests_as_the_drivers_declared_default() {
        let mut omitted = config();
        omitted.host = None;
        let explicit = config();

        assert_eq!(
            identify(&omitted, &*driver_with_defaults()).service_key,
            identify(&explicit, &*driver_with_defaults()).service_key,
            "省略 host 与显式写成默认 host 是同一台机器，必须同键（否则会漏判成两个不同端点）"
        );

        assert_ne!(
            identify(&omitted, &*driver()).service_key,
            identify(&explicit, &*driver()).service_key,
            "驱动未声明默认 host 时，省略 host 与显式 host 不是同一个位置"
        );
    }

    /// 端口同理：省略 `port` 与显式写成默认端口是同一台机器。
    #[test]
    fn an_omitted_port_digests_as_the_drivers_declared_default() {
        let mut omitted = config();
        omitted.port = None;
        assert_eq!(
            identify(&omitted, &*driver_with_defaults()).service_key,
            identify(&config(), &*driver_with_defaults()).service_key,
        );

        // 两者都省略时，解析结果就是驱动声明的那一对默认值，因此与写全相同。
        // 这一条是用户真正会写出来的形态：UI 里「什么都不填」与「把默认值抄上去」
        // 是同一台机器，绝不能因为省略就漏判自覆盖。
        let mut omitted_both = config();
        omitted_both.host = None;
        omitted_both.port = None;
        assert_eq!(
            identify(&omitted_both, &*driver_with_defaults()).service_key,
            identify(&config(), &*driver_with_defaults()).service_key,
            "host 与 port 同时省略，解析后就是驱动声明的默认值，必须同键"
        );

        // 反向：驱动只声明端口、没有声明默认 host 时，省略的 host 不能被补成
        // 别人的默认值 —— 那是凭空造位置，会制造假重叠也会掩盖真重叠。
        let mut hostless = config();
        hostless.host = None;
        hostless.port = None;
        assert_ne!(
            identify(&hostless, &*driver_with_default_port_only()).service_key,
            identify(&config(), &*driver_with_default_port_only()).service_key,
            "驱动没有声明默认 host 时，省略 host 与显式 host 不是同一个位置"
        );
    }

    /// 配置里写的值优先于驱动默认：显式 host 不该被默认值覆盖。
    #[test]
    fn an_explicit_host_beats_the_drivers_declared_default() {
        let mut elsewhere = config();
        elsewhere.host = Some("db-b.example.com".into());
        assert_ne!(
            identify(&elsewhere, &*driver_with_defaults()).service_key,
            identify(&config(), &*driver_with_defaults()).service_key,
        );
    }

    /// 文件型驱动（SQLite/DuckDB）把库当作 `database` 里的文件路径，`host` 恒为空。
    /// 若把「能否定位」只押在解析后的 host 上，它们的 service_key 会永远为空，
    /// 于是每一次搬运都被 fail-closed 挡掉。
    #[test]
    fn a_file_backed_config_keeps_its_service_key_from_the_database_path() {
        let mut file = config();
        file.id = "conn-sqlite".into();
        file.database_type = "SQLite".into();
        file.host = None;
        file.port = None;
        file.database = Some("/tmp/datazen/app.db".into());

        let identity = identify(&file, &*driver());
        assert!(
            identity.service_key.starts_with("data-transfer:"),
            "文件型驱动必须仍能从 database 路径定位，got {:?}",
            identity.service_key
        );

        let mut other_file = file.clone();
        other_file.database = Some("/tmp/datazen/other.db".into());
        assert_ne!(
            identify(&file, &*driver()).service_key,
            identify(&other_file, &*driver()).service_key,
            "不同文件路径不是同一物理端点"
        );
    }
}
