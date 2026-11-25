//! Port of `packages/frontend/src/components/filter/types.ts`.
//!
//! `FilterColumn` is the column the filter editor can filter on. Its `type` is
//! the driver's type string (`int4`, `varchar(64)`, …) and decides which
//! operators the column may be filtered with; `None` falls back to the text
//! operators. This is the single definition: T056's local copy in
//! `filter_condition_editor.rs` was removed when this module landed.

/// A column the filter editor can filter on.
#[allow(dead_code)] // contract type; consumers are the filter tests and the editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterColumn {
    pub name: String,
    /// React's `type` — the driver's type string; `None` = text operators.
    pub data_type: Option<String>,
}

#[allow(dead_code)] // constructor used by the editor and tests.
impl FilterColumn {
    pub fn new(name: impl Into<String>, data_type: Option<&str>) -> Self {
        FilterColumn {
            name: name.into(),
            data_type: data_type.map(str::to_owned),
        }
    }
}

// Leftover L1 (removal stage): when the filter panel view binds driver columns,
// construct this type from the driver's `DataTypeMeta` instead of hand-building
// it; drop the blanket allows above once that production caller lands.
