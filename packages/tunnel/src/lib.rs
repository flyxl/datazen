//! Form-neutral tunnel mechanisms.
//!
//! A database connection that goes through a jump host, an HTTP `CONNECT`
//! proxy or a WebSocket relay has two halves: the **local leg** (bind a
//! loopback listener, forward every inbound connection to the upstream) and the
//! **form leg** (where the tunnel is configured, stored and started from).
//!
//! This crate owns the local leg and nothing else. It has no Tauri dependency,
//! no desktop session, no store and no IPC: the desktop host and the future
//! web form drive the very same code, which is the whole point of keeping the
//! mechanisms here instead of inside `src-tauri`.
//!
//! | module | what it does |
//! |--------|--------------|
//! | [`http_proxy`] | HTTP `CONNECT` proxy tunnel (`https://` upstream included) |
//! | [`websocket`] | WebSocket relay tunnel (`ws://` / `wss://` upstream included) |
//! | [`ssh`] | SSH forwarding tunnel with TOFU host-key verification |
//! | [`ssh_known_hosts`] | the SSH known-hosts store the SSH tunnel writes |
//! | [`crypto`] | the process-wide rustls provider the TLS legs need |
//!
//! The configuration types (`SshTunnelConfig`, `HttpProxyTunnelConfig`,
//! `WebSocketTunnelConfig`) and the error type (`DriverError`) belong to
//! `datazen-driver-api`: the tunnel crates read a driver's contract, it does
//! not depend on the tunnel.

mod crypto;
mod http_proxy;
mod ssh;
mod ssh_known_hosts;
mod websocket;

pub use crypto::install_default_crypto_provider;
pub use http_proxy::{verify_upstream as verify_http_proxy_upstream, HttpProxyTunnel};
pub use ssh::SshTunnel;
pub use websocket::{verify_upstream as verify_websocket_upstream, WebSocketTunnel};
