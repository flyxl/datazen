//! Redis Cluster topology helpers (`CLUSTER NODES`, pinned-node connections).

use redis::aio::ConnectionLike;
use redis::AsyncCommands;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterNodeInfo {
    pub id: String,
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterNodesResult {
    pub nodes: Vec<ClusterNodeInfo>,
}

pub async fn cluster_nodes<C>(conn: &mut C) -> Result<ClusterNodesResult, String>
where
    C: AsyncCommands + ConnectionLike + Send,
{
    let raw: String = redis::cmd("CLUSTER")
        .arg("NODES")
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ClusterNodesResult {
        nodes: parse_cluster_nodes(&raw),
    })
}

pub fn parse_cluster_nodes(raw: &str) -> Vec<ClusterNodeInfo> {
    let mut nodes = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let id = parts[0].to_string();
        let addr = parts[1].split('@').next().unwrap_or(parts[1]).to_string();
        let flags = parts[2];
        let role = cluster_role_from_flags(flags);
        nodes.push(ClusterNodeInfo { id, addr, role });
    }
    nodes
}

fn cluster_role_from_flags(flags: &str) -> Option<String> {
    if flags.contains("master") {
        Some("master".into())
    } else if flags.contains("slave") || flags.contains("replica") {
        Some("replica".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cluster_nodes_extracts_addr_and_role() {
        let raw = "abc123 10.0.0.1:7000@17000 master - 0 1 connected 0-5460\n\
                   def456 10.0.0.2:7001@17001 slave abc123 0 1 connected\n";
        let nodes = parse_cluster_nodes(raw);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].id, "abc123");
        assert_eq!(nodes[0].addr, "10.0.0.1:7000");
        assert_eq!(nodes[0].role.as_deref(), Some("master"));
        assert_eq!(nodes[1].addr, "10.0.0.2:7001");
        assert_eq!(nodes[1].role.as_deref(), Some("replica"));
    }
}
