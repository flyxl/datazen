//! The data-sync and schema-diff shapes the aggregate declares — ported
//! from React `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! The React file's "Data Sync types" section is one vocabulary in two
//! halves: the comparison listing ([`TableCompareStatus`],
//! [`SyncObjectKind`], [`TableComparison`]) and the schema-diff detail
//! ([`ColumnDiffEntry`], [`ChangedColumnDiff`],
//! [`CheckConstraintDiffEntry`], [`TableOptionsDiff`],
//! [`TableSchemaDiff`]). Both halves flow through IPC (the data-sync
//! window's compare/deploy journeys), so the wire family's camelCase serde
//! law applies throughout.
//!
//! backend-client's own `types/schemaDiff.ts` carries a same-named
//! `TableSchemaDiff` family for the web-surface domain — that port lands
//! with its own module; these are the aggregate's frontend declarations,
//! exactly the split React has.
//!
//! [`TableOptionsDiff`]'s legs are [`TableOptionsSnapshot`] —
//! [`crate::types::driver_sdk`]'s wasm-safe mirror of the driver-sdk shape
//! the aggregate re-exports.
//!
//! Pure contracts — no IO, no state — so the module compiles for wasm32
//! exactly like its `chart_types` sibling.

use serde::{Deserialize, Serialize};

use crate::types::driver_sdk::TableOptionsSnapshot;

/// React `TableCompareStatus = 'identical' | 'different' | 'source_only' |
/// 'target_only'` — how one object's presence compares across the two
/// sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableCompareStatus {
    Identical,
    Different,
    SourceOnly,
    TargetOnly,
}

impl TableCompareStatus {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identical => "identical",
            Self::Different => "different",
            Self::SourceOnly => "source_only",
            Self::TargetOnly => "target_only",
        }
    }
}

/// React `SyncObjectKind = 'table' | 'view' | 'function' | 'procedure'` —
/// the object kinds a sync can pair across two sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncObjectKind {
    Table,
    View,
    Function,
    Procedure,
}

impl SyncObjectKind {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::Function => "function",
            Self::Procedure => "procedure",
        }
    }
}

/// React `TableComparison` — one object's comparison row.
///
/// `kind` is plain `?:` (absent stays absent); `sourceRows`/`targetRows`
/// are required-but-nullable (`number | null` — a side without the object
/// has no row count, and catalog counts stay `i64`, the family's width).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableComparison {
    pub table: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<SyncObjectKind>,
    pub status: TableCompareStatus,
    pub source_rows: Option<i64>,
    pub target_rows: Option<i64>,
}

/// React `ColumnDiffEntry` — one column's identity as the diff lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDiffEntry {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
}

/// React `ChangedColumnDiff` — one column that differs on both sides: the
/// column, each side's entry, and the names of the changed aspects (the
/// UI's change chips).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedColumnDiff {
    pub name: String,
    pub source: ColumnDiffEntry,
    pub target: ColumnDiffEntry,
    pub changes: Vec<String>,
}

/// React `CheckConstraintDiffEntry` — one check constraint as the diff
/// lists it: its name and its expression.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckConstraintDiffEntry {
    pub name: String,
    pub expression: String,
}

/// React `TableOptionsDiff` — the two sides' storage options plus the
/// names of the changed aspects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableOptionsDiff {
    pub source: TableOptionsSnapshot,
    pub target: TableOptionsSnapshot,
    pub changes: Vec<String>,
}

/// React `TableSchemaDiff` — one table's full diff, the schema-diff
/// panel's model.
///
/// The alias pairs are deliberate: `added`/`removed` are the legacy
/// spellings of `missingOnTarget`/`extraOnTarget` (absent-on-target → ADD
/// on deploy; present-on-target-only → DROP on deploy), and the React file
/// requires both to be present — `added`/`removed` have no `?`, they are
/// required arrays, while the `missingOnTarget`/`extraOnTarget` spellings
/// are optional. `targetOnly` marks the selected-table-exists-only-on-
/// target case (planned as DROP TABLE).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSchemaDiff {
    pub table: String,
    /// The selected table exists only on the target and is planned as
    /// DROP TABLE.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_only: Option<bool>,
    /// Present on source, missing on target → ADD on deploy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_on_target: Option<Vec<ColumnDiffEntry>>,
    /// Present on target only → DROP on deploy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_on_target: Option<Vec<ColumnDiffEntry>>,
    /// Alias of `missingOnTarget` (legacy, required).
    pub added: Vec<ColumnDiffEntry>,
    /// Alias of `extraOnTarget` (legacy, required).
    pub removed: Vec<ColumnDiffEntry>,
    pub changed: Vec<ChangedColumnDiff>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_check_constraints: Option<Vec<CheckConstraintDiffEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_check_constraints: Option<Vec<CheckConstraintDiffEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_options: Option<TableOptionsDiff>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ddl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ddl: Option<String>,
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

    fn column(name: &str, data_type: &str, nullable: bool, is_primary_key: bool) -> ColumnDiffEntry {
        ColumnDiffEntry {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            is_primary_key,
        }
    }

    #[test]
    fn the_sync_unions_keep_their_literals() {
        let statuses = [
            (TableCompareStatus::Identical, "identical"),
            (TableCompareStatus::Different, "different"),
            (TableCompareStatus::SourceOnly, "source_only"),
            (TableCompareStatus::TargetOnly, "target_only"),
        ];
        for (value, literal) in statuses {
            assert_eq!(value.as_str(), literal);
            let parsed: TableCompareStatus = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let kinds = [
            (SyncObjectKind::Table, "table"),
            (SyncObjectKind::View, "view"),
            (SyncObjectKind::Function, "function"),
            (SyncObjectKind::Procedure, "procedure"),
        ];
        for (value, literal) in kinds {
            assert_eq!(value.as_str(), literal);
            let parsed: SyncObjectKind = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn a_comparison_row_keeps_its_nullable_row_counts() {
        let comparison = TableComparison {
            table: "users".into(),
            kind: Some(SyncObjectKind::View),
            status: TableCompareStatus::SourceOnly,
            source_rows: Some(42),
            target_rows: None,
        };
        let wire = encode(&comparison);
        assert_eq!(wire["status"], "source_only");
        assert_eq!(wire["kind"], "view");
        assert_eq!(wire["sourceRows"], 42);
        // A side without the object has no row count — the key stays
        // present with `null`.
        assert_eq!(wire["targetRows"], serde_json::Value::Null);
        let back: TableComparison = decode(wire);
        assert_eq!(back, comparison);

        // `kind` absent stays absent (the legacy rows carried no kind).
        let unkind = TableComparison { kind: None, ..comparison };
        assert!(encode(&unkind).get("kind").is_none());
    }

    #[test]
    fn a_full_table_diff_round_trips() {
        let diff = TableSchemaDiff {
            table: "users".into(),
            target_only: None,
            missing_on_target: Some(vec![column("bio", "text", true, false)]),
            extra_on_target: None,
            added: vec![column("bio", "text", true, false)],
            removed: vec![column("legacy_flag", "boolean", true, false)],
            changed: vec![ChangedColumnDiff {
                name: "email".into(),
                source: column("email", "varchar(255)", false, false),
                target: column("email", "varchar(320)", false, false),
                changes: vec!["dataType".into()],
            }],
            missing_check_constraints: Some(vec![CheckConstraintDiffEntry {
                name: "users_email_check".into(),
                expression: "email ~ '^[^@]+@[^@]+$'".into(),
            }]),
            extra_check_constraints: None,
            table_options: Some(TableOptionsDiff {
                source: TableOptionsSnapshot {
                    comment: Some("source users".into()),
                    engine: None,
                    charset: Some("utf8".into()),
                },
                target: TableOptionsSnapshot {
                    comment: None,
                    engine: Some("InnoDB".into()),
                    charset: Some("utf8mb4".into()),
                },
                changes: vec!["charset".into()],
            }),
            source_ddl: Some("CREATE TABLE users (…)".into()),
            target_ddl: Some("CREATE TABLE users (…)".into()),
        };
        let wire = encode(&diff);
        // The alias pair rides both spellings: `added` required,
        // `missingOnTarget` optional-but-present here.
        assert_eq!(wire["added"][0]["name"], "bio");
        assert_eq!(wire["missingOnTarget"][0]["name"], "bio");
        assert_eq!(wire["removed"][0]["name"], "legacy_flag");
        assert_eq!(wire["changed"][0]["changes"][0], "dataType");
        assert_eq!(wire["missingCheckConstraints"][0]["name"], "users_email_check");
        assert_eq!(wire["tableOptions"]["target"]["charset"], "utf8mb4");
        // The optional check-constraint leg that is absent stays absent.
        assert!(wire.get("extraCheckConstraints").is_none());
        let back: TableSchemaDiff = decode(wire);
        assert_eq!(back, diff);

        // The legacy-only shape (no optional spellings) still decodes —
        // the diff window's older payloads.
        let legacy: TableSchemaDiff = decode(serde_json::json!({
            "table": "t",
            "added": [],
            "removed": [],
            "changed": []
        }));
        assert_eq!(legacy.table, "t");
        assert!(legacy.missing_on_target.is_none());
        assert!(legacy.table_options.is_none());
    }
}
