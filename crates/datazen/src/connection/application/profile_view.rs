//! `ProfileView`：连接档案的回显视图投影（§5.1：连接列表响应归 application）。
//!
//! 原属 platform-api `dto/profile.rs` 的视图部分，D2（2026-10-26）起唯一定义在
//! `datazen::connection::application`。投影入口是
//! [`ProfileView::from_record`]（原 `ProfileRecord::to_view` 迁至本层：
//! domain 不依赖 application 的响应形状）。platform-api 只再导出同一类型。
//!
//! §4.1 的字段校验：列表**不返回** password、token、TLS 私钥或任何可直接解密
//! 材料；`publicOptions` 只含可见且非敏感字段。凭据内容只存 `SecretProvider`。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::connection::domain::ProfileRecord;
use crate::shared::id::{ConnectionId, Counter};

/// 连接配置视图。**不含** `secretRef`、`networkRouteRef`、明文密码等敏感字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub connection_id: ConnectionId,
    pub name: String,
    pub driver_id: String,
    /// 配置版本。变更时该连接上所有会话被判失效（租约仍保持固定，INV-02）。
    pub config_revision: Counter,
    pub credential_revision: Counter,
    pub initial_namespace: crate::connection::domain::NamespaceTarget,
    /// 可见且非敏感的配置项。
    pub public_options: BTreeMap<String, serde_json::Value>,
    /// 只表示「是否已配置凭据」，**不泄露**是否存在、长度或内容。
    pub credential_configured: bool,
    pub enabled: bool,
}

impl ProfileView {
    pub fn new(
        connection_id: ConnectionId,
        name: impl Into<String>,
        driver_id: impl Into<String>,
    ) -> Self {
        Self {
            connection_id,
            name: name.into(),
            driver_id: driver_id.into(),
            config_revision: Counter::ZERO,
            credential_revision: Counter::ZERO,
            initial_namespace: crate::connection::domain::NamespaceTarget::default(),
            public_options: BTreeMap::new(),
            credential_configured: false,
            enabled: true,
        }
    }

    pub fn with_config_revision(mut self, revision: Counter) -> Self {
        self.config_revision = revision;
        self
    }

    pub fn with_credential_revision(mut self, revision: Counter) -> Self {
        self.credential_revision = revision;
        self
    }

    pub fn with_initial_namespace(
        mut self,
        namespace: crate::connection::domain::NamespaceTarget,
    ) -> Self {
        self.initial_namespace = namespace;
        self
    }

    pub fn with_public_option(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.public_options.insert(key.into(), value);
        self
    }

    pub fn with_credential_configured(mut self) -> Self {
        self.credential_configured = true;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 投影为可回显视图：剔除 `secretRef`、网络路由与内部开关，只留下非敏感配置。
    ///
    /// 原 `ProfileRecord::to_view`（platform-api `ports/profile.rs`），D2 迁入
    /// application 层：响应形状属于本层，domain 不反向依赖。
    pub fn from_record(record: &ProfileRecord) -> Self {
        let mut view = Self::new(
            record.connection_id.clone(),
            record.name.clone(),
            record.driver_id.clone(),
        )
        .with_config_revision(record.config_revision.counter())
        .with_credential_revision(record.credential_revision.counter())
        .with_initial_namespace(record.initial_namespace.clone())
        .with_enabled(record.enabled);

        for (key, value) in &record.driver_options {
            if !is_sensitive_option(key) {
                view = view.with_public_option(key.clone(), value.clone());
            }
        }
        if record.secret_ref.is_some() {
            view = view.with_credential_configured();
        }
        view
    }
}

/// 保守的敏感字段名黑名单。
///
/// 宿主不认识某 driver 的 schema 时宁可**多剔除**：视图里少显示一个字段只是体验问题，
/// 错泄一个字段是安全事故。
pub(crate) fn is_sensitive_option(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    const SENSITIVE_MARKERS: [&str; 12] = [
        "password",
        "passwd",
        "secret",
        "token",
        "credential",
        "privatekey",
        "private_key",
        "certkey",
        "apikey",
        "api_key",
        "authorization",
        "signature",
    ];
    SENSITIVE_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::id::{
        ConfigRevision, CredentialRevision, NetworkRouteRevision, OrganizationId, SecretRef,
        Timestamp,
    };
    use serde_json::json;

    fn record() -> ProfileRecord {
        let mut driver_options = BTreeMap::new();
        driver_options.insert("host".to_owned(), serde_json::json!("db.internal"));
        driver_options.insert("port".to_owned(), serde_json::json!(5432));
        driver_options.insert("sslMode".to_owned(), serde_json::json!("require"));
        driver_options.insert("password".to_owned(), serde_json::json!("s3cret"));
        driver_options.insert("privateKeyPem".to_owned(), serde_json::json!("-----BEGIN"));
        driver_options.insert("apiToken".to_owned(), serde_json::json!("tok"));
        driver_options.insert(
            "authorizationHeader".to_owned(),
            serde_json::json!("Bearer x"),
        );

        ProfileRecord {
            name: "prod".into(),
            initial_namespace: crate::connection::domain::NamespaceTarget::database("app"),
            driver_options,
            secret_ref: Some(SecretRef::new("secret://vault/db-prod")),
            read_only: true,
            created_at: Timestamp::new("2026-01-01T00:00:00Z"),
            updated_at: Timestamp::new("2026-01-01T00:00:00Z"),
            ..ProfileRecord::new(
                OrganizationId::new("org-1"),
                ConnectionId::new("conn-1"),
                "postgres",
            )
        }
    }

    #[test]
    fn view_projection_drops_sensitive_options_and_secret_refs() {
        let view = ProfileView::from_record(&record());
        let json = serde_json::to_string(&view).expect("serialize");
        for forbidden in [
            "s3cret",
            "BEGIN",
            "apiToken",
            "privateKeyPem",
            "authorizationHeader",
            "secretRef",
        ] {
            assert!(!json.contains(forbidden), "视图泄漏了 {forbidden}: {json}");
        }
        assert_eq!(
            view.public_options.get("host"),
            Some(&serde_json::json!("db.internal"))
        );
        assert_eq!(
            view.public_options.get("port"),
            Some(&serde_json::json!(5432))
        );
        assert_eq!(
            view.public_options.get("sslMode"),
            Some(&serde_json::json!("require"))
        );
        assert!(view.credential_configured);
    }

    #[test]
    fn view_projection_keeps_shape_fields_in_sync() {
        let record = record();
        let view = ProfileView::from_record(&record);
        assert_eq!(view.config_revision, record.config_revision.counter());
        assert_eq!(
            view.credential_revision,
            record.credential_revision.counter()
        );
        assert_eq!(view.config_revision, Counter::new(0));
        assert_eq!(view.initial_namespace, record.initial_namespace);
        assert!(view.enabled);
    }

    #[test]
    fn sensitive_marker_matching_is_case_insensitive_and_substring_based() {
        assert!(is_sensitive_option("Password"));
        assert!(is_sensitive_option("MY_TOKEN"));
        assert!(is_sensitive_option("sslPrivateKey"));
        assert!(!is_sensitive_option("host"));
        assert!(!is_sensitive_option("port"));
        assert!(!is_sensitive_option("sslMode"));
        assert!(!is_sensitive_option("database"));
    }

    #[test]
    fn missing_secret_ref_means_no_configured_credentials() {
        let mut record = record();
        record.secret_ref = None;
        assert!(!record.credential_configured());
        assert!(!ProfileView::from_record(&record).credential_configured);
    }

    #[test]
    fn profile_view_round_trips_without_any_secret_field() {
        let view = ProfileView::new(ConnectionId::new("conn-1"), "prod", "postgres")
            .with_config_revision(Counter::new(2))
            .with_credential_revision(Counter::new(5));
        let value = serde_json::to_value(&view).expect("serialize");
        assert_eq!(value["configRevision"], json!("2"));
        assert_eq!(value["credentialRevision"], json!("5"));
        assert_eq!(value["credentialConfigured"], json!(false));
        assert_eq!(value["enabled"], json!(true));

        for forbidden in [
            "secretRef",
            "password",
            "token",
            "privateKey",
            "networkRouteRef",
        ] {
            assert!(value.get(forbidden).is_none(), "视图不得出现 {forbidden}");
        }
        assert_eq!(
            serde_json::from_value::<ProfileView>(value).expect("deserialize"),
            view
        );
    }

    #[test]
    fn profile_view_helpers_are_reachable_without_mutating_callers() {
        let view = ProfileView::new(ConnectionId::new("conn-1"), "prod", "postgres")
            .with_config_revision(Counter::new(1))
            .with_initial_namespace(crate::connection::domain::NamespaceTarget::database("app"))
            .with_public_option("sslMode", json!("require"))
            .with_credential_configured()
            .with_enabled(false);

        assert_eq!(view.initial_namespace.database.as_deref(), Some("app"));
        assert_eq!(view.public_options.get("sslMode"), Some(&json!("require")));
        assert!(view.credential_configured);
        assert!(!view.enabled);
        let value = serde_json::to_value(&view).expect("serialize");
        assert_eq!(
            serde_json::from_value::<ProfileView>(value).expect("deserialize"),
            view
        );
    }

    #[test]
    fn record_revisions_surface_as_counters_in_the_view() {
        let mut record = record();
        record.config_revision = ConfigRevision::new(9);
        record.credential_revision = CredentialRevision::new(4);
        record.network_route_revision = NetworkRouteRevision::new(7);
        let view = ProfileView::from_record(&record);
        assert_eq!(view.config_revision, Counter::new(9));
        assert_eq!(view.credential_revision, Counter::new(4));
        // 网络路由版本不属于回显视图。
        let value = serde_json::to_value(&view).expect("serialize");
        assert!(value.get("networkRouteRevision").is_none());
    }
}
