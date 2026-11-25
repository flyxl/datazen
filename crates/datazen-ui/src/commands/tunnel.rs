//! Port of `packages/frontend/src/commands/tunnel.ts` (T432) — the
//! saved-tunnel command family: the CRUD pair over `tunnels.json`,
//! the G9 secret-free summary read, the usage read and the
//! round-trip probe.
//!
//! ## What maps where
//!
//! - The seven `invoke` wrappers are [`TunnelMethod`] + the
//!   `*_request` shapers (the desktop adapter's command rail — the
//!   commands/ convention).
//! - `SavedTunnel` / its kind / the three leg configs are
//!   `datazen-ui-api`'s mirrors (`connection_types.rs` — the TS
//!   `ssh` leg is `SavedTunnelSshConfig`, field-identical to
//!   `SshTunnelConfig`; the mirror's own note).
//! - `SavedTunnelSummary` and `TunnelUsage` (driver-sdk types with
//!   no ui-api mirror yet) are this port's own contracts, verbatim:
//!   the summary is the **G9 projection** — name + kind only, so
//!   decrypted credentials never reach a list or picker (the
//!   no-secrets pin below).
//! - `getTunnel`'s `SavedTunnel | null` answer and `testTunnel`'s
//!   elapsed-milliseconds answer are the host's decode (the
//!   shapers only carry the requests).
//!
//! Known deltas: none — every payload is key-for-key with the React
//! wire.

// No production consumer yet (the tunnel settings track wires the
// faces); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use datazen_ui_api::SavedTunnel;

/// `SavedTunnelSummary` — the secret-free projection
/// `get_tunnel_summaries` answers. Lists and pickers only ever need
/// name + kind, so no credential field can appear (G9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTunnelSummary {
    pub id: String,
    pub name: String,
    pub kind: datazen_ui_api::SavedTunnelKind,
}

/// `TunnelUsage` — the connections referencing a saved tunnel
/// (`get_tunnel_usage`'s answer).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelUsage {
    pub connection_ids: Vec<String>,
    pub connection_names: Vec<String>,
}

/// The tunnel command family's invoke targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelMethod {
    GetTunnels,
    GetTunnelSummaries,
    GetTunnel,
    GetTunnelUsage,
    SaveTunnel,
    DeleteTunnel,
    TestTunnel,
}

impl TunnelMethod {
    /// The invoke command.
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::GetTunnels => "get_tunnels",
            Self::GetTunnelSummaries => "get_tunnel_summaries",
            Self::GetTunnel => "get_tunnel",
            Self::GetTunnelUsage => "get_tunnel_usage",
            Self::SaveTunnel => "save_tunnel",
            Self::DeleteTunnel => "delete_tunnel",
            Self::TestTunnel => "test_tunnel",
        }
    }
}

/// `getTunnels()` — full entities, credentials included; only the
/// edit form may call it (the comment's own rule).
pub fn get_tunnels_request() -> (TunnelMethod, Value) {
    (TunnelMethod::GetTunnels, json!({}))
}

/// `getTunnelSummaries()` — the G9 projection for lists and pickers.
pub fn get_tunnel_summaries_request() -> (TunnelMethod, Value) {
    (TunnelMethod::GetTunnelSummaries, json!({}))
}

/// `getTunnel(id)` — one full entity, `null` when absent (the host's
/// decode).
pub fn get_tunnel_request(id: &str) -> (TunnelMethod, Value) {
    (TunnelMethod::GetTunnel, json!({ "id": id }))
}

/// `getTunnelUsage(id)` — which connections reference the tunnel.
pub fn get_tunnel_usage_request(id: &str) -> (TunnelMethod, Value) {
    (TunnelMethod::GetTunnelUsage, json!({ "id": id }))
}

/// `saveTunnel(tunnel)` — upsert by id.
pub fn save_tunnel_request(tunnel: &SavedTunnel) -> (TunnelMethod, Value) {
    (TunnelMethod::SaveTunnel, json!({ "tunnel": tunnel }))
}

/// `deleteTunnel(id)`.
pub fn delete_tunnel_request(id: &str) -> (TunnelMethod, Value) {
    (TunnelMethod::DeleteTunnel, json!({ "id": id }))
}

/// `testTunnel(id, targetHost, targetPort)` — the round-trip probe
/// through a saved tunnel; the answer is elapsed milliseconds (the
/// host's decode).
pub fn test_tunnel_request(
    id: &str,
    target_host: &str,
    target_port: u16,
) -> (TunnelMethod, Value) {
    (
        TunnelMethod::TestTunnel,
        json!({ "id": id, "targetHost": target_host, "targetPort": target_port }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_names_are_the_react_invokes() {
        assert_eq!(TunnelMethod::GetTunnels.wire_name(), "get_tunnels");
        assert_eq!(TunnelMethod::GetTunnelSummaries.wire_name(), "get_tunnel_summaries");
        assert_eq!(TunnelMethod::GetTunnel.wire_name(), "get_tunnel");
        assert_eq!(TunnelMethod::GetTunnelUsage.wire_name(), "get_tunnel_usage");
        assert_eq!(TunnelMethod::SaveTunnel.wire_name(), "save_tunnel");
        assert_eq!(TunnelMethod::DeleteTunnel.wire_name(), "delete_tunnel");
        assert_eq!(TunnelMethod::TestTunnel.wire_name(), "test_tunnel");
    }

    #[test]
    fn the_reads_carry_only_their_keys() {
        // The list reads are bare invokes.
        let (method, payload) = get_tunnels_request();
        assert_eq!(method, TunnelMethod::GetTunnels);
        assert_eq!(payload, json!({}), "no payload keys");
        let (_, payload) = get_tunnel_summaries_request();
        assert_eq!(payload, json!({}), "no payload keys");
        // The id reads carry the one key.
        let (_, payload) = get_tunnel_request("t-1");
        assert_eq!(payload, json!({ "id": "t-1" }));
        let (_, payload) = get_tunnel_usage_request("t-1");
        assert_eq!(payload, json!({ "id": "t-1" }));
        let (_, payload) = delete_tunnel_request("t-1");
        assert_eq!(payload, json!({ "id": "t-1" }));
    }

    #[test]
    fn the_save_wraps_the_tunnel_under_its_key() {
        let tunnel = SavedTunnel {
            id: "t-1".to_owned(),
            name: "Prod Bastion".to_owned(),
            kind: datazen_ui_api::SavedTunnelKind::Ssh,
            ssh: Some(datazen_ui_api::SshTunnelConfig {
                enabled: true,
                host: "bastion.example.com".to_owned(),
                port: 22,
                username: "ops".to_owned(),
                auth_method: datazen_ui_api::SshAuthMethod::Password,
                password: Some("secret".to_owned()),
                private_key_path: None,
                passphrase: None,
                jump: None,
            }),
            http_proxy: None,
            websocket: None,
        };
        let (method, payload) = save_tunnel_request(&tunnel);
        assert_eq!(method, TunnelMethod::SaveTunnel);
        assert_eq!(payload["tunnel"]["id"], "t-1");
        assert_eq!(payload["tunnel"]["name"], "Prod Bastion");
        assert_eq!(payload["tunnel"]["kind"], "ssh");
        assert_eq!(payload["tunnel"]["ssh"]["host"], "bastion.example.com");
        assert_eq!(payload["tunnel"]["ssh"]["authMethod"], "password");
        // The credentials ride the full entity (the edit form's own
        // read); only the summary strips them.
        assert_eq!(payload["tunnel"]["ssh"]["password"], "secret");
        // The unused legs stay absent (skip_serializing_if).
        assert!(payload["tunnel"].get("httpProxy").is_none());
        assert!(payload["tunnel"].get("websocket").is_none());
    }

    #[test]
    fn the_probe_spells_the_triple() {
        let (method, payload) = test_tunnel_request("t-1", "db.internal", 5432);
        assert_eq!(method, TunnelMethod::TestTunnel);
        assert_eq!(payload, json!({ "id": "t-1", "targetHost": "db.internal", "targetPort": 5432 }));
    }

    #[test]
    fn the_summary_is_the_g9_secret_free_projection() {
        let summary = SavedTunnelSummary {
            id: "t-1".to_owned(),
            name: "Prod Bastion".to_owned(),
            kind: datazen_ui_api::SavedTunnelKind::HttpProxy,
        };
        let wire = serde_json::to_value(&summary).unwrap();
        assert_eq!(wire, json!({ "id": "t-1", "name": "Prod Bastion", "kind": "httpProxy" }));
        for key in [
            "password",
            "passphrase",
            "authToken",
            "privateKeyPath",
            "ssh",
            "httpProxy",
            "websocket",
        ] {
            assert!(
                wire.get(key).is_none(),
                "SavedTunnelSummary must not carry `{key}`: {wire}"
            );
        }
        // Round-trips.
        let back: SavedTunnelSummary = serde_json::from_value(wire).unwrap();
        assert_eq!(back, summary);
    }

    #[test]
    fn the_usage_round_trips_its_camel_case() {
        let usage = TunnelUsage {
            connection_ids: vec!["c1".to_owned(), "c2".to_owned()],
            connection_names: vec!["Alpha".to_owned(), "Beta".to_owned()],
        };
        let wire = serde_json::to_value(&usage).unwrap();
        assert_eq!(wire["connectionIds"], json!(["c1", "c2"]));
        assert_eq!(wire["connectionNames"], json!(["Alpha", "Beta"]));
        let back: TunnelUsage = serde_json::from_value(wire).unwrap();
        assert_eq!(back, usage);
    }
}
