//! Tunnel entity IPC (independent of connection configs).

use super::error::{CmdExt, CommandError};
use super::AppState;
use datazen_driver_api::SavedTunnel;
use tauri::State;

pub(crate) async fn get_tunnels_impl(state: &AppState) -> Result<Vec<SavedTunnel>, CommandError> {
    Ok(state.store.get_tunnels().await)
}

pub(crate) async fn get_tunnel_impl(
    state: &AppState,
    id: String,
) -> Result<Option<SavedTunnel>, CommandError> {
    Ok(state.store.get_tunnel(&id).await)
}

pub(crate) async fn save_tunnel_impl(
    state: &AppState,
    tunnel: SavedTunnel,
) -> Result<(), CommandError> {
    tracing::info!(id = %tunnel.id, name = %tunnel.name, kind = ?tunnel.kind, "save_tunnel");
    state.store.save_tunnel(tunnel).await.cmd_err("save_tunnel")
}

pub(crate) async fn delete_tunnel_impl(
    state: &AppState,
    id: String,
) -> Result<(), CommandError> {
    tracing::info!(%id, "delete_tunnel");
    state.store.delete_tunnel(&id).await.cmd_err("delete_tunnel")
}

#[tauri::command]
pub async fn get_tunnels(state: State<'_, AppState>) -> Result<Vec<SavedTunnel>, CommandError> {
    get_tunnels_impl(&state).await
}

#[tauri::command]
pub async fn get_tunnel(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<SavedTunnel>, CommandError> {
    get_tunnel_impl(&state, id).await
}

#[tauri::command]
pub async fn save_tunnel(
    state: State<'_, AppState>,
    tunnel: SavedTunnel,
) -> Result<(), CommandError> {
    save_tunnel_impl(&state, tunnel).await
}

#[tauri::command]
pub async fn delete_tunnel(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    delete_tunnel_impl(&state, id).await
}
