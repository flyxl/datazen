//! The three table-read settings the aggregate re-exports — ported from
//! React `packages/frontend/src/types/settings.ts` via
//! `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! React's `types/index.ts` re-exports `DefaultTableSort`, `GridDensity`
//! and `RowCountMode` from `./settings` — the three settings a table read
//! consults before it asks the host for rows. The TS file's own header
//! records why they live there — not in `@datazen/backend-client`: they are
//! **host** preferences (the shared transport carries a whitelist of
//! portable preferences, and these are about how a grid reads a table).
//! The same file also declares the default constants
//! (`DEFAULT_ROW_COUNT_MODE`, …), which the aggregate does **not**
//! re-export — React's `types/index.ts` re-exports the three types only,
//! so the constants stay with the settings-store port that consumes them
//! (the port's tests below pin the default literals instead).
//!
//! No Rust home existed, so this module is the mirror. Pure contracts —
//! no IO, no state — so the module compiles for wasm32 exactly like its
//! `chart_types` sibling.

use serde::{Deserialize, Serialize};

/// React `RowCountMode = 'auto' | 'exact' | 'estimated' | 'none'` — how a
/// table read obtains its row count.
///
/// - `auto` — the host does not force a strategy: the driver declares what
///   it can do, and the grid shows whatever the driver answered (today's
///   behaviour, and the default for that reason);
/// - `exact` — always `COUNT(*)`, however expensive;
/// - `estimated` — ask for an estimate when the database has one; the host
///   falls back to an exact count when it does not;
/// - `none` — do not count at all; the pager then shows the page number
///   only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RowCountMode {
    Auto,
    Exact,
    Estimated,
    None,
}

impl RowCountMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Exact => "exact",
            Self::Estimated => "estimated",
            Self::None => "none",
        }
    }
}

/// React `DefaultTableSort = 'primaryKeyAsc' | 'primaryKeyDesc' | 'none'` —
/// the order a table read uses when the user did not ask for one.
///
/// - `primaryKeyAsc` — the driver's own default (today's behaviour);
/// - `primaryKeyDesc` — the primary key, descending;
/// - `none` — no order at all; paging may then repeat or skip rows, and
///   the grid says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultTableSort {
    PrimaryKeyAsc,
    PrimaryKeyDesc,
    None,
}

impl DefaultTableSort {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrimaryKeyAsc => "primaryKeyAsc",
            Self::PrimaryKeyDesc => "primaryKeyDesc",
            Self::None => "none",
        }
    }
}

/// React `GridDensity = 'compact' | 'standard' | 'relaxed'` — the row
/// density of the data grids (DB-17).
///
/// The setting is a **delta**, not an absolute height: each grid keeps its
/// own default row height (the table grid and the query-result grid
/// differ), and the density shifts it. `standard` is a delta of zero, so
/// the default changes nothing anywhere — the PRD's "do not change
/// defaults" holds per grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GridDensity {
    Compact,
    Standard,
    Relaxed,
}

impl GridDensity {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Standard => "standard",
            Self::Relaxed => "relaxed",
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
    fn the_table_read_settings_keep_their_literals() {
        let modes = [
            (RowCountMode::Auto, "auto"),
            (RowCountMode::Exact, "exact"),
            (RowCountMode::Estimated, "estimated"),
            (RowCountMode::None, "none"),
        ];
        for (value, literal) in modes {
            assert_eq!(value.as_str(), literal);
            let parsed: RowCountMode = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let sorts = [
            (DefaultTableSort::PrimaryKeyAsc, "primaryKeyAsc"),
            (DefaultTableSort::PrimaryKeyDesc, "primaryKeyDesc"),
            (DefaultTableSort::None, "none"),
        ];
        for (value, literal) in sorts {
            assert_eq!(value.as_str(), literal);
            let parsed: DefaultTableSort = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let densities = [
            (GridDensity::Compact, "compact"),
            (GridDensity::Standard, "standard"),
            (GridDensity::Relaxed, "relaxed"),
        ];
        for (value, literal) in densities {
            assert_eq!(value.as_str(), literal);
            let parsed: GridDensity = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn the_settings_file_default_literals_stay_pinned() {
        // React types/settings.ts's constants — DEFAULT_ROW_COUNT_MODE,
        // DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD (200_000), DEFAULT_TABLE_SORT,
        // DEFAULT_GRID_DENSITY — are not on the aggregate's export surface
        // (types/index.ts re-exports the three types only), but their
        // literals are the settings-store port's contract, so pin the enum
        // spellings here: nobody forces a strategy, and the default order
        // is the primary key ascending, the default density the delta of
        // zero.
        assert_eq!(RowCountMode::Auto.as_str(), "auto");
        assert_eq!(DefaultTableSort::PrimaryKeyAsc.as_str(), "primaryKeyAsc");
        assert_eq!(GridDensity::Standard.as_str(), "standard");
    }
}
