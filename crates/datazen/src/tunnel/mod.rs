//! Form-neutral tunnel mechanisms and the tunnel domain boundary.
//!
//! The module owns the tunnel repository ports and the local-leg mechanisms.
//! Host infrastructure supplies tunnel definitions and known-host persistence;
//! this module never reads files or host state directly.
//!
//! The form-neutral use cases (the tunnel-management UI's "test tunnel" probe
//! and the full-list two-tier combination) live in [`application`], so the
//! desktop command shell and a future web handler share one copy.

pub mod application;
mod crypto;
pub mod domain;
/// WR-F5：宿主侧注入点（存储根 / 凭据 / 监听点）。机制共用，仅注入来源（web 形态即服务端宿主）。
pub mod host;
mod http_proxy;
mod ssh;
mod ssh_known_hosts;
#[cfg(test)]
pub(crate) mod test_support;
mod websocket;

pub use crypto::install_default_crypto_provider;
pub use domain::{KnownHostEntry, KnownHostsRepository, TunnelRepository};
pub use host::{
    HostInMemoryKnownHosts, HostInMemoryTunnelRoot, HostListenPoint, HostSecretRef,
    HostTunnelCredentialSource, HostTunnelSources, UnresolvedHostCredentials,
    HOST_SECRET_RESOLVER_GAP,
};
pub use http_proxy::{verify_upstream as verify_http_proxy_upstream, HttpProxyTunnel};
pub use ssh::SshTunnel;
pub use websocket::{verify_upstream as verify_websocket_upstream, WebSocketTunnel};
