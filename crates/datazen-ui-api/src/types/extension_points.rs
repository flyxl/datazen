//! The `CountStrategy` half of the aggregate's extension-points re-exports —
//! ported from React `packages/frontend/src/types/index.ts` (tasklist T211,
//! task T128's dependency inventory).
//!
//! React's `types/index.ts` re-exports three names from
//! `@datazen/extension-points`: `SslMode`, `DatabaseObjectKind` and
//! [`CountStrategy`]. The first two are already owned by this crate's
//! `connection_types` (T128's thin mirrors of the same extension-points
//! unions — the aggregate re-exports them from there, it does not redefine
//! them); `CountStrategy` had no Rust home, so this module is it.
//!
//! Source of truth: `packages/extension-points/src/sql-editor/databaseTypes.ts`
//! declares `CountStrategy = 'exact' | 'estimated' | 'unsupported'` — what a
//! driver answered when asked for a row count. There is no native Rust twin
//! to mirror yet (driver-api carries no closed count-strategy enum; the
//! kernel answers strategies per call), so this is the canonical mirror.
//! Like every types-family module it is pure contract — no IO, no state —
//! and compiles for wasm32 exactly like its `chart_types` sibling.

use serde::{Deserialize, Serialize};

/// React `CountStrategy = 'exact' | 'estimated' | 'unsupported'` — how a
/// row count was obtained.
///
/// Declared by `@datazen/extension-points` and re-exported by the aggregate
/// (`types/index.ts`); the [`crate::types::table_data::TableDataResult`]
/// `totalRowsKind` field is its consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CountStrategy {
    Exact,
    Estimated,
    Unsupported,
}

impl CountStrategy {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Estimated => "estimated",
            Self::Unsupported => "unsupported",
        }
    }
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
    fn count_strategy_keeps_the_react_literals() {
        let table = [
            (CountStrategy::Exact, "exact"),
            (CountStrategy::Estimated, "estimated"),
            (CountStrategy::Unsupported, "unsupported"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: CountStrategy = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }
}
