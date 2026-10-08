//! Shared physical identity for migration admission; profile IDs only own budget accounting.
use datazen_driver_api::DatabaseDriver;
use datazen_platform_api::id::ConnectionId;
use datazen_schema_diff::normalize_dialect;
use sha2::{Digest, Sha256};
use std::borrow::Cow;

use crate::db::ConnectionConfig;

/// `service_key` 的命名空间前缀，让传输 Job 的位置键不会和别的子系统撞车。
const SERVICE_KEY_PREFIX: &str = "data-migration";

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

/// Resolve a live endpoint, rejecting missing ownership/configuration before admission.
pub(crate) async fn session_identity(
    manager: &super::ConnectionManager,
    db_session_id: &str,
    scope: Option<(&str, Option<&str>)>,
) -> Result<EndpointIdentity, crate::commands::CommandError> {
    use crate::commands::{CmdExt, CommandError};
    let owner = manager
        .owner_connection_id(db_session_id)
        .await
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| {
            CommandError::Validation("Migration session has no owning connection".into())
        })?;
    let (driver, _) = manager
        .get_session(db_session_id)
        .await
        .cmd_err("migration_endpoint")?;
    let mut config = manager
        .migration_identity_config(db_session_id)
        .await
        .cmd_err("migration_endpoint")?;
    if let Some((database, schema)) = scope {
        config.database = normalized(Some(database))
            .or_else(|| normalized(config.database.as_deref()))
            .map(str::to_owned);
        config.schema = crate::services::metadata_schema(
            driver.as_ref(),
            schema,
            None,
            config.schema.as_deref(),
        );
    }
    config.id = owner;
    checked_identify(&config, driver.as_ref())
}

pub(crate) fn checked_identify(
    config: &ConnectionConfig,
    driver: &dyn DatabaseDriver,
) -> Result<EndpointIdentity, crate::commands::CommandError> {
    let identity = identify(config, driver);
    if identity.connection_id.as_str().trim().is_empty() || identity.service_key.is_empty() {
        return Err(crate::commands::CommandError::Validation(
            "Migration endpoint requires an owning connection and physical location".into(),
        ));
    }
    Ok(identity)
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
    schema: Option<Cow<'a, str>>,
}

impl<'a> ResolvedLocation<'a> {
    fn of(config: &'a ConnectionConfig, driver: &dyn DatabaseDriver) -> Self {
        Self {
            host: normalized(config.host.as_deref())
                .map(Cow::Borrowed)
                .or_else(|| driver.default_host().map(Cow::Borrowed)),
            port: config.port.or_else(|| driver.default_port()),
            schema: normalized(config.schema.as_deref())
                .map(Cow::Borrowed)
                .or_else(|| driver.default_schema().map(Cow::Borrowed)),
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
        "host": location.host.as_deref().map(str::to_ascii_lowercase),
        "port": location.port,
        "database": normalized(config.database.as_deref()),
        "schema": location.schema.as_deref(),
        "options": location_options(&serde_json::json!(config.options)),
        "tunnel": location_options(&serde_json::json!(config.ssh_tunnel)),
        "tunnel_kind": config.tunnel_kind,
        "http_proxy_tunnel": location_options(&serde_json::json!(config.http_proxy_tunnel)),
        "websocket_tunnel": location_options(&serde_json::json!(config.websocket_tunnel)),
    })
}

// Authentication and profile labels do not change a routed physical location.
fn location_options(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(fields) => serde_json::Value::Object(
            fields
                .iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "password"
                            | "passphrase"
                            | "privateKeyPath"
                            | "username"
                            | "authMethod"
                            | "token"
                            | "authorization"
                            | "headers"
                            | "name"
                            | "id"
                    )
                })
                .map(|(key, value)| (key.clone(), location_options(value)))
                .collect(),
        ),
        serde_json::Value::Array(values) => values.iter().map(location_options).collect(),
        _ => value.clone(),
    }
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
        assert!(identity.service_key.starts_with("data-migration:"));
        assert_ne!(identity.service_key, "data-migration");
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
            identity.service_key.starts_with("data-migration:"),
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
    #[test]
    fn admission_journey_keeps_profile_accounting_separate_from_physical_overlap() {
        let original = config();
        let mut alias = original.clone();
        alias.id = "another-profile".into();
        alias.host = Some("DB-A.EXAMPLE.COM".into());
        alias.username = Some("other-user".into());
        alias.password = Some("synthetic-other-password".into());
        let before = checked_identify(&original, &*driver_with_defaults()).unwrap();
        let same = checked_identify(&alias, &*driver_with_defaults()).unwrap();
        assert_eq!(before.service_key, same.service_key);
        assert_ne!(before.connection_id, same.connection_id);
        alias.database = Some("archive".into());
        assert_ne!(
            before.service_key,
            checked_identify(&alias, &*driver_with_defaults())
                .unwrap()
                .service_key
        );
        alias.database = original.database.clone();
        alias.port = Some(5433);
        assert_ne!(
            before.service_key,
            checked_identify(&alias, &*driver_with_defaults())
                .unwrap()
                .service_key
        );
        alias.port = None;
        assert_eq!(
            before.service_key,
            checked_identify(&alias, &*driver_with_defaults())
                .unwrap()
                .service_key
        );
        alias.id.clear();
        assert!(checked_identify(&alias, &*driver_with_defaults()).is_err());
        alias.id = "restored-profile".into();
        alias.host = None;
        alias.database = None;
        assert!(checked_identify(&alias, &*driver()).is_err());
        alias.database = Some("restored-db".into());
        assert!(checked_identify(&alias, &*driver()).is_ok());
    }

    #[test]
    fn tunnel_authentication_changes_do_not_hide_same_location() {
        let mut first = config();
        first.ssh_tunnel = Some(SshTunnelConfig {
            enabled: true,
            host: "bastion".into(),
            port: 22,
            username: "user".into(),
            auth_method: "password".into(),
            password: Some("synthetic-a".into()),
            private_key_path: None,
            passphrase: None,
            jump: None,
        });
        let mut second = first.clone();
        second.ssh_tunnel.as_mut().unwrap().password = Some("synthetic-b".into());
        assert_eq!(
            identify(&first, &*driver()).service_key,
            identify(&second, &*driver()).service_key
        );
        second.ssh_tunnel.as_mut().unwrap().host = "other-bastion".into();
        assert_ne!(
            identify(&first, &*driver()).service_key,
            identify(&second, &*driver()).service_key
        );
    }
    #[tokio::test]
    async fn session_aliases_are_refused_and_cross_database_sessions_remain_legal() {
        use crate::testing::app_state::TestAppState;
        use datazen_runtime::job::{detect_endpoint_overlap, EndpointRef, EndpointRole};
        let test = TestAppState::new().await;
        let (_, source) = test.save_and_connect("source-alias").await;
        let (_, target) = test.save_and_connect("target-alias").await;
        let source_identity = session_identity(&test.state.connection_manager, &source, None)
            .await
            .unwrap();
        let target_identity = session_identity(&test.state.connection_manager, &target, None)
            .await
            .unwrap();
        let mut endpoints = vec![
            EndpointRef {
                connection_id: source_identity.connection_id,
                service_key: source_identity.service_key,
                objects: vec!["users".into()],
                role: EndpointRole::SourceReader,
            },
            EndpointRef {
                connection_id: target_identity.connection_id,
                service_key: target_identity.service_key,
                objects: vec!["users".into()],
                role: EndpointRole::TargetWriter,
            },
        ];
        assert!(detect_endpoint_overlap(&endpoints).is_err());
        let other = session_identity(
            &test.state.connection_manager,
            &source,
            Some(("another-database", None)),
        )
        .await
        .unwrap();
        endpoints[1].connection_id = other.connection_id;
        endpoints[1].service_key = other.service_key;
        assert!(detect_endpoint_overlap(&endpoints).is_ok());
        assert!(
            session_identity(&test.state.connection_manager, "missing", None)
                .await
                .is_err()
        );
    }
}
