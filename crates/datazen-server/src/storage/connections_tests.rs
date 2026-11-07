//! `ServerConnections` 的行为钉子（与端口逐方法对齐 + 凭据密封纪律）。

use std::sync::Arc;

use datazen::connection::domain::catalog::{ConnectionRepository as _, ConnectionScope};
use datazen_driver_api::{ConnectionConfig, SshTunnelConfig};

use super::ServerConnections;
use crate::execution::secret_provider::ServerSecretVault;

const KEY: &[u8] = b"connection-catalog-test-key-32-bytes!";

fn rig() -> (ServerConnections, Arc<crate::storage::AdminStore>) {
    let store = Arc::new(crate::storage::AdminStore::open_in_memory().expect("store"));
    store.migrate().expect("migrate");
    let vault = Arc::new(ServerSecretVault::new(store.clone(), KEY.to_vec()).expect("vault"));
    (ServerConnections::new(store.clone(), vault), store)
}

fn scope(org: &str) -> ConnectionScope {
    ConnectionScope::new(org)
}

fn config(id: &str, password: Option<&str>, group: Option<&str>) -> ConnectionConfig {
    ConnectionConfig {
        id: id.into(),
        name: format!("Connection {id}"),
        database_type: "postgresql".into(),
        host: Some("db.internal".into()),
        port: Some(5432),
        database: Some("app".into()),
        schema: None,
        username: Some("alice".into()),
        password: password.map(str::to_string),
        ssl_mode: Default::default(),
        connection_timeout: 30,
        max_pool_size: 5,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: group.map(str::to_string),
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

#[tokio::test]
async fn save_then_get_round_trips_the_form_with_plaintext_credentials() {
    let (repo, store) = rig();
    repo.save(&scope("org-1"), config("c1", Some("s3cret"), None))
        .await
        .expect("save");
    let fetched = repo
        .get(&scope("org-1"), "c1")
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(fetched.password.as_deref(), Some("s3cret"), "表单逐字回显");
    // 明文不入库：行内 config_json 与 stored_secret 都不含明文。
    let conn = store.lock_for_adapter().expect("conn");
    let raw: String = conn
        .query_row(
            "SELECT config_json FROM connection_catalog WHERE organization_id='org-1' AND connection_id='c1'",
            [],
            |row| row.get(0),
        )
        .expect("row");
    assert!(!raw.contains("s3cret"), "净化投影不得含明文：{raw}");
    let sealed: String = conn
        .query_row(
            "SELECT ciphertext_b64 FROM stored_secret_version WHERE organization_id='org-1'",
            [],
            |row| row.get(0),
        )
        .expect("sealed row");
    drop(conn);
    assert!(!sealed.contains("s3cret"), "密封盒是密文");
}

#[tokio::test]
async fn credentials_without_secrets_skip_the_vault() {
    let (repo, store) = rig();
    repo.save(&scope("org-1"), config("c1", None, None))
        .await
        .expect("save");
    {
        let conn = store.lock_for_adapter().expect("conn");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM stored_secret WHERE organization_id='org-1'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 0, "无敏感字段不进保险库");
    }
    let fetched = repo
        .get(&scope("org-1"), "c1")
        .await
        .expect("get")
        .expect("exists");
    assert!(fetched.password.is_none());
}

#[tokio::test]
async fn ssh_nested_secrets_are_sealed_too() {
    let (repo, store) = rig();
    let mut with_ssh = config("c1", None, None);
    with_ssh.ssh_tunnel = Some(SshTunnelConfig {
        enabled: true,
        host: "jump.internal".into(),
        port: 22,
        username: "ops".into(),
        auth_method: "password".into(),
        password: Some("ssh-pass".into()),
        private_key_path: None,
        passphrase: None,
        jump: None,
    });
    repo.save(&scope("org-1"), with_ssh).await.expect("save");
    let conn = store.lock_for_adapter().expect("conn");
    let raw: String = conn
        .query_row(
            "SELECT config_json FROM connection_catalog WHERE connection_id='c1'",
            [],
            |row| row.get(0),
        )
        .expect("row");
    assert!(!raw.contains("ssh-pass"), "嵌套 ssh 密码也要摘走：{raw}");
    drop(conn);
    let fetched = repo
        .get(&scope("org-1"), "c1")
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(
        fetched.ssh_tunnel.expect("ssh").password.as_deref(),
        Some("ssh-pass"),
        "嵌套明文逐字拼回"
    );
}

#[tokio::test]
async fn organization_scoping_hides_other_tenants_cm05() {
    let (repo, _store) = rig();
    repo.save(&scope("org-1"), config("c1", Some("x"), None))
        .await
        .expect("save");
    assert!(
        repo.get(&scope("org-2"), "c1")
            .await
            .expect("get")
            .is_none(),
        "读越界=不存在"
    );
    assert!(repo.list(&scope("org-2")).await.expect("list").is_empty());
    let delete = repo.delete(&scope("org-2"), "c1").await;
    assert!(
        matches!(
            delete,
            Err(datazen::connection::domain::catalog::ConnectionRepositoryError::NotFound(_))
        ),
        "写越界=NotFound"
    );
}

#[tokio::test]
async fn delete_removes_the_row_and_its_sealed_material() {
    let (repo, store) = rig();
    repo.save(&scope("org-1"), config("c1", Some("s3cret"), None))
        .await
        .expect("save");
    repo.delete(&scope("org-1"), "c1").await.expect("delete");
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
async fn reorder_puts_mentioned_first_then_keeps_relative_order() {
    let (repo, _store) = rig();
    for id in ["a", "b", "c", "d"] {
        repo.save(&scope("org-1"), config(id, None, None))
            .await
            .expect("save");
    }
    repo.reorder(&scope("org-1"), vec!["d".into(), "zzz".into(), "b".into()])
        .await
        .expect("reorder");
    let list = repo.list(&scope("org-1")).await.expect("list");
    let ids: Vec<String> = list.iter().map(|config| config.id.clone()).collect();
    assert_eq!(
        ids,
        vec!["d", "b", "a", "c"],
        "提及者在前，未提及保持原序垫后，未知忽略"
    );
}

#[tokio::test]
async fn touch_only_advances_the_timestamp_column() {
    let (repo, store) = rig();
    repo.save(&scope("org-1"), config("c1", Some("x"), None))
        .await
        .expect("save");
    repo.touch_last_connected_at(&scope("org-1"), "c1")
        .await
        .expect("touch");
    let conn = store.lock_for_adapter().expect("conn");
    let stamp: Option<String> = conn
        .query_row(
            "SELECT last_connected_at FROM connection_catalog WHERE connection_id='c1'",
            [],
            |row| row.get(0),
        )
        .expect("stamp");
    assert!(stamp.is_some(), "窄更新只动时间戳列");
    drop(conn);
    let missing = repo.touch_last_connected_at(&scope("org-1"), "nope").await;
    assert!(matches!(
        missing,
        Err(datazen::connection::domain::catalog::ConnectionRepositoryError::NotFound(_))
    ));
}

#[tokio::test]
async fn groups_union_persisted_custom_and_connection_rows() {
    let (repo, _store) = rig();
    repo.save(&scope("org-1"), config("c1", None, Some("Prod")))
        .await
        .expect("save");
    repo.save_groups(
        &scope("org-1"),
        vec!["Custom".into(), "Prod".into(), "".into()],
    )
    .await
    .expect("save groups");
    let groups = repo.groups(&scope("org-1")).await.expect("groups");
    assert_eq!(groups, vec!["Custom", "Prod"], "并集去重排序，空名过滤");
    // 整表替换：再存一次只剩 Custom。
    repo.save_groups(&scope("org-1"), vec!["Custom".into()])
        .await
        .expect("save groups");
    let groups = repo.groups(&scope("org-1")).await.expect("groups");
    assert_eq!(groups, vec!["Custom", "Prod"], "连接行上的组仍在并集里");
}

#[tokio::test]
async fn save_replaces_the_form_and_rotates_the_sealed_material() {
    let (repo, store) = rig();
    repo.save(&scope("org-1"), config("c1", Some("old-pass"), None))
        .await
        .expect("save");
    repo.save(&scope("org-1"), config("c1", Some("new-pass"), None))
        .await
        .expect("save");
    let fetched = repo
        .get(&scope("org-1"), "c1")
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(
        fetched.password.as_deref(),
        Some("new-pass"),
        "轮换后取到新明文"
    );
    let conn = store.lock_for_adapter().expect("conn");
    let revisions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM stored_secret_version WHERE organization_id='org-1'",
            [],
            |row| row.get(0),
        )
        .expect("versions");
    assert_eq!(revisions, 2, "同一 secret_ref 下版本递增，不覆盖");
}
