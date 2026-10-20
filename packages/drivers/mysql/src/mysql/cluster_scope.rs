//! Cluster scope for a MySQL / MariaDB server.
//!
//! The scope says whether two logical databases live on the same physical
//! node: a healthy group-replication member and a healthy wsrep primary
//! share one identity, everything else falls back to the node identity or
//! fails closed when the cluster signals cannot be read.

use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ClusterScopeDecision {
    Cluster { scope: &'static str, id: String },
    NodeFallback,
    Unknown,
}

pub(crate) fn mysql_group_replication_scope(
    group_name: Option<&str>,
    member_state: Result<Option<&str>, ()>,
) -> ClusterScopeDecision {
    let Some(group_name) = group_name.map(str::trim).filter(|name| !name.is_empty()) else {
        return if group_name.is_some() {
            ClusterScopeDecision::NodeFallback
        } else {
            ClusterScopeDecision::Unknown
        };
    };

    match member_state {
        Err(()) => ClusterScopeDecision::Unknown,
        Ok(None) => ClusterScopeDecision::NodeFallback,
        Ok(Some(state)) if state.eq_ignore_ascii_case("ONLINE") => ClusterScopeDecision::Cluster {
            scope: "mysql-group-replication",
            id: group_name.to_string(),
        },
        Ok(Some(_)) => ClusterScopeDecision::Unknown,
    }
}

pub(crate) fn mysql_group_replication_plugin_active(
    plugin_statuses: Option<&HashMap<String, String>>,
) -> Option<bool> {
    let plugin_statuses = plugin_statuses?;
    match plugin_statuses
        .get("group_replication")
        .map(String::as_str)
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        None | Some("INACTIVE") | Some("DISABLED") => Some(false),
        Some("ACTIVE") => Some(true),
        Some(_) => None,
    }
}

pub(crate) fn mariadb_wsrep_scope(
    statuses: Option<&HashMap<String, String>>,
) -> ClusterScopeDecision {
    let Some(statuses) = statuses else {
        return ClusterScopeDecision::Unknown;
    };
    if statuses.is_empty() {
        return ClusterScopeDecision::NodeFallback;
    }

    let Some(state_uuid) = statuses
        .get("wsrep_cluster_state_uuid")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return ClusterScopeDecision::Unknown;
    };
    let healthy = statuses
        .get("wsrep_cluster_status")
        .is_some_and(|value| value.eq_ignore_ascii_case("Primary"))
        && statuses
            .get("wsrep_connected")
            .is_some_and(|value| value.eq_ignore_ascii_case("ON"))
        && statuses
            .get("wsrep_ready")
            .is_some_and(|value| value.eq_ignore_ascii_case("ON"));
    if healthy {
        ClusterScopeDecision::Cluster {
            scope: "mariadb-wsrep",
            id: state_uuid.to_string(),
        }
    } else {
        ClusterScopeDecision::Unknown
    }
}

pub(crate) fn canonical_database_identity(
    driver: &str,
    scope: &str,
    id: &str,
    database: &str,
) -> String {
    serde_json::json!({
        "driver": driver,
        "scope": scope,
        "id": id,
        "database": database,
    })
    .to_string()
}
