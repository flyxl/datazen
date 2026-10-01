//! 固定实体与可调初值（fake-runtime-fixtures.md §8.1、§7.2 / §9.2）。
//!
//! §8.1 L425：夹具以**数据表**形式提供，测试不得各自硬编码组织、用户、profile、
//! 命名空间或标记值。凡是要在用例里出现这些字面量的地方，一律从 `fixtures()` 取。

use std::collections::BTreeMap;
use std::time::Duration;

use crate::connection::types::{
    ConfigRevision, ConnectionId, OrganizationId, PrincipalId,
};

/// §8.1 L419：两个组织。
pub const ORG_A: &str = "org-alpha";
pub const ORG_B: &str = "org-beta";

/// §8.1 L420：三个用户。`USER_A1` / `USER_A2` 与 driver 侧共享同一个 DB 账号但策略不同。
pub const USER_A1: &str = "user-alpha-1";
pub const USER_A2: &str = "user-alpha-2";
pub const USER_B1: &str = "user-beta-1";

/// §8.1 L421：`PROFILE_P`。
pub const PROFILE_P: &str = "conn-fixture-p";
/// §8.1 L422：`PROFILE_P_V2` —— configRevision 变化 ⇒ `poolKeyFingerprint` 必须换 key。
pub const PROFILE_P_V2: &str = "conn-fixture-p-v2";

/// §8.1 L423：A / B 两个命名空间。
pub const NS_A: &str = "dz_ns_a";
pub const NS_B: &str = "dz_ns_b";

/// §8.1 L424：A / B 两个目标标记值，同名表不同值。
pub const MARKER_A: &str = "dz-marker-alpha";
pub const MARKER_B: &str = "dz-marker-beta";

/// §8.1 L424 / §10.2(2)：跨目标解析护栏表名。
pub const MARKER_TABLE: &str = "dz_target_marker";

/// §8.1 L423：`USER_A1` / `USER_A2` 共享的执行身份。
pub const IDENTITY_SHARED: &str = "exec-identity-shared";

/// §10.2 L511 DDL 通用形态。
pub const MARKER_TABLE_DDL: &str =
    "CREATE TABLE dz_target_marker (id INTEGER PRIMARY KEY, marker TEXT NOT NULL, written_at TIMESTAMP NOT NULL)";

/// §8.1 L423 的个人执行身份生成式。
pub fn identity_personal(user_id: &str) -> String {
    format!("exec-identity-{user_id}")
}

/// 固定组织。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureOrg {
    pub key: &'static str,
    pub id: OrganizationId,
}

/// 固定用户。`execution_identity` 决定 `PoolKeyInputs::execution_identity_key`，
/// `policy_isolation_key` 决定隔离；两者**必须**能独立变化（§8.1 L425、CM-05/CM-67）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureUser {
    pub key: &'static str,
    pub id: PrincipalId,
    pub organization_id: OrganizationId,
    pub execution_identity: String,
    pub policy_isolation_key: String,
    /// 与之共享 driver 侧 DB 账号的用户（§8.1 L420）。
    pub shares_db_account_with: Option<PrincipalId>,
}

/// 固定连接配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureProfile {
    pub key: &'static str,
    pub connection_id: ConnectionId,
    pub config_revision: ConfigRevision,
    pub organization_id: OrganizationId,
}

/// 固定命名空间，附带同名标记表的标记值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureNamespace {
    pub key: &'static str,
    pub database: &'static str,
    pub marker_value: &'static str,
}

/// 固定实体表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureCatalog {
    pub orgs: Vec<FixtureOrg>,
    pub users: Vec<FixtureUser>,
    pub profiles: Vec<FixtureProfile>,
    pub namespaces: Vec<FixtureNamespace>,
    pub marker_table: &'static str,
    pub marker_table_ddl: &'static str,
}

/// §8.1 固定实体表。
pub fn fixtures() -> FixtureCatalog {
    let org_alpha = OrganizationId::new(ORG_A);
    let org_beta = OrganizationId::new(ORG_B);
    let user_a1 = PrincipalId::new(USER_A1);
    let user_a2 = PrincipalId::new(USER_A2);
    let user_b1 = PrincipalId::new(USER_B1);

    FixtureCatalog {
        orgs: vec![
            FixtureOrg { key: "ORG_A", id: org_alpha.clone() },
            FixtureOrg { key: "ORG_B", id: org_beta.clone() },
        ],
        users: vec![
            FixtureUser {
                key: "USER_A1",
                id: user_a1.clone(),
                organization_id: org_alpha.clone(),
                execution_identity: IDENTITY_SHARED.to_owned(),
                policy_isolation_key: "policy-alpha-1".to_owned(),
                shares_db_account_with: Some(user_a2.clone()),
            },
            FixtureUser {
                key: "USER_A2",
                id: user_a2,
                organization_id: org_alpha.clone(),
                // §8.1 L425：与 U1 共享同一执行身份。
                execution_identity: IDENTITY_SHARED.to_owned(),
                // §8.1 L425：但 policyIsolationKey 必须不同，否则 CM-05/CM-67 无法测。
                policy_isolation_key: "policy-alpha-2".to_owned(),
                shares_db_account_with: Some(user_a1.clone()),
            },
            FixtureUser {
                key: "USER_B1",
                id: user_b1,
                organization_id: org_beta,
                execution_identity: identity_personal(USER_B1),
                policy_isolation_key: "policy-beta-1".to_owned(),
                shares_db_account_with: None,
            },
        ],
        profiles: vec![
            FixtureProfile {
                key: "PROFILE_P",
                connection_id: ConnectionId::new(PROFILE_P),
                config_revision: ConfigRevision::new(7),
                organization_id: org_alpha.clone(),
            },
            FixtureProfile {
                key: "PROFILE_P_V2",
                connection_id: ConnectionId::new(PROFILE_P_V2),
                config_revision: ConfigRevision::new(8),
                organization_id: org_alpha,
            },
        ],
        namespaces: vec![
            FixtureNamespace { key: "NS_A", database: NS_A, marker_value: MARKER_A },
            FixtureNamespace { key: "NS_B", database: NS_B, marker_value: MARKER_B },
        ],
        marker_table: MARKER_TABLE,
        marker_table_ddl: MARKER_TABLE_DDL,
    }
}

/// 已安装进 harness 的实体集合。查询走方法，不暴露裸切片。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureWorld {
    catalog: FixtureCatalog,
    indexes: BTreeMap<&'static str, usize>,
}

impl FixtureWorld {
    pub fn empty() -> Self {
        Self { catalog: fixtures(), indexes: BTreeMap::new() }
    }

    /// §2 表里 `fixtures.rs` 的第二个导出。
    pub fn install_fixtures(&mut self) -> &FixtureCatalog {
        self.indexes.clear();
        for (position, org) in self.catalog.orgs.iter().enumerate() {
            self.indexes.insert(org.key, position);
        }
        for (position, user) in self.catalog.users.iter().enumerate() {
            self.indexes.insert(user.key, position);
        }
        for (position, profile) in self.catalog.profiles.iter().enumerate() {
            self.indexes.insert(profile.key, position);
        }
        for (position, namespace) in self.catalog.namespaces.iter().enumerate() {
            self.indexes.insert(namespace.key, position);
        }
        &self.catalog
    }

    pub fn catalog(&self) -> &FixtureCatalog {
        &self.catalog
    }

    pub fn user(&self, key: &str) -> Option<&FixtureUser> {
        let position = *self.indexes.get(key)?;
        self.catalog.users.get(position)
    }

    pub fn profile(&self, key: &str) -> Option<&FixtureProfile> {
        let position = *self.indexes.get(key)?;
        self.catalog.profiles.get(position)
    }

    pub fn namespace(&self, key: &str) -> Option<&FixtureNamespace> {
        let position = *self.indexes.get(key)?;
        self.catalog.namespaces.get(position)
    }

    /// 命名空间对应的标记值 —— §10.2(2) 跨目标解析护栏。
    pub fn marker_value(&self, key: &str) -> Option<&'static str> {
        self.namespace(key).map(|namespace| namespace.marker_value)
    }
}

/// §2 表里 `fixtures.rs` 的第二个导出的自由函数形态。
pub fn install_fixtures(world: &mut FixtureWorld) -> &FixtureCatalog {
    world.install_fixtures()
}

/// §7.2 / §9.2 的可调初值。`Default` 是设计值，`lowered_for_tests()` 是夹具下调值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialValues {
    pub short_op_pool_max_per_pool_key: u32,
    pub pool_idle_ttl: Duration,
    pub empty_pool_metadata_cap: u32,
    pub per_user_connected_editors: u32,
    pub desktop_total_target_connections: u32,
    pub team_per_db_service_quota: u32,
    pub acquire_timeout: Duration,
    pub session_queue_capacity: u32,
    pub per_user_logical_sessions: u32,
    pub per_org_logical_sessions: u32,
    pub client_disconnect_retention: Duration,
    pub no_transaction_idle: Duration,
    pub idle_transaction: Duration,
    pub cancel_cleanup_deadline: Duration,
    pub data_buffer_per_pipeline: u64,
    pub idempotency_token_ttl: Duration,
    /// §6.2：每订阅的事件/字节上限。
    pub drain_events_per_subscription: u64,
    pub drain_bytes_per_subscription: u64,
    /// §6.2：无消费者等待与 drain 期限。
    pub no_consumer_wait: Duration,
    pub drain_deadline: Duration,
}

impl Default for InitialValues {
    fn default() -> Self {
        Self {
            short_op_pool_max_per_pool_key: 2,
            pool_idle_ttl: Duration::from_secs(60),
            empty_pool_metadata_cap: 32,
            per_user_connected_editors: 5,
            desktop_total_target_connections: 16,
            team_per_db_service_quota: 20,
            acquire_timeout: Duration::from_secs(10),
            session_queue_capacity: 32,
            per_user_logical_sessions: 100,
            per_org_logical_sessions: 1000,
            client_disconnect_retention: Duration::from_secs(60),
            no_transaction_idle: Duration::from_secs(30 * 60),
            idle_transaction: Duration::from_secs(5 * 60),
            cancel_cleanup_deadline: Duration::from_secs(10),
            data_buffer_per_pipeline: 8 * 1024 * 1024,
            idempotency_token_ttl: Duration::from_secs(24 * 3600),
            drain_events_per_subscription: 256,
            drain_bytes_per_subscription: 1024 * 1024,
            no_consumer_wait: Duration::from_secs(30),
            drain_deadline: Duration::from_secs(10),
        }
    }
}

impl InitialValues {
    /// §7.2：每用户/每组织逻辑 session 100/1000 → 4/8；数据缓冲 8 MiB → 64 KiB。
    pub fn lowered_for_tests() -> Self {
        Self {
            per_user_logical_sessions: 4,
            per_org_logical_sessions: 8,
            data_buffer_per_pipeline: 64 * 1024,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_table_matches_section_8_1_literals() {
        let catalog = fixtures();
        assert_eq!(catalog.orgs[0].id.as_str(), "org-alpha");
        assert_eq!(catalog.orgs[1].id.as_str(), "org-beta");
        assert_eq!(catalog.users[0].id.as_str(), "user-alpha-1");
        assert_eq!(catalog.users[1].id.as_str(), "user-alpha-2");
        assert_eq!(catalog.users[2].id.as_str(), "user-beta-1");
        assert_eq!(catalog.profiles[0].connection_id.as_str(), "conn-fixture-p");
        assert_eq!(catalog.profiles[0].config_revision.get(), 7);
        assert_eq!(catalog.profiles[1].config_revision.get(), 8);
        assert_eq!(catalog.namespaces[0].database, "dz_ns_a");
        assert_eq!(catalog.namespaces[1].database, "dz_ns_b");
        assert_eq!(catalog.marker_table, "dz_target_marker");
        assert_eq!(catalog.namespaces[0].marker_value, "dz-marker-alpha");
        assert_eq!(catalog.namespaces[1].marker_value, "dz-marker-beta");
    }

    #[test]
    fn shared_db_account_users_differ_only_in_policy_isolation_key() {
        // §8.1 L425 / CM-05、CM-67：共享 IDENTITY_SHARED，但 policyIsolationKey 必须不同。
        let catalog = fixtures();
        let a1 = catalog.users.iter().find(|user| user.id.as_str() == USER_A1).expect("USER_A1 必须在表里");
        let a2 = catalog.users.iter().find(|user| user.id.as_str() == USER_A2).expect("USER_A2 必须在表里");
        assert_eq!(a1.execution_identity, IDENTITY_SHARED);
        assert_eq!(a2.execution_identity, IDENTITY_SHARED, "两者共享同一个 DB 账号");
        assert_eq!(a1.shares_db_account_with.as_ref(), Some(&a2.id));
        assert_ne!(
            a1.policy_isolation_key, a2.policy_isolation_key,
            "policyIsolationKey 相同会让 CM-05/CM-67 变成假绿"
        );
    }

    #[test]
    fn cross_organization_user_is_not_visible_to_org_alpha() {
        // §8.1 L421：USER_B1 跨组织不可见。
        let catalog = fixtures();
        let a1 = catalog.users.iter().find(|user| user.id.as_str() == USER_A1).expect("USER_A1");
        let b1 = catalog.users.iter().find(|user| user.id.as_str() == USER_B1).expect("USER_B1");
        assert_ne!(a1.organization_id, b1.organization_id);
        assert_ne!(a1.execution_identity, b1.execution_identity, "个人执行身份必须不同于共享身份");
    }

    #[test]
    fn profile_v2_changes_config_revision_so_pool_key_must_change() {
        // §8.1 L422：验证 poolKeyFingerprint 换 key。
        let catalog = fixtures();
        let p = catalog.profiles.iter().find(|profile| profile.key == "PROFILE_P").expect("PROFILE_P");
        let v2 = catalog.profiles.iter().find(|profile| profile.key == "PROFILE_P_V2").expect("PROFILE_P_V2");
        assert_ne!(p.config_revision.get(), v2.config_revision.get());
    }

    #[test]
    fn installed_world_resolves_every_key_and_marker() {
        let mut world = FixtureWorld::empty();
        install_fixtures(&mut world);
        assert_eq!(world.user("USER_B1").expect("USER_B1").id.as_str(), "user-beta-1");
        assert_eq!(world.profile("PROFILE_P_V2").expect("PROFILE_P_V2").config_revision.get(), 8);
        assert_eq!(world.namespace("NS_A").expect("NS_A").database, "dz_ns_a");
        assert_eq!(world.marker_value("NS_A"), Some("dz-marker-alpha"));
        assert_eq!(world.marker_value("NS_B"), Some("dz-marker-beta"));
        assert_ne!(
            world.marker_value("NS_A"),
            world.marker_value("NS_B"),
            "§10.2(2)：同名表必须写入不同标记值"
        );
        assert_eq!(world.marker_value("NS_MISSING"), None);
    }

    #[test]
    fn lowered_values_only_touch_the_three_documented_knobs() {
        let design = InitialValues::default();
        let lowered = InitialValues::lowered_for_tests();
        assert_eq!(lowered.per_user_logical_sessions, 4);
        assert_eq!(lowered.per_org_logical_sessions, 8);
        assert_eq!(lowered.data_buffer_per_pipeline, 64 * 1024);
        assert_eq!(design.per_user_logical_sessions, 100);
        assert_eq!(design.per_org_logical_sessions, 1000);
        assert_eq!(design.data_buffer_per_pipeline, 8 * 1024 * 1024);
        assert_eq!(lowered.acquire_timeout, design.acquire_timeout, "其余初值不得被下调");
        assert_eq!(lowered.no_transaction_idle, Duration::from_secs(1800));
        assert_eq!(lowered.idle_transaction, Duration::from_secs(300));
        assert_eq!(lowered.client_disconnect_retention, Duration::from_secs(60));
        assert_eq!(lowered.idempotency_token_ttl, Duration::from_secs(24 * 3600));
        assert_eq!(lowered.drain_events_per_subscription, 256);
    }
}