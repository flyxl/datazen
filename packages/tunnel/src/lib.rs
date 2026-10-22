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
//! | [`ssh_known_hosts`] | the SSH known-hosts **rules** (fingerprint / TOFU decision) |
//! | [`known_hosts_repository`] | the SSH known-hosts **storage port** the host implements |
//! | [`crypto`] | the process-wide rustls provider the TLS legs need |
//!
//! The configuration types (`SshTunnelConfig`, `HttpProxyTunnelConfig`,
//! `WebSocketTunnelConfig`) and the error type (`DriverError`) belong to
//! `datazen-driver-api`: the tunnel crates read a driver's contract, it does
//! not depend on the tunnel.
//!
//! # What this crate deliberately does **not** do
//!
//! It does not read the machine's files. The SSH known-hosts store used to be
//! `std::fs` calls inside this crate; the **rules** stay here, the **storage**
//! is the [`KnownHostsRepository`] port: the desktop host implements it over
//! `{appData}/ssh_known_hosts.json`, a future web form over server-side
//! storage. Same split for tunnel definitions: the **port**
//! (`datazen_platform_api::ports::TunnelRepository`) lives in the contract
//! crate, the desktop host implements it over its encrypted `tunnels.json`.

mod crypto;
mod http_proxy;
mod known_hosts_repository;
mod ssh;
mod ssh_known_hosts;
mod websocket;

pub use crypto::install_default_crypto_provider;
pub use http_proxy::{verify_upstream as verify_http_proxy_upstream, HttpProxyTunnel};
pub use known_hosts_repository::{KnownHostsRepository, KnownHostsRepositoryError};
pub use ssh::SshTunnel;
pub use ssh_known_hosts::KnownHostEntry;
pub use websocket::{verify_upstream as verify_websocket_upstream, WebSocketTunnel};
