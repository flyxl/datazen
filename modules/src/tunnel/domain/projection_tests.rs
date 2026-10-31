//! Tunnel **summary / usage** 投影测试（`TunnelRepository::list` / `usage` 的映射）。
//!
//! 自桌面 `commands/tunnel_summary_tests.rs` 整体归位（B2 隧道域刀）：映射本体
//! 已收进本目录的 [`projection`](super)（[`summarize`] / [`usage_from_connections`]），
//! 测试跟着映射走，断言逐字保留。替身 [`MemoryTunnelStore`] 的 `list` / `usage`
//! 就是调那两条共享投影，所以这里钉的是实现方生产路径会用的那一份映射。
//!
//! 桌面侧保留的是 C4 适配器烟测（真实命令入口 + `DesktopRepository`），见
//! `apps/desktop/src-tauri/src/commands/tunnel.rs` 的测试模块。

use super::*;
use crate::shared::id::OrganizationId;
use crate::shared::scope::OrganizationScope;
use crate::tunnel::domain::TunnelRepository;
use crate::tunnel::test_support::MemoryTunnelStore;
use datazen_driver_api::{SslMode, TunnelKind};

fn scope() -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new("org-1"))
}

const SSH_PASSWORD: &str = "ssh-password-secret";
const SSH_PASSPHRASE: &str = "ssh-passphrase-secret";
const SSH_HOST: &str = "bastion.internal";

fn saved_ssh_tunnel(id: &str, name: &str) -> SavedTunnel {
    SavedTunnel {
        id: id.into(),
        name: name.into(),
        kind: TunnelKind::Ssh,
        ssh: Some(datazen_driver_api::SshTunnelConfig {
            enabled: true,
            host: SSH_HOST.into(),
            port: 2222,
            username: "ubuntu".into(),
            auth_method: "password".into(),
            password: Some(SSH_PASSWORD.into()),
            private_key_path: None,
            passphrase: Some(SSH_PASSPHRASE.into()),
            jump: None,
        }),
        http_proxy: None,
        websocket: None,
    }
}

/// 连接定义夹具：只填反向投影读得到的字段（id / name / tunnel_id）。
fn sample_connection(id: &str, name: &str, tunnel_id: Option<&str>) -> ConnectionConfig {
    ConnectionConfig {
        id: id.into(),
        name: name.into(),
        database_type: "postgres".into(),
        host: Some("localhost".into()),
        port: Some(5432),
        database: Some("app".into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::Prefer,
        connection_timeout: 30,
        max_pool_size: 10,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: tunnel_id.map(str::to_string),
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

fn assert_no_secret_leak(json: &str) {
    for needle in [
        SSH_PASSWORD,
        SSH_PASSPHRASE,
        SSH_HOST,
        "ubuntu",
        "password",
        "passphrase",
        "authToken",
        "auth_token",
        "privateKeyPath",
    ] {
        assert!(
            !json.contains(needle),
            "IPC payload leaked `{needle}`: {json}"
        );
    }
}

#[tokio::test]
async fn summaries_drop_every_tunnel_secret() {
    let store = MemoryTunnelStore::new();
    store.save_tunnel(saved_ssh_tunnel("t1", "Prod Bastion"));

    let summaries = store.list(&scope()).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "t1");
    assert_eq!(summaries[0].name, "Prod Bastion");
    assert_eq!(summaries[0].kind, TunnelKind::Ssh);

    let json = serde_json::to_string(&summaries).unwrap();
    assert_no_secret_leak(&json);
    let value: serde_json::Value = serde_json::to_value(&summaries).unwrap();
    assert_eq!(
        value,
        serde_json::json!([{ "id": "t1", "name": "Prod Bastion", "kind": "ssh" }]),
        "the summary must expose exactly id/name/kind"
    );
}

#[tokio::test]
async fn usage_lists_referencing_connections_in_order() {
    let store = MemoryTunnelStore::new();
    store.save_tunnel(saved_ssh_tunnel("t1", "Prod Bastion"));
    store.save_tunnel(saved_ssh_tunnel("t2", "Other Bastion"));

    store.add_connection(sample_connection("conn-hit", "Hit", Some("t1")));
    store.add_connection(sample_connection("conn-other", "Other", Some("t2")));

    let usage = store.usage(&scope(), "t1").await.unwrap();
    assert_eq!(usage.connection_ids, vec!["conn-hit".to_string()]);
    assert_eq!(usage.connection_names, vec!["Hit".to_string()]);

    let json = serde_json::to_string(&usage).unwrap();
    assert_no_secret_leak(&json);

    // Miss path: a stored tunnel nobody references returns empty arrays.
    let miss = store.usage(&scope(), "t2-missing").await.unwrap();
    assert!(miss.connection_ids.is_empty());
    assert!(miss.connection_names.is_empty());

    let miss_unknown = store.usage(&scope(), "no-such-tunnel").await.unwrap();
    assert!(miss_unknown.connection_ids.is_empty());
    assert!(miss_unknown.connection_names.is_empty());
}

// ── [tester] independent verification additions ──────────────────

#[tokio::test]
async fn test_tester_summaries_are_empty_for_an_empty_store() {
    let store = MemoryTunnelStore::new();
    let summaries = store.list(&scope()).await.unwrap();
    assert!(
        summaries.is_empty(),
        "an empty store must project to an empty summary list"
    );
}

#[tokio::test]
async fn test_tester_summaries_keep_store_order_and_drop_secrets_for_every_kind() {
    use datazen_driver_api::{HttpProxyTunnelConfig, WebSocketTunnelConfig};

    let store = MemoryTunnelStore::new();
    store.save_tunnel(saved_ssh_tunnel("t1", "Bastion"));
    store.save_tunnel(SavedTunnel {
        id: "t2".into(),
        name: "Corp proxy".into(),
        kind: TunnelKind::HttpProxy,
        ssh: None,
        http_proxy: Some(HttpProxyTunnelConfig {
            enabled: true,
            host: "proxy.corp".into(),
            port: 8080,
            scheme: "http".into(),
            username: Some("proxy-user".into()),
            password: Some("proxy-password-secret".into()),
            headers: None,
            connect_timeout_secs: 30,
        }),
        websocket: None,
    });
    store.save_tunnel(SavedTunnel {
        id: "t3".into(),
        name: "Relay".into(),
        kind: TunnelKind::WebSocket,
        ssh: None,
        http_proxy: None,
        websocket: Some(WebSocketTunnelConfig {
            enabled: true,
            url: "wss://relay.corp/v1".into(),
            auth_token: Some("ws-auth-token-secret".into()),
            headers: None,
            connect_timeout_secs: 30,
            ping_interval_secs: 30,
            mode: "datazen_v1".into(),
        }),
    });

    let summaries = store.list(&scope()).await.unwrap();
    assert_eq!(
        summaries.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["t1", "t2", "t3"],
        "summaries must preserve store order"
    );
    assert_eq!(
        summaries.iter().map(|s| s.kind).collect::<Vec<_>>(),
        vec![
            TunnelKind::Ssh,
            TunnelKind::HttpProxy,
            TunnelKind::WebSocket
        ]
    );

    // Every kind's secrets must be gone, not just SSH's.
    let json = serde_json::to_string(&summaries).unwrap();
    for needle in [
        SSH_PASSWORD,
        SSH_PASSPHRASE,
        SSH_HOST,
        "proxy-password-secret",
        "proxy-user",
        "proxy.corp",
        "ws-auth-token-secret",
        "relay.corp",
    ] {
        assert!(!json.contains(needle), "summary leaked `{needle}`: {json}");
    }
}

#[tokio::test]
async fn test_tester_usage_is_positionally_aligned_across_multiple_hits() {
    let store = MemoryTunnelStore::new();
    store.save_tunnel(saved_ssh_tunnel("t1", "Bastion"));

    // Deliberately insert in an order where the ids and the names do not
    // sort alike, so a positional mix-up cannot cancel out.
    for (id, name) in [("c-b", "Bravo"), ("c-a", "Alpha"), ("c-c", "Charlie")] {
        store.add_connection(sample_connection(id, name, Some("t1")));
    }

    // Non-matching neighbours: one direct connection (`None`) and one on a
    // different tunnel.
    store.add_connection(sample_connection("c-direct", "Direct", None));
    store.add_connection(sample_connection("c-other", "Other", Some("t-other")));

    let usage = store.usage(&scope(), "t1").await.unwrap();
    assert_eq!(
        usage.connection_ids,
        vec!["c-b".to_string(), "c-a".to_string(), "c-c".to_string()],
        "ids must come back in store order"
    );
    assert_eq!(
        usage.connection_names,
        vec![
            "Bravo".to_string(),
            "Alpha".to_string(),
            "Charlie".to_string()
        ],
        "each name must stay aligned with its own id"
    );
    assert_eq!(usage.connection_ids.len(), usage.connection_names.len());
}

#[tokio::test]
async fn test_tester_usage_with_empty_id_matches_no_real_reference() {
    let store = MemoryTunnelStore::new();
    store.save_tunnel(saved_ssh_tunnel("t1", "Bastion"));

    store.add_connection(sample_connection("c-ref", "Referenced", Some("t1")));
    store.add_connection(sample_connection("c-direct", "Direct", None));

    // `None` must never match; an empty query must not sweep in every
    // direct connection (none of them carries an empty-string reference).
    let usage = store.usage(&scope(), "").await.unwrap();
    assert!(usage.connection_ids.is_empty(), "{usage:?}");
    assert!(usage.connection_names.is_empty(), "{usage:?}");
}

/// [tester] (e)#2 — the empty-id guard must actually exclude connections
/// whose reference is the empty string (this is what `materialize_tunnel_refs`
/// treats as "no reference"); the pre-fix code matched `Some("")`.
#[tokio::test]
async fn test_tester_usage_with_empty_id_excludes_empty_string_references() {
    let store = MemoryTunnelStore::new();

    store.add_connection(sample_connection("c-empty-ref", "Empty ref", Some("")));
    store.add_connection(sample_connection("c-real-ref", "Real ref", Some("t1")));

    let empty = store.usage(&scope(), "").await.unwrap();
    assert!(
        empty.connection_ids.is_empty() && empty.connection_names.is_empty(),
        "an empty query must not match an empty-string reference: {empty:?}"
    );

    // The genuine reference is still found by its own id.
    let hit = store.usage(&scope(), "t1").await.unwrap();
    assert_eq!(hit.connection_ids, vec!["c-real-ref".to_string()]);
}
