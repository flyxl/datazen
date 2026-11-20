//! The favorites shape the aggregate declares — ported from React
//! `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! React's `types/index.ts` declares `FavoriteQuery` itself — a saved
//! statement. Since the file-first switch this is one `.sql` file under
//! the favorites root, named by `id`, with these fields in `--`
//! front-matter above the statement. backend-client's
//! `types/queryAssets.ts` declares a same-named `FavoriteQuery` for the
//! query-assets domain (the W8 web surface: "on the desktop this is one
//! `.sql` file … on the server it is a row. The wire shape is the same
//! either way") — that port lands with its own module; this is the
//! aggregate's frontend declaration, exactly the split React has, and the
//! field lists are identical (the query-assets twin is the audit path).
//!
//! Pure contract — the front-matter parse and the IPC both cross this
//! shape — so the wire family's camelCase serde law applies, and the
//! module compiles for wasm32 exactly like its `chart_types` sibling.

use serde::{Deserialize, Serialize};

/// React `FavoriteQuery` — one saved statement.
///
/// `updatedAt`, `database`, `keyword` and `folder` are `?: string | null`
/// — null on the wire when set to `None`, absent also accepted — because
/// the file front-matter and the server row both emit explicit nulls for
/// known-absent values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteQuery {
    pub id: String,
    pub connection_id: String,
    pub title: String,
    pub sql: String,
    pub created_at: String,
    /// Absent when the file carries no `updatedAt`, or the change predates
    /// it. Null on the wire when set to `None`; absent also accepted.
    pub updated_at: Option<String>,
    /// Target database the statement was saved against, for later
    /// re-binding. Null on the wire when set to `None`; absent also
    /// accepted.
    pub database: Option<String>,
    /// Reserved for the keyword track; parsed and round-tripped, not yet
    /// bound. Null on the wire when set to `None`; absent also accepted.
    pub keyword: Option<String>,
    /// Directory relative to the favorites root, `/`-separated. Absent =
    /// root. Null on the wire when set to `None`; absent also accepted.
    pub folder: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn a_favorite_round_trips_its_front_matter_fields() {
        let favorite = FavoriteQuery {
            id: "01JD8ZQ5X7MW3R8Q2V8S4T6U9A".into(),
            connection_id: "c1".into(),
            title: "Daily active users".into(),
            sql: "SELECT count(*) FROM events WHERE …".into(),
            created_at: "2026-11-20T03:14:15Z".into(),
            updated_at: Some("2026-11-20T04:00:00Z".into()),
            database: Some("analytics".into()),
            keyword: None,
            folder: Some("reports/weekly".into()),
        };
        let wire = encode(&favorite);
        assert_eq!(wire["id"], "01JD8ZQ5X7MW3R8Q2V8S4T6U9A");
        assert_eq!(wire["folder"], "reports/weekly");
        // `keyword` is `?: string | null` — the key stays present with
        // `null` for a known-absent value.
        assert_eq!(wire["keyword"], serde_json::Value::Null);
        let back: FavoriteQuery = decode(wire);
        assert_eq!(back, favorite);
    }

    #[test]
    fn the_minimal_front_matter_still_decodes() {
        // The file-first switch's older front-matter carried only the five
        // required fields — everything else reads as its absence.
        let minimal: FavoriteQuery = decode(serde_json::json!({
            "id": "01JD8ZQ5X7MW3R8Q2V8S4T6U9A",
            "connectionId": "c1",
            "title": "Untitled",
            "sql": "SELECT 1",
            "createdAt": "2026-11-20T03:14:15Z"
        }));
        assert!(minimal.updated_at.is_none());
        assert!(minimal.database.is_none());
        assert!(minimal.keyword.is_none());
        assert!(minimal.folder.is_none());
        // And a root-level favorite (no folder) round-trips the null wire.
        let wire = encode(&minimal);
        assert_eq!(wire["folder"], serde_json::Value::Null);
    }
}
