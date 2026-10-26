//! Tunnel-owned repository ports and persisted vocabulary.

mod known_hosts;
mod tunnel_repository;

pub use known_hosts::{KnownHostEntry, KnownHostsRepository};
pub use tunnel_repository::TunnelRepository;
