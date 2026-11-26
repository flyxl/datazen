//! The driver-sdk re-export members the aggregate still owed — ported from
//! React `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! React's `types/index.ts` re-exports nine tunnel names and seven catalog
//! names from `@datazen/driver-sdk`. Most of them already have Rust homes:
//! `TunnelKind`, `SshAuthMethod`, `HttpProxyTunnelConfig`,
//! `WebSocketTunnelConfig`, `SavedTunnel` and `SshTunnelConfig`'s family live
//! in this crate's `connection_types` (T128 — the `types/connection.ts`
//! facade port, whose own re-export groups are the same driver-sdk names);
//! `TableType`, `TableInfo`, `ColumnSchema`, `IndexInfo`, `ForeignKeyInfo`
//! and `TableSchema` live there too. The aggregate re-exports those from
//! `connection_types` and never redefines them.
//!
//! What was still missing — and what this module owns:
//!
//! | React name | Rust side found | decision here |
//! |---|---|---|
//! | `TunnelSource` | none (driver-sdk declares it; no Rust mirror anywhere) | port: closed enum |
//! | `SavedTunnelSummary` | none (secret-free projection, G9) | port |
//! | `TunnelUsage` | none | port |
//! | `TableOptionsSnapshot` | none in this crate — the driver-api twin exists (`schema_metadata.rs`, with `skip_serializing_if` on all three legs) | wasm-safe local mirror, same wire shape |
//! | `SavedTunnelSshConfig` | `connection_types::SshTunnelConfig` — T128's documented fold (field-identical; the TS split existed for a package boundary this crate does not have) | alias re-export from the aggregate, no second definition |
//!
//! `SavedTunnelSummary.kind` reuses `connection_types::SavedTunnelKind` — the
//! `Exclude<TunnelKind, 'none'>` narrowing T128 already named — so the two
//! tunnel vocabularies stay one vocabulary.
//!
//! Pure contracts — no IO, no state — so the module compiles for wasm32
//! exactly like its `chart_types` sibling.

use serde::{Deserialize, Serialize};

use crate::connection_types::SavedTunnelKind;

/// React `TunnelSource = 'none' | 'saved' | 'inline'` — which control owns
/// the tunnel reference in the connection form.
///
/// Single source of truth replacing the previous "kind select + saved
/// select" pair that overwrote each other (the driver-sdk comment's own
/// story): `none` → direct connection, `saved` → references a
/// `SavedTunnel` by `tunnelId`, `inline` → one-off tunnel configured in the
/// form itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TunnelSource {
    None,
    Saved,
    Inline,
}

impl TunnelSource {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Saved => "saved",
            Self::Inline => "inline",
        }
    }
}

/// React `SavedTunnelSummary` — the secret-free projection of `SavedTunnel`
/// returned by `get_tunnel_summaries`.
///
/// Lists and pickers only ever need name + kind, so decrypted credentials
/// never reach the webview for rendering (the driver-sdk G9 ruling the TS
/// comment records). `kind` reuses [`SavedTunnelKind`] — the
/// `Exclude<TunnelKind, 'none'>` narrowing — because a stored tunnel is
/// always one of the three real strategies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTunnelSummary {
    pub id: String,
    pub name: String,
    pub kind: SavedTunnelKind,
}

/// React `TunnelUsage` — the connections referencing a saved tunnel
/// (`get_tunnel_usage`). Two parallel lists: ids and display names, in the
/// same order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelUsage {
    pub connection_ids: Vec<String>,
    pub connection_names: Vec<String>,
}

/// React `TableOptionsSnapshot` — the per-table storage options the schema
/// panels diff.
///
/// Wasm-safe local mirror of driver-api's `schema_metadata::TableOptionsSnapshot`
/// with the same wire shape. All three fields are
/// `?: string | null` in TS, and the twin declares them
/// `skip_serializing_if` — so absent stays absent on the wire (the twin's
/// own emission), while `null` and absent both decode (serde `Option`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableOptionsSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire).unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value).unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn tunnel_source_keeps_the_react_literals() {
        let table = [
            (TunnelSource::None, "none"),
            (TunnelSource::Saved, "saved"),
            (TunnelSource::Inline, "inline"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: TunnelSource = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn saved_tunnel_summaries_stay_secret_free_and_narrow_to_real_strategies() {
        let summary = SavedTunnelSummary {
            id: "t1".into(),
            name: "Prod Bastion".into(),
            kind: SavedTunnelKind::WebSocket,
        };
        // Exactly three keys — no credential leg exists to leak.
        assert_eq!(
            encode(&summary),
            serde_json::json!({ "id": "t1", "name": "Prod Bastion", "kind": "websocket" })
        );
        let back: SavedTunnelSummary = decode(encode(&summary));
        assert_eq!(back, summary);
    }

    #[test]
    fn tunnel_usage_keeps_two_parallel_lists() {
        let usage = TunnelUsage {
            connection_ids: vec!["c1".into(), "c2".into()],
            connection_names: vec!["Analytics".into(), "Billing".into()],
        };
        let wire = encode(&usage);
        assert_eq!(wire["connectionIds"], serde_json::json!(["c1", "c2"]));
        assert_eq!(
            wire["connectionNames"],
            serde_json::json!(["Analytics", "Billing"])
        );
        let back: TunnelUsage = decode(wire);
        assert_eq!(back, usage);
    }

    #[test]
    fn table_options_snapshot_mirrors_the_driver_api_skip_wire() {
        // Absent stays absent (the twin's own emission)…
        let bare = TableOptionsSnapshot {
            comment: None,
            engine: None,
            charset: None,
        };
        assert_eq!(encode(&bare), serde_json::json!({}));
        // …while each leg can carry a value…
        let full = TableOptionsSnapshot {
            comment: Some("facts since 2019".into()),
            engine: Some("InnoDB".into()),
            charset: Some("utf8mb4".into()),
        };
        assert_eq!(
            encode(&full),
            serde_json::json!({
                "comment": "facts since 2019",
                "engine": "InnoDB",
                "charset": "utf8mb4",
            })
        );
        // …and explicit `null` payloads decode too (the `?: string | null`
        // union's other leg).
        let from_nulls: TableOptionsSnapshot =
            decode(serde_json::json!({ "comment": null, "engine": null, "charset": null }));
        assert_eq!(from_nulls, bare);
    }
}
