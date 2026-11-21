#[cfg(test)]
mod tests;

use crate::{ConnectionConfig, Translate};

pub const KEY_DELETE_FAILED: &str = "settings.tunnels.delete.failed";
pub const KEY_UNBIND_FAILED: &str = "settings.tunnels.delete.unbindFailed";

pub type GetConnections<'a> = dyn Fn() -> Result<Vec<ConnectionConfig>, String> + 'a;
pub type SaveConnection<'a> = dyn Fn(&ConnectionConfig) -> Result<(), String> + 'a;
pub type DeleteTunnel<'a> = dyn Fn(&str) -> Result<(), String> + 'a;

pub struct DeleteTunnelDeps<'a> {
    pub get_connections: &'a GetConnections<'a>,
    pub save_connection: &'a SaveConnection<'a>,
    pub delete_tunnel: &'a DeleteTunnel<'a>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeleteTunnelOutcome {
    Deleted {
        unbound_connection_ids: Vec<String>,
    },
    UnbindFailed {
        message: String,
        rolled_back: Vec<String>,
        rollback_failed: Vec<String>,
    },
    DeleteFailed {
        message: String,
        unbound_connection_ids: Vec<String>,
    },
}

fn message_of(error: String, fallback: String) -> String {
    if error.is_empty() {
        fallback
    } else {
        error
    }
}

pub fn unbind_connection_patch(connection: &ConnectionConfig) -> ConnectionConfig {
    let mut patched = connection.clone();
    patched.tunnel_id = None;
    patched.tunnel_kind = None;
    patched
}

pub fn delete_tunnel_and_unbind(
    tunnel_id: &str,
    deps: &DeleteTunnelDeps<'_>,
    t: &dyn Translate,
) -> DeleteTunnelOutcome {
    let connections = match (deps.get_connections)() {
        Ok(connections) => connections,
        Err(error) => {
            return DeleteTunnelOutcome::UnbindFailed {
                message: message_of(error, t.translate(KEY_DELETE_FAILED, &[])),
                rolled_back: Vec::new(),
                rollback_failed: Vec::new(),
            }
        }
    };

    let referencing = connections
        .iter()
        .filter(|connection| connection.tunnel_id.as_deref() == Some(tunnel_id));
    let mut unbound: Vec<ConnectionConfig> = Vec::new();

    for connection in referencing {
        if let Err(error) = (deps.save_connection)(&unbind_connection_patch(connection)) {
            let mut rolled_back = Vec::new();
            let mut rollback_failed = Vec::new();
            for done in unbound.iter().rev() {
                match (deps.save_connection)(done) {
                    Ok(()) => rolled_back.push(done.id.clone()),
                    Err(_) => rollback_failed.push(done.id.clone()),
                }
            }
            return DeleteTunnelOutcome::UnbindFailed {
                message: message_of(error, t.translate(KEY_UNBIND_FAILED, &[])),
                rolled_back,
                rollback_failed,
            };
        }
        unbound.push(connection.clone());
    }

    if let Err(error) = (deps.delete_tunnel)(tunnel_id) {
        return DeleteTunnelOutcome::DeleteFailed {
            message: message_of(error, t.translate(KEY_DELETE_FAILED, &[])),
            unbound_connection_ids: unbound.iter().map(|connection| connection.id.clone()).collect(),
        };
    }

    DeleteTunnelOutcome::Deleted {
        unbound_connection_ids: unbound.iter().map(|connection| connection.id.clone()).collect(),
    }
}
