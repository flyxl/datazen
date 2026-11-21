use super::{error_message, new_tunnel_id, TunnelInput, TunnelStoreState};
use crate::types::SavedTunnelSummary;
use crate::{SavedTunnel, TunnelUsage};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TunnelStoreEffect {
    SaveTunnel(SavedTunnel),
    DeleteTunnel(String),
    ForceLoad,
    GetUsage(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TunnelCreatePlan {
    pub tunnel: SavedTunnel,
    pub effects: Vec<TunnelStoreEffect>,
}

pub fn begin_load(state: &TunnelStoreState, force: bool) -> Option<TunnelStoreState> {
    if state.loaded && !force {
        return None;
    }
    Some(TunnelStoreState {
        loading: true,
        error: None,
        ..state.clone()
    })
}

pub fn settle_load(
    state: &TunnelStoreState,
    result: Result<Vec<SavedTunnelSummary>, String>,
    fallback: &str,
) -> TunnelStoreState {
    match result {
        Ok(summaries) => TunnelStoreState {
            summaries,
            loaded: true,
            loading: false,
            error: None,
        },
        Err(error) => TunnelStoreState {
            summaries: Vec::new(),
            loaded: true,
            loading: false,
            error: Some(error_message(Some(&error), fallback)),
        },
    }
}

pub fn plan_create(input: &TunnelInput, random_base36: &str) -> TunnelCreatePlan {
    let id = input
        .id
        .clone()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| new_tunnel_id(random_base36));
    let tunnel = SavedTunnel {
        id,
        name: input.name.clone(),
        kind: input.kind,
        ssh: input.ssh.clone(),
        http_proxy: input.http_proxy.clone(),
        websocket: input.websocket.clone(),
    };
    TunnelCreatePlan {
        tunnel: tunnel.clone(),
        effects: vec![
            TunnelStoreEffect::SaveTunnel(tunnel),
            TunnelStoreEffect::ForceLoad,
        ],
    }
}

pub fn plan_update(tunnel: &SavedTunnel) -> Vec<TunnelStoreEffect> {
    vec![
        TunnelStoreEffect::SaveTunnel(tunnel.clone()),
        TunnelStoreEffect::ForceLoad,
    ]
}

pub fn plan_remove(id: &str) -> Vec<TunnelStoreEffect> {
    vec![
        TunnelStoreEffect::DeleteTunnel(id.to_owned()),
        TunnelStoreEffect::ForceLoad,
    ]
}

pub fn plan_usage(id: &str) -> Vec<TunnelStoreEffect> {
    vec![TunnelStoreEffect::GetUsage(id.to_owned())]
}

pub fn usage_from_lists(connection_ids: Vec<String>, connection_names: Vec<String>) -> TunnelUsage {
    TunnelUsage {
        connection_ids,
        connection_names,
    }
}
