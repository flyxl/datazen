mod mapping;
mod types;
mod validation;

#[cfg(test)]
mod tests;

pub use mapping::{draft_to_saved_tunnel, empty_tunnel_draft, saved_tunnel_to_draft};
pub use types::{
    HttpProxyDraft, SshDraft, SshJumpDraft, TunnelDraft, TunnelDraftProblem, WebSocketDraft,
    DEFAULT_HTTP_PROXY_PORT, DEFAULT_SSH_PORT, DEFAULT_TUNNEL_TIMEOUT, SAVED_TUNNEL_KINDS,
};
pub use validation::{
    copy_tunnel_name, is_positive_port, is_tunnel_draft_valid, positive_port, timeout_seconds,
    tunnel_draft_problem,
};
