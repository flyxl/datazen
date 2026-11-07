//! Sentinel openers, master election and the plaintext arm.

use super::client::sentinel_node_info;
use super::parse::connect_with_timeout;
use super::plan::SentinelPlan;
use super::plan::TlsPlan;
use datazen_driver_api::DriverError;
use redis::aio::MultiplexedConnection;
use redis::sentinel::{Sentinel, SentinelClient, SentinelServerType};
use std::time::Duration;

pub(crate) async fn open_sentinel_conn(
    plan: &SentinelPlan,
    timeout: Duration,
) -> Result<(SentinelClient, MultiplexedConnection), DriverError> {
    let node_info = sentinel_node_info(plan);
    let mut client = SentinelClient::build(
        plan.sentinel_urls.clone(),
        plan.master_name.clone(),
        Some(node_info),
        SentinelServerType::Master,
    )
    .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
    let connection = connect_with_timeout(timeout, client.get_async_connection()).await?;
    Ok((client, connection))
}

pub(crate) async fn open_sentinel_pubsub(
    plan: &SentinelPlan,
    timeout: Duration,
) -> Result<redis::aio::PubSub, DriverError> {
    let node_info = sentinel_node_info(plan);
    let mut sentinel = Sentinel::build(plan.sentinel_urls.clone())
        .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
    let master_client = connect_with_timeout(
        timeout,
        sentinel.async_master_for(&plan.master_name, Some(&node_info)),
    )
    .await?;
    connect_with_timeout(timeout, master_client.get_async_pubsub()).await
}

pub(crate) fn plaintext_url(url: &str) -> String {
    url.replacen("rediss://", "redis://", 1)
}

pub(crate) fn plaintext_sentinel_plan(plan: &SentinelPlan) -> SentinelPlan {
    let mut plain = plan.clone();
    plain.tls = TlsPlan::plaintext();
    plain.sentinel_urls = plain
        .sentinel_urls
        .iter()
        .map(|u| plaintext_url(u))
        .collect();
    plain
}
