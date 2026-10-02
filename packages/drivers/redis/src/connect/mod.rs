//! Connection plan parsing and live Redis backends (standalone / cluster / sentinel + TLS).
//!
//! Split by responsibility; the original flat `connect` module's public paths are
//! preserved by the `pub use` re-exports below, so `crate::connect::<name>` (and the
//! `lib.rs` re-export) keeps resolving for every caller.

pub mod client;
pub mod cluster;
pub mod live;
pub mod parse;
pub mod plan;
pub mod sentinel;
pub mod standalone;
pub mod tls;

pub use client::looks_like_connection_loss;
pub use client::open_pinned_node_conn;
pub use live::build_connection_plan;
pub use live::open_live_conn;
pub use live::open_pubsub_connection;
pub use plan::ConnectionPlan;
pub use plan::RedisLiveConn;
pub use plan::TlsPlan;
pub use plan::Topology;

#[macro_export]
macro_rules! with_redis_conn {
    ($live:expr, |$c:ident| $body:expr) => {
        match $live {
            $crate::connect::RedisLiveConn::Standalone($c) => $body,
            $crate::connect::RedisLiveConn::Cluster($c) => $body,
            $crate::connect::RedisLiveConn::Sentinel { connection: $c, .. } => $body,
        }
    };
}

#[cfg(test)]
use datazen_driver_api::SslMode;
#[cfg(test)]
use serde_json::Map;

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::ConnectionConfig;
    use serde_json::json;
    use std::time::Duration;

    fn base_config() -> ConnectionConfig {
        ConnectionConfig {
            id: "id".into(),
            name: "Redis".into(),
            database_type: "redis".into(),
            host: Some("127.0.0.1".into()),
            port: Some(6379),
            database: Some("db0".into()),
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Disable,
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
    fn default_topology_is_standalone() {
        let plan = build_connection_plan(&base_config()).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => {
                assert_eq!(p.db_index, 0);
                assert!(p.url.starts_with("redis://"));
                assert!(!p.tls.enabled);
                assert!(!p.tls.prefer_fallback);
            }
            _ => panic!("expected standalone plan"),
        }
    }

    #[test]
    fn empty_password_is_treated_as_none() {
        // Servers without `requirepass` reject `HELLO AUTH default ""` with an
        // authentication error, so an empty stored password must be dropped
        // before building the connection plan.
        let mut config = base_config();
        config.password = Some(String::new());
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => assert_eq!(p.password, None),
            _ => panic!("expected standalone plan"),
        }

        config.password = Some("   ".to_string());
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => assert_eq!(p.password, None),
            _ => panic!("expected standalone plan"),
        }

        config.password = Some("secret".to_string());
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => assert_eq!(p.password.as_deref(), Some("secret")),
            _ => panic!("expected standalone plan"),
        }
    }

    #[test]
    fn cluster_topology_reads_nodes() {
        let mut config = base_config();
        let mut opts = Map::new();
        opts.insert("topology".into(), json!("cluster"));
        opts.insert(
            "clusterNodes".into(),
            json!(["10.0.0.1:7000", "10.0.0.2:7000"]),
        );
        config.options = Some(opts);
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Cluster(p) => {
                assert_eq!(p.node_urls.len(), 2);
                assert!(p.node_urls[0].contains("10.0.0.1:7000"));
                assert!(p.node_urls[1].contains("10.0.0.2:7000"));
            }
            _ => panic!("expected cluster plan"),
        }
    }

    #[test]
    fn tls_enabled_from_options_flag() {
        let mut config = base_config();
        let mut opts = Map::new();
        opts.insert("tls".into(), json!({ "enabled": true }));
        config.options = Some(opts);
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => {
                assert!(p.tls.enabled);
                assert!(!p.tls.prefer_fallback);
                assert!(p.url.starts_with("rediss://"));
            }
            _ => panic!("expected standalone plan"),
        }
    }

    #[test]
    fn tls_enabled_from_ssl_mode_require() {
        let mut config = base_config();
        config.ssl_mode = SslMode::Require;
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => {
                assert!(p.tls.enabled);
                assert!(!p.tls.prefer_fallback);
            }
            _ => panic!("expected standalone plan"),
        }
    }

    #[test]
    fn ssl_mode_prefer_attempts_tls_with_plaintext_fallback() {
        // Default connections carry ssl_mode=Prefer: build a rediss:// URL,
        // but mark the plan to fall back to plaintext when TLS fails.
        let mut config = base_config();
        config.ssl_mode = SslMode::Prefer;
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => {
                assert!(p.tls.enabled);
                assert!(p.tls.prefer_fallback);
                assert!(p.url.starts_with("rediss://"));
            }
            _ => panic!("expected standalone plan"),
        }
    }

    #[test]
    fn ssl_mode_prefer_with_explicit_tls_flag_never_downgrades() {
        // Wizard explicitly enabled TLS: prefer must not silently fall back.
        let mut config = base_config();
        config.ssl_mode = SslMode::Prefer;
        let mut opts = Map::new();
        opts.insert("tls".into(), json!({ "enabled": true }));
        config.options = Some(opts);
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => {
                assert!(p.tls.enabled);
                assert!(!p.tls.prefer_fallback);
            }
            _ => panic!("expected standalone plan"),
        }
    }

    #[test]
    fn connect_timeout_honors_config() {
        let mut config = base_config();
        config.connection_timeout = 7;
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => assert_eq!(p.connect_timeout, Duration::from_secs(7)),
            _ => panic!("expected standalone plan"),
        }
    }

    /// Previously `#[ignore = "requires local redis on 127.0.0.1:6379"]`.
    ///
    /// That reason is gone: the crate now starts its own `redis-server` on its
    /// own port (see `crate::live_server`), so there is nothing left to ignore
    /// and no fixed port to aim at. A stale ignore reason is worse than a
    /// missing test — it tells the next reader the coverage is missing when it
    /// is not.
    ///
    /// It runs against a *plaintext* server on purpose, because that is the
    /// condition both halves are about: a server that cannot do TLS.
    #[tokio::test]
    async fn live_prefer_falls_back_to_plaintext_and_require_refuses() {
        let server = crate::live_server::live_server_or_skip!();
        server.wait_until_serving().await.expect(
            "the harness starts its own server and reports loudly if that fails; a failure here \
             means the binary is installed but unusable",
        );

        let mut cfg = base_config();
        cfg.port = Some(server.port());
        cfg.ssl_mode = SslMode::Prefer;
        cfg.connection_timeout = 4;
        let plan = build_connection_plan(&cfg).unwrap();
        let t0 = std::time::Instant::now();
        open_live_conn(&plan).await.expect(
            "Prefer must reach a plaintext server, and this one is serving plaintext on the port \
             the harness just proved with PING",
        );
        // TLS probe (≤5s) fails against the plaintext server, then plaintext
        // fallback connects — total should stay well under a full timeout.
        assert!(t0.elapsed() < Duration::from_secs(12));

        let mut cfg = base_config();
        cfg.port = Some(server.port());
        cfg.ssl_mode = SslMode::Require;
        cfg.connection_timeout = 3;
        let plan = build_connection_plan(&cfg).unwrap();
        let t0 = std::time::Instant::now();
        let err = match open_live_conn(&plan).await {
            Ok(_) => panic!("TLS-required connect must fail against plaintext server"),
            Err(e) => e,
        };
        assert!(t0.elapsed() < Duration::from_secs(10));
        assert!(
            err.to_string().contains("timed out"),
            "Require against a plaintext server must report the handshake timing out, not a \
             generic failure that reads like a credentials problem: {err}"
        );
    }

    #[test]
    fn sentinel_requires_master_name_and_nodes() {
        let mut config = base_config();
        let mut opts = Map::new();
        opts.insert("topology".into(), json!("sentinel"));
        opts.insert("sentinelNodes".into(), json!(["127.0.0.1:26379"]));
        config.options = Some(opts);
        assert!(build_connection_plan(&config).is_err());

        let mut config = base_config();
        let mut opts = Map::new();
        opts.insert("topology".into(), json!("sentinel"));
        opts.insert("sentinelMasterName".into(), json!("mymaster"));
        config.options = Some(opts);
        assert!(build_connection_plan(&config).is_err());
    }

    #[test]
    fn sentinel_plan_parses() {
        let mut config = base_config();
        let mut opts = Map::new();
        opts.insert("topology".into(), json!("sentinel"));
        opts.insert("sentinelMasterName".into(), json!("mymaster"));
        opts.insert("sentinelNodes".into(), json!(["127.0.0.1:26379"]));
        config.options = Some(opts);
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Sentinel(p) => {
                assert_eq!(p.master_name, "mymaster");
                assert_eq!(p.sentinel_urls.len(), 1);
            }
            _ => panic!("expected sentinel plan"),
        }
    }

    #[test]
    fn standalone_url_omits_password() {
        let mut config = base_config();
        config.username = Some("alice".into());
        config.password = Some("s3cret".into());
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => {
                assert!(!p.url.contains("s3cret"), "{}", p.url);
                assert!(!p.url.contains('@'), "{}", p.url);
                assert_eq!(p.username.as_deref(), Some("alice"));
                assert_eq!(p.password.as_deref(), Some("s3cret"));
            }
            _ => panic!("expected standalone"),
        }
    }
}
