//! Form-neutral tunnel mechanisms and the tunnel domain boundary.
//!
//! The module owns the tunnel repository ports and the local-leg mechanisms.
//! Host infrastructure supplies tunnel definitions and known-host persistence;
//! this module never reads files or host state directly.
//!
//! The form-neutral use case [`test_saved_tunnel`] (the tunnel-management
//! UI's "test tunnel" probe) also lives here, so the desktop command shell
//! and a future web handler share one copy (B2 tunnel knife).

mod crypto;
pub mod domain;
mod http_proxy;
pub mod probe;
mod ssh;
mod ssh_known_hosts;
#[cfg(test)]
pub(crate) mod test_support;
mod websocket;

pub use crypto::install_default_crypto_provider;
pub use domain::{KnownHostEntry, KnownHostsRepository, TunnelRepository};
pub use http_proxy::{verify_upstream as verify_http_proxy_upstream, HttpProxyTunnel};
pub use probe::test_saved_tunnel;
pub use ssh::SshTunnel;
pub use websocket::{verify_upstream as verify_websocket_upstream, WebSocketTunnel};
