//! Cluster openers, with the plaintext-fallback arm.

use super::parse::connect_with_timeout;
use super::plan::TlsPlan;
use super::sentinel::plaintext_url;
use super::standalone::PREFER_TLS_PROBE;
use super::tls::load_tls_certificates;
use super::tls::tls_mode_for_plan;
use datazen_driver_api::DriverError;
use redis::cluster::ClusterClient;
use redis::cluster_async::ClusterConnection;
use std::time::Duration;

async fn open_cluster_conn(
    node_urls: &[String],
    username: Option<&str>,
    password: Option<&str>,
    tls: &TlsPlan,
    timeout: Duration,
) -> Result<ClusterConnection, DriverError> {
    let mut builder = ClusterClient::builder(node_urls.to_vec());
    if let Some(user) = username {
        builder = builder.username(user.to_string());
    }
    if let Some(pass) = password {
        builder = builder.password(pass.to_string());
    }
    if let Some(mode) = tls_mode_for_plan(tls) {
        builder = builder.tls(mode);
        if let Some(certs) = load_tls_certificates(tls)? {
            builder = builder.certs(certs);
        }
    }
    let client = builder
        .build()
        .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
    connect_with_timeout(timeout, client.get_async_connection()).await
}

pub(crate) async fn open_cluster_conn_with_fallback(
    node_urls: &[String],
    username: Option<&str>,
    password: Option<&str>,
    tls: &TlsPlan,
    timeout: Duration,
) -> Result<ClusterConnection, DriverError> {
    let tls_timeout = if tls.prefer_fallback {
        timeout.min(PREFER_TLS_PROBE)
    } else {
        timeout
    };
    match open_cluster_conn(node_urls, username, password, tls, tls_timeout).await {
        Ok(conn) => Ok(conn),
        Err(DriverError::ConnectionFailed(_)) if tls.prefer_fallback => {
            let plain_urls: Vec<String> = node_urls.iter().map(|u| plaintext_url(u)).collect();
            open_cluster_conn(
                &plain_urls,
                username,
                password,
                &TlsPlan::plaintext(),
                timeout,
            )
            .await
        }
        Err(e) => Err(e),
    }
}
