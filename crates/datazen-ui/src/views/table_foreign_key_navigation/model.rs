//! Pure VM mirrors for `useTableForeignKeyNavigation` and its helper modules.
//! Every type here is host-owned data; the decisions live in `logic` / `history`.

use std::collections::BTreeMap;

/// React `Value` (`string | number | boolean | null`).
#[allow(dead_code)] // VM mirror; supplied by the host row data.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FkValue {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

impl FkValue {
    /// React's `value == null` check (absent ⇒ cannot navigate).
    #[allow(dead_code)] // pure helper; consumed by navigation and tests.
    pub(crate) fn is_absent(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// JSON encoding used by `fkNavKey` (numbers without a trailing `.0`).
    #[allow(dead_code)] // pure helper; consumed by the nav key and tests.
    pub(crate) fn json(&self) -> String {
        match self {
            Self::Str(value) => json_quote(value),
            Self::Num(value) => {
                if value.fract() == 0.0 && value.is_finite() {
                    format!("{}", *value as i64)
                } else {
                    format!("{value}")
                }
            }
            Self::Bool(value) => value.to_string(),
            Self::Null => "null".to_owned(),
        }
    }
}

/// Minimal JSON string escaping for the nav key (identifiers / values).
#[allow(dead_code)] // pure helper; consumed by the nav key and tests.
pub(crate) fn json_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// React `ForeignKeyInfo`.
#[allow(dead_code)] // VM mirror; supplied by the host FK metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ForeignKeyInfoVm {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
}

/// React `FkGroupAffordance` (one group shared by every member column).
#[allow(dead_code)] // VM mirror; produced by `index_foreign_keys`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FkGroupAffordanceVm {
    pub local_columns: Vec<String>,
    pub target_columns: Vec<String>,
    pub target_label: String,
    pub constraint_name: String,
    pub reference_table: String,
}

/// React `column → group` index.
#[allow(dead_code)] // VM mirror; produced by `index_foreign_keys`.
pub(crate) type FkIndexVm = BTreeMap<String, FkGroupAffordanceVm>;

/// React `FkNavTargetIdentity` — the four fields a nav key needs.
#[allow(dead_code)] // VM mirror; supplied by the jump request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FkNavTargetIdentityVm {
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
    pub columns: Vec<String>,
}

/// React `FilterCondition` (only the `eq` filters the FK path builds).
#[allow(dead_code)] // VM mirror; produced by `build_fk_filters`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FkFilterVm {
    pub column: String,
    pub operator: &'static str,
    pub value: FkValue,
}

/// React `GridViewport`.
#[allow(dead_code)] // VM mirror; supplied by the host grid.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GridViewportVm {
    pub scroll_top: i64,
    pub anchor_row: Option<usize>,
    pub anchor_column: Option<String>,
}

/// React `FkNavEntry.origin` (the breadcrumb/restore minimum).
#[allow(dead_code)] // VM mirror; supplied by the jump record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FkNavOriginVm {
    pub source_panel_id: String,
    pub source_table: String,
    pub local_columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
}

/// React `FkNavEntry`.
#[allow(dead_code)] // VM mirror; produced by the history machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FkNavEntryVm {
    pub id: String,
    pub panel_id: String,
    pub nav_key: String,
    pub origin: FkNavOriginVm,
}
