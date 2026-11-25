//! Prediction contracts, ported from React
//! `packages/frontend/src/lib/relationPrediction/types.ts` (T081).
//!
//! Only the three contracts the metadata adapter consumes are ported:
//! `PredictionColumn`, `DeclaredForeignKey` and `PredictionTable`. Optional
//! boolean fields stay `Option<bool>` exactly as in TS (`nullable?`, `indexed?`),
//! so a caller that omits them is representable; the adapter always fills both.
//!
//! Pure value types: no Rune tree, no `oklch`, no i18n, no IPC.

/// One column of a table under consideration.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T081 forthcoming consumer: the SQL-editor prediction layer (not landed yet).
pub struct PredictionColumn {
    pub name: String,
    /// Driver-reported type string, e.g. `integer`, `varchar`.
    pub data_type: String,
    pub nullable: Option<bool>,
    /// True when the column is indexed (any index), a weak signal of FK usage.
    pub indexed: Option<bool>,
}

/// A declared foreign key, so prediction never restates ground truth.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T081 forthcoming consumer: the SQL-editor prediction layer (not landed yet).
pub struct DeclaredForeignKey {
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
}

/// A table to consider as a source and/or target of a relationship.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T081 forthcoming consumer: the SQL-editor prediction layer (not landed yet).
pub struct PredictionTable {
    /// Stable identity used in evidence and output.
    pub id: String,
    /// Bare table name, used for name matching.
    pub name: String,
    /// Schema the table lives in; prediction does not cross schemas.
    pub schema: Option<String>,
    pub columns: Vec<PredictionColumn>,
    pub primary_key: Vec<String>,
    /// Every column set covered by a unique index (single-column or composite).
    /// A relationship can only target a key, never an arbitrary column.
    pub unique_column_sets: Vec<Vec<String>>,
    pub declared_foreign_keys: Vec<DeclaredForeignKey>,
}
