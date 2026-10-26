//! The desktop `TunnelRepository` port: the two tiers (list = summary without
//! secrets, get = the full definition), organization isolation (CM-05), and the
//! on-disk shape (still encrypted, still the same file).
//!
//! These tests pin what the port must **not** change: the list tier stays
//! secret-free, a foreign organization sees nothing at all, and the file on
//! disk keeps both its shape and its encryption.

use super::fixtures::*;
use datazen_driver_api::{HttpProxyTunnelConfig, SavedTunnel, TunnelKind};
use datazen_modules::tunnel::TunnelRepository;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::OrganizationId;
use datazen_platform_api::ports::scope::OrganizationScope;

/// The desktop's own organization (an installation-level constant).
fn local_scope() -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new(
        crate::platform::identity::LOCAL_ORGANIZATION_ID,
    ))
}

/// Some other organization — the desktop must not serve it.
fn foreign_scope() -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new("some-other-org"))
}

fn proxy_tunnel(id: &str) -> SavedTunnel {
    SavedTunnel {
        id: id.into(),
        name: "Proxy".into(),
        kind: TunnelKind::HttpProxy,
        ssh: None,
        http_proxy: Some(HttpProxyTunnelConfig {
            enabled: true,
            host: "proxy.internal".into(),
            port: 3128,
            scheme: "http".into(),
            username: Some("svc".into()),
            password: Some("secret-pw".into()),
            headers: None,
            connect_timeout_secs: 30,
        }),
        websocket: None,
    }
}

#[tokio::test]
async fn the_list_tier_carries_no_secret_and_the_get_tier_carries_the_definition() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    store.save_tunnel(proxy_tunnel("t1")).await.unwrap();

    let list = store.list(&local_scope()).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "t1");
    assert_eq!(list[0].name, "Proxy");
    assert_eq!(list[0].kind, TunnelKind::HttpProxy);
    let json = serde_json::to_value(&list[0]).unwrap();
    assert!(
        !json.to_string().contains("secret-pw"),
        "名单档不得带凭据: {json}"
    );
    // 摘要只有三个字段：id / name / kind（`kind` 的取值 `httpProxy` 是**值**，
    // 不是隧道的配置块——配置块不得出现在名单里）。
    assert_eq!(
        json.as_object().map(|o| o.len()),
        Some(3),
        "名单档只有三个字段: {json}"
    );
    assert!(
        json.get("httpProxy").is_none(),
        "名单档不得带配置块: {json}"
    );

    let one = store.get(&local_scope(), "t1").await.unwrap().unwrap();
    assert_eq!(
        one.http_proxy.as_ref().unwrap().password.as_deref(),
        Some("secret-pw"),
        "单条取用档必须给全量定义"
    );
}

#[tokio::test]
async fn a_foreign_organization_sees_nothing_and_cannot_write() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    store.save_tunnel(proxy_tunnel("t1")).await.unwrap();

    // 读路径：按「不存在」处理，**不**返回「无权限」（CM-05）。
    assert!(store.list(&foreign_scope()).await.unwrap().is_empty());
    assert!(store.get(&foreign_scope(), "t1").await.unwrap().is_none());
    let usage = store.usage(&foreign_scope(), "t1").await.unwrap();
    assert!(usage.connection_ids.is_empty());
    assert!(usage.connection_names.is_empty());

    // 写路径：越组织的写入按「不存在」处理。
    assert!(matches!(
        store.save(&foreign_scope(), proxy_tunnel("t2")).await,
        Err(PortError::NotFound(_))
    ));
    assert!(matches!(
        store.delete(&foreign_scope(), "t1").await,
        Err(PortError::NotFound(_))
    ));

    // 本组织的那条**没有**被动过。
    assert!(store.get(&local_scope(), "t1").await.unwrap().is_some());
    assert_eq!(store.list(&local_scope()).await.unwrap().len(), 1);
}

#[tokio::test]
async fn the_on_disk_file_stays_encrypted_and_keeps_its_shape() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    store.save_tunnel(proxy_tunnel("t1")).await.unwrap();

    let raw = std::fs::read_to_string(dir.path().join("tunnels.json")).unwrap();
    assert!(raw.contains("\"id\""), "落盘形状不变: {raw}");
    assert!(raw.contains("\"httpProxy\""), "落盘形状不变: {raw}");
    assert!(!raw.contains("secret-pw"), "代理口令必须加密落盘: {raw}");

    // 重开一次：读回来的是**明文**（解密发生在内存里），与端口化之前一致。
    let reopened = init_store_for_test(dir.path()).await;
    let one = reopened.get(&local_scope(), "t1").await.unwrap().unwrap();
    assert_eq!(
        one.http_proxy.as_ref().unwrap().password.as_deref(),
        Some("secret-pw")
    );
}

#[tokio::test]
async fn the_usage_projection_lists_the_referencing_connections() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let mut connection = sample_connection_with_ssh();
    connection.tunnel_id = Some("t1".into());
    store.save_connection(connection.clone()).await.unwrap();
    store.save_tunnel(proxy_tunnel("t1")).await.unwrap();

    let usage = store.usage(&local_scope(), "t1").await.unwrap();
    assert_eq!(usage.connection_ids, vec![connection.id.clone()]);
    assert_eq!(usage.connection_names, vec![connection.name.clone()]);

    // 空 id 不做扫面：`materialize_tunnel_refs` 把 "" 当「无隧道引用」。
    let empty = store.usage(&local_scope(), "").await.unwrap();
    assert!(empty.connection_ids.is_empty());
    assert!(empty.connection_names.is_empty());
}
