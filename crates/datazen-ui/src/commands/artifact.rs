//! Port of `packages/frontend/src/commands/artifact.ts` (T130).
//!
//! One command: the published-result artifact export. The Host owns
//! identity, partial-result confirmation and the save destination; the UI
//! only passes the artifact id and reads the two-arm receipt.
//!
//! ## What maps where
//!
//! - `invoke('export_artifact', { request: { artifactId } })` becomes
//!   [`export_artifact_request`]: the exact wire bag (`request` wrapper
//!   included), the `schema_diff` runner's shaper pattern.
//! - `ArtifactExportResponse` (the tagged union) is
//!   [`ArtifactExportResponse`] with its serde shape — the host decodes
//!   the IPC answer straight into it. T057 carried the same arms as
//!   `query_artifact_export::ArtifactExportReceipt`; that type is now a
//!   re-export of this one, the single source.
//!
//! Known deltas: none behavioural.

use serde::{Deserialize, Serialize};

/// React `ArtifactExportResponse`: the Host's answer to the export.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub(crate) enum ArtifactExportResponse {
    /// The native save dialog was cancelled — a normal exit, not an error.
    #[serde(rename = "cancelled")]
    Cancelled,
    /// The artifact was written; `byteLength` is the Host's receipt detail.
    #[serde(rename = "saved")]
    #[serde(rename_all = "camelCase")]
    Saved {
        artifact_id: String,
        byte_length: u64,
    },
}

/// `artifactCommands.exportArtifact(artifactId)`: the
/// `export_artifact` legacy call's exact wire bag.
pub(crate) fn export_artifact_request(artifact_id: &str) -> (&'static str, serde_json::Value) {
    (
        "export_artifact",
        serde_json::json!({ "request": { "artifactId": artifact_id } }),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn the_wire_bag_wraps_the_id_in_request() {
        assert_eq!(
            export_artifact_request("artifact-1"),
            (
                "export_artifact",
                serde_json::json!({ "request": { "artifactId": "artifact-1" } })
            )
        );
    }

    #[test]
    fn the_receipt_decodes_both_arms() {
        let cancelled: ArtifactExportResponse =
            serde_json::from_value(serde_json::json!({ "status": "cancelled" }))
                .expect("cancelled decodes");
        assert_eq!(cancelled, ArtifactExportResponse::Cancelled);
        let saved: ArtifactExportResponse = serde_json::from_value(serde_json::json!({
            "status": "saved",
            "artifactId": "artifact-1",
            "byteLength": 100,
        }))
        .expect("saved decodes");
        assert_eq!(
            saved,
            ArtifactExportResponse::Saved { artifact_id: "artifact-1".to_owned(), byte_length: 100 }
        );
        // And serializes back (the host's own bridge direction).
        assert_eq!(
            serde_json::to_value(&saved).unwrap_or_default(),
            serde_json::json!({
                "status": "saved",
                "artifactId": "artifact-1",
                "byteLength": 100,
            })
        );
        // Anything else is a decode error, never a silent default.
        assert!(serde_json::from_value::<ArtifactExportResponse>(
            serde_json::json!({ "status": "nope" })
        )
        .is_err());
    }
}
