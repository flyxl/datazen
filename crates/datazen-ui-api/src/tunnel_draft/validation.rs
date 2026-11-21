use crate::SavedTunnelKind;

use super::types::{TunnelDraft, TunnelDraftProblem, DEFAULT_TUNNEL_TIMEOUT};

pub fn is_positive_port(value: &str) -> bool {
    if value.trim().is_empty() {
        return false;
    }
    match value.trim().parse::<f64>() {
        Ok(port) => port.fract() == 0.0 && port > 0.0 && port <= 65535.0,
        Err(_) => false,
    }
}

pub fn positive_port(value: &str, fallback: u16) -> u16 {
    if is_positive_port(value) {
        match value.trim().parse::<u16>() {
            Ok(port) => port,
            Err(_) => fallback,
        }
    } else {
        fallback
    }
}

pub fn timeout_seconds(value: &str) -> u32 {
    match value.parse::<f64>() {
        Ok(seconds) if seconds.is_finite() && seconds > 0.0 => seconds as u32,
        _ => DEFAULT_TUNNEL_TIMEOUT.parse::<u32>().unwrap_or(30),
    }
}

pub fn tunnel_draft_problem(draft: &TunnelDraft) -> Option<TunnelDraftProblem> {
    if draft.name.trim().is_empty() {
        return Some(TunnelDraftProblem::Name);
    }
    match draft.kind {
        SavedTunnelKind::Ssh => {
            if draft.ssh.host.trim().is_empty() {
                return Some(TunnelDraftProblem::SshHost);
            }
            if !is_positive_port(&draft.ssh.port) {
                return Some(TunnelDraftProblem::SshPort);
            }
            if draft.ssh.username.trim().is_empty() {
                return Some(TunnelDraftProblem::SshUsername);
            }
            if draft.jump_enabled() {
                if draft.ssh.jump.host.trim().is_empty() {
                    return Some(TunnelDraftProblem::SshJumpHost);
                }
                if !is_positive_port(&draft.ssh.jump.port) {
                    return Some(TunnelDraftProblem::SshJumpPort);
                }
                if draft.ssh.jump.username.trim().is_empty() {
                    return Some(TunnelDraftProblem::SshJumpUsername);
                }
            }
            None
        }
        SavedTunnelKind::HttpProxy => {
            if draft.http_proxy.host.trim().is_empty() {
                return Some(TunnelDraftProblem::HttpProxyHost);
            }
            if !is_positive_port(&draft.http_proxy.port) {
                return Some(TunnelDraftProblem::HttpProxyPort);
            }
            None
        }
        SavedTunnelKind::WebSocket => {
            if draft.websocket.url.trim().is_empty() {
                return Some(TunnelDraftProblem::WsUrl);
            }
            None
        }
    }
}

pub fn is_tunnel_draft_valid(draft: &TunnelDraft) -> bool {
    tunnel_draft_problem(draft).is_none()
}

pub fn copy_tunnel_name(name: &str, suffix: &str) -> String {
    format!("{name}{suffix}")
}

impl TunnelDraft {
    fn jump_enabled(&self) -> bool {
        self.ssh.jump.enabled
    }
}
