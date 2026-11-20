//! React `windows/connection/home/` ports, one module per React file.
//!
//! The A1-era `views/connections_home.rs` still owns the composed landing page
//! and its pinned `connections-*` keys. These modules extract the card list,
//! hero and MCP promo bar into standalone, host-wireable views; the host wiring
//! that feeds them live store data is a later task, so the reusable items carry
//! per-item `#[allow(dead_code)]` with that removal stage noted.

pub(crate) mod connection_card_list;
pub(crate) mod home_hero;
pub(crate) mod mcp_promo_bar;
