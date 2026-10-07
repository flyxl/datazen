//! Plan building and the live-connection openers for the three topologies (pub/sub included).

use super::cluster::open_cluster_conn_with_fallback;
use super::parse::non_empty;
use super::parse::opt_string;
use super::parse::parse_db_index;
use super::parse::parse_node_urls;
use super::parse::parse_sentinel_urls;
use super::parse::parse_tls;
use super::parse::parse_topology;
use super::plan::ClusterPlan;
use super::plan::ConnectionPlan;
use super::plan::RedisLiveConn;
use super::plan::SentinelPlan;
use super::plan::StandalonePlan;
use super::plan::TlsPlan;
use super::plan::Topology;
use super::sentinel::open_sentinel_conn;
use super::sentinel::open_sentinel_pubsub;
use super::sentinel::plaintext_sentinel_plan;
use super::sentinel::plaintext_url;
use super::standalone::open_standalone_conn_with_fallback;
use super::standalone::open_standalone_pubsub;
use super::standalone::open_standalone_pubsub_with_fallback;
use super::standalone::PREFER_TLS_PROBE;
use super::tls::build_node_url;
use datazen_driver_api::{ConnectionConfig, DriverError};
use std::time::Duration;

pub fn build_connection_plan(config: &ConnectionConfig) -> Result<ConnectionPlan, DriverError> {
    let opts = config.options.as_ref();
    let topology = parse_topology(opts);
    let tls = parse_tls(opts, &config.ssl_mode);
    let connect_timeout = Duration::from_secs(config.connection_timeout.max(1) as u64);

    match topology {
        Topology::Standalone => {
            let host = config.host.as_deref().unwrap_or(super::DEFAULT_HOST);
            let port = config.port.unwrap_or(super::DEFAULT_PORT);
            let db_index = parse_db_index(config.database.as_deref())?;
            let url = build_node_url(&tls, host, port, None, None, Some(db_index));
            Ok(ConnectionPlan::Standalone(StandalonePlan {
                url,
                username: non_empty(config.username.as_deref()),
                password: non_empty(config.password.as_deref()),
                tls,
                db_index,
                connect_timeout,
            }))
        }
        Topology::Cluster => {
            let node_urls = parse_node_urls(opts, config, &tls, None)?;
            if node_urls.is_empty() {
                return Err(DriverError::InvalidConfig(
                    "cluster topology requires at least one cluster node".into(),
                ));
            }
            Ok(ConnectionPlan::Cluster(ClusterPlan {
                node_urls,
                username: non_empty(config.username.as_deref()),
                password: non_empty(config.password.as_deref()),
                tls,
                connect_timeout,
            }))
        }
        Topology::Sentinel => {
            let master_name = opt_string(opts, "sentinelMasterName").ok_or_else(|| {
                DriverError::InvalidConfig("sentinel topology requires sentinelMasterName".into())
            })?;
            let sentinel_password = opt_string(opts, "sentinelNodePassword");
            let sentinel_urls =
                parse_sentinel_urls(opts, config, &tls, sentinel_password.as_deref())?;
            if sentinel_urls.is_empty() {
                return Err(DriverError::InvalidConfig(
                    "sentinel topology requires at least one sentinel node".into(),
                ));
            }
            Ok(ConnectionPlan::Sentinel(SentinelPlan {
                sentinel_urls,
                master_name,
                sentinel_password,
                username: non_empty(config.username.as_deref()),
                password: non_empty(config.password.as_deref()),
                tls,
                db_index: parse_db_index(config.database.as_deref())?,
                connect_timeout,
            }))
        }
    }
}

/// Open a dedicated Pub/Sub connection for SUBSCRIBE / PSUBSCRIBE.
///
/// Cluster topology uses the first seed node as a standalone client (Pub/Sub is
/// node-local on Cluster).
pub async fn open_pubsub_connection(
    plan: &ConnectionPlan,
) -> Result<redis::aio::PubSub, DriverError> {
    match plan {
        ConnectionPlan::Standalone(p) => {
            open_standalone_pubsub_with_fallback(
                &p.url,
                &p.tls,
                p.username.as_deref(),
                p.password.as_deref(),
                p.connect_timeout,
            )
            .await
        }
        ConnectionPlan::Cluster(p) => {
            let url = p.node_urls.first().ok_or_else(|| {
                DriverError::InvalidConfig(
                    "cluster topology requires at least one cluster node".into(),
                )
            })?;
            let tls_timeout = if p.tls.prefer_fallback {
                p.connect_timeout.min(PREFER_TLS_PROBE)
            } else {
                p.connect_timeout
            };
            let mut attempt = open_standalone_pubsub(
                url,
                &p.tls,
                p.username.as_deref(),
                p.password.as_deref(),
                tls_timeout,
            )
            .await;
            if matches!(attempt, Err(DriverError::ConnectionFailed(_))) && p.tls.prefer_fallback {
                attempt = open_standalone_pubsub(
                    &plaintext_url(url),
                    &TlsPlan::plaintext(),
                    p.username.as_deref(),
                    p.password.as_deref(),
                    p.connect_timeout,
                )
                .await;
            }
            attempt
        }
        ConnectionPlan::Sentinel(p) => {
            let mut attempt = open_sentinel_pubsub(p, p.connect_timeout).await;
            if matches!(attempt, Err(DriverError::ConnectionFailed(_))) && p.tls.prefer_fallback {
                attempt =
                    open_sentinel_pubsub(&plaintext_sentinel_plan(p), p.connect_timeout).await;
            }
            attempt
        }
    }
}

pub async fn open_live_conn(plan: &ConnectionPlan) -> Result<RedisLiveConn, DriverError> {
    match plan {
        ConnectionPlan::Standalone(p) => {
            let connection = open_standalone_conn_with_fallback(
                &p.url,
                &p.tls,
                p.username.as_deref(),
                p.password.as_deref(),
                p.connect_timeout,
            )
            .await?;
            Ok(RedisLiveConn::Standalone(connection))
        }
        ConnectionPlan::Cluster(p) => {
            let connection = open_cluster_conn_with_fallback(
                &p.node_urls,
                p.username.as_deref(),
                p.password.as_deref(),
                &p.tls,
                p.connect_timeout,
            )
            .await?;
            Ok(RedisLiveConn::Cluster(connection))
        }
        ConnectionPlan::Sentinel(p) => {
            let mut attempt = open_sentinel_conn(p, p.connect_timeout).await;
            if matches!(attempt, Err(DriverError::ConnectionFailed(_))) && p.tls.prefer_fallback {
                attempt = open_sentinel_conn(&plaintext_sentinel_plan(p), p.connect_timeout).await;
            }
            let (client, connection) = attempt?;
            Ok(RedisLiveConn::Sentinel { client, connection })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::RedisDriver;
    use datazen_driver_api::{DatabaseDriver, SslMode};

    fn config(host: Option<&str>, port: Option<u16>) -> ConnectionConfig {
        ConnectionConfig {
            id: "cfg".into(),
            name: "n".into(),
            database_type: "redis".into(),
            host: host.map(str::to_string),
            port,
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: Default::default(),
            connection_timeout: 5,
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

    fn standalone_url(config: &ConnectionConfig) -> String {
        match build_connection_plan(config).expect("connection plan") {
            ConnectionPlan::Standalone(plan) => plan.url,
            other => panic!("expected a standalone plan, got {other:?}"),
        }
    }

    /// 反漂移闸：`default_host()`/`default_port()` 是宿主做端点物理身份摘要时
    /// 唯一的「默认 host/port」来源。它一旦与 `build_connection_plan` /
    /// `parse_node_urls` 实际拨号的地址分家，省略 host 的连接与显式写全 host
    /// 的连接就会算出两个不同的 service_key，自覆盖在 admission 静默漏判。
    /// 这里不测常量本身，只测「声明 == 实拨」。
    #[test]
    fn the_declared_defaults_are_exactly_what_connect_dials() {
        let driver = RedisDriver::new();
        let declared_host = driver.default_host().expect("redis has an implicit host");
        let declared_port = driver.default_port().expect("redis has an implicit port");

        assert_eq!(declared_host, crate::connect::DEFAULT_HOST);
        assert_eq!(declared_port, crate::connect::DEFAULT_PORT);

        let hostless = config(None, None);
        let tls = parse_tls(None, &hostless.ssl_mode);
        let expected = build_node_url(&tls, declared_host, declared_port, None, None, Some(0));
        assert_eq!(
            standalone_url(&hostless),
            expected,
            "default_host()/default_port() must be what build_connection_plan dials"
        );

        // 集群/哨兵默认节点走parse_node_urls，同一条默认ing 规则必须一致。
        let nodes = parse_node_urls(None, &hostless, &parse_tls(None, &hostless.ssl_mode), None)
            .expect("node urls");
        assert_eq!(
            nodes,
            vec![build_node_url(
                &tls,
                declared_host,
                declared_port,
                None,
                None,
                None
            )],
            "default_host()/default_port() must be what parse_node_urls dials"
        );

        let explicit = standalone_url(&config(Some("cache-a.example.com"), Some(6380)));
        assert!(
            explicit.starts_with("redis://cache-a.example.com:6380"),
            "{explicit}"
        );
    }
}
