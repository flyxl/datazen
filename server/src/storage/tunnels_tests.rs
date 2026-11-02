//! `ServerTunnels` 的行为钉子（与端口逐方法对齐 + 名单档无秘密纪律）。

use std::sync::Arc;

use datazen::shared::id::OrganizationId;
use datazen::shared::scope::OrganizationScope;
use datazen::tunnel::domain::TunnelRepository as _;
use datazen_driver_api::{HttpProxyTunnelConfig, SavedTunnel, TunnelKind};

use super::ServerTunnels;
use crate::execution::secret_provider::ServerSecretVault;

const KEY: &[u8] = b"tunnel-repository-test-key-32-bytes!!";

fn rig() -> (ServerTunnels, Arc<crate::storage::AdminStore>) {
    let store = Arc::new(crate::storage::AdminStore::open_in_memory().expect("store"));
    store.migrate().expect("migrate");
    let vault = Arc::new(ServerSecretVault::new(store.clone(), KEY.to_vec()).expect("vault"));
    (ServerTunnels::new(store.clone(), vault), store)
}

fn scope(org: &str) -> OrganizationScope {
    OrganizationScope {
        organization_id: OrganizationId::new(org),
    }
}

fn ssh_tunnel(id: &str, password: Option<&str>) -> SavedTunnel {
    SavedTunnel {
        id: id.into(),
        name: format!("Tunnel {id}"),
        kind: TunnelKind::Ssh,
        ssh: Some(datazen_driver_api::SshTunnelConfig {
            enabled: true,
            host: "jump.internal".into(),
            port: 22,
            username: "ops".into(),
            auth_method: "password".into(),
            password: password.map(str::to_string),
            private_key_path: None,
            passphrase: None,
            jump: None,
        }),
        http_proxy: None,
        websocket: None,
    }
}

#[tokio::test]
async fn save_then_get_round_trips_with_plaintext_credentials() {
    let (repo, _store) = rig();
    repo.save(&scope("org-1"), ssh_tunnel("t1", Some("jump-pass")))
        .await
        .expect("save");
    let fetched = repo
        .get(&scope("org-1"), "t1")
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(
        fetched.ssh.expect("ssh").password.as_deref(),
        Some("jump-pass")
    );
}

#[tokio::test]
async fn list_tier_is_metadata_only() {
    let (repo, _store) = rig();
    repo.save(&scope("org-1"), ssh_tunnel("t1", Some("jump-pass")))
        .await
        .expect("save");
    let list = repo.list(&scope("org-1")).await.expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "t1");
    let json = serde_json::to_string(&list).expect("serialize");
    assert!(!json.contains("jump-pass"), "名单档泄漏了凭据：{json}");
    assert!(
        !json.to_lowercase().contains("password"),
        "名单档不含秘密字段名"
    );
}

#[tokio::test]
async fn plaintext_never_lands_in_the_row() {
    let (repo, store) = rig();
    repo.save(&scope("org-1"), ssh_tunnel("t1", Some("jump-pass")))
        .await
        .expect("save");
    let conn = store.lock_for_adapter().expect("conn");
    let raw: String = conn
        .query_row(
            "SELECT config_json FROM tunnel_definition WHERE tunnel_id='t1'",
            [],
            |row| row.get(0),
        )
        .expect("row");
    assert!(!raw.contains("jump-pass"), "净化投影不得含明文：{raw}");
}

#[tokio::test]
async fn organization_scoping_hides_other_tenants_cm05() {
    let (repo, _store) = rig();
    repo.save(&scope("org-1"), ssh_tunnel("t1", None))
        .await
        .expect("save");
    assert!(repo
        .get(&scope("org-2"), "t1")
        .await
        .expect("get")
        .is_none());
    assert!(repo.list(&scope("org-2")).await.expect("list").is_empty());
    let delete = repo.delete(&scope("org-2"), "t1").await;
    assert!(matches!(
        delete,
        Err(datazen::shared::error::PortError::NotFound(_))
    ));
}

#[tokio::test]
async fn delete_removes_the_row_and_its_sealed_material() {
    let (repo, store) = rig();
    repo.save(&scope("org-1"), ssh_tunnel("t1", Some("jump-pass")))
        .await
        .expect("save");
    repo.delete(&scope("org-1"), "t1").await.expect("delete");
    let conn = store.lock_for_adapter().expect("conn");
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM stored_secret WHERE organization_id='org-1'",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(rows, 0, "秘密材料随行消亡");
}

#[tokio::test]
async fn usage_projects_connections_via_the_indexed_column() {
    let (repo, store) = rig();
    repo.save(&scope("org-1"), ssh_tunnel("t1", None))
        .await
        .expect("save tunnel");
    // 直接落两行连接目录（投影列 tunnel_id 指向 t1）。
    let conn = store.lock_for_adapter().expect("conn");
    let at = "2026-01-01T00:00:00.000Z";
    for (id, name) in [("c1", "Alpha"), ("c2", "Beta")] {
        conn.execute(
            "INSERT INTO connection_catalog
               (organization_id, connection_id, name, group_name, tunnel_id, secret_ref,
                config_json, sort_order, last_connected_at, created_at, updated_at)
             VALUES ('org-1', ?1, ?2, NULL, 't1', NULL, '{}', 0, NULL, ?3, ?3)",
            rusqlite::params![id, name, at],
        )
        .expect("insert connection");
    }
    drop(conn);
    let usage = repo.usage(&scope("org-1"), "t1").await.expect("usage");
    assert_eq!(usage.connection_ids, vec!["c1", "c2"]);
    assert_eq!(usage.connection_names, vec!["Alpha", "Beta"]);
    // 未被引用的隧道 = 空投影。
    let empty = repo.usage(&scope("org-1"), "t-none").await.expect("usage");
    assert!(empty.connection_ids.is_empty());
}

#[tokio::test]
async fn kind_vocabulary_is_enforced_on_both_sides() {
    let (repo, _store) = rig();
    // `none` 不是持久化形态：save 拒绝。
    let mut none = ssh_tunnel("t-none", None);
    none.kind = TunnelKind::None;
    let refused = repo.save(&scope("org-1"), none).await;
    assert!(matches!(
        refused,
        Err(datazen::shared::error::PortError::BackendUnavailable(_))
    ));
    // httpProxy/websocket 形态可持久化并回读。
    let proxy = SavedTunnel {
        id: "t2".into(),
        name: "Proxy".into(),
        kind: TunnelKind::HttpProxy,
        ssh: None,
        http_proxy: Some(HttpProxyTunnelConfig {
            enabled: true,
            host: "proxy.internal".into(),
            port: 3128,
            scheme: "http".into(),
            username: None,
            password: Some("proxy-token".into()),
            headers: None,
            connect_timeout_secs: 30,
        }),
        websocket: None,
    };
    repo.save(&scope("org-1"), proxy).await.expect("save proxy");
    let fetched = repo
        .get(&scope("org-1"), "t2")
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(fetched.kind, TunnelKind::HttpProxy);
    assert_eq!(
        fetched.http_proxy.expect("proxy").password.as_deref(),
        Some("proxy-token"),
        "代理凭据逐字回显"
    );
}
