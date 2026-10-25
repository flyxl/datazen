//! The named-view DTOs: the wire shape of a saved filter view.
//!
//! A named view is what a user saves instead of retyping a filter: the
//! **applied** filters of one table, plus the sorts and the column layout
//! they were looking at.
//!
//! The shape is deliberately the same on both sides of the IPC boundary — the
//! Rust struct here and the TypeScript interface in
//! `packages/backend-client/src/types/tableData.ts` are one contract, and the
//! key sets are pinned by a test on each side so the two cannot drift.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use datazen_driver_api::filters::{FilterCondition, SortCondition};

/// The scope a named view belongs to.
///
/// The scope is the persistent identity of the table the view was saved from:
/// the **connection id**, the database, the schema and the table. A
/// `dbSessionId` is runtime state and is never persisted, so it must never
/// appear here (AGENTS.md, "ID 术语规范").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedViewScope {
    /// The persistent connection id.
    pub connection_id: String,
    /// The logical database, when the engine has one.
    #[serde(default)]
    pub database: Option<String>,
    /// The schema, when the engine has one.
    #[serde(default)]
    pub schema: Option<String>,
    /// The table name.
    pub table: String,
}

/// A saved view of one table.
///
/// `id` is minted by the backend and never changes: renaming a view keeps its
/// identity, so the frontend can replace its local copy with the record the
/// backend returns without losing track of which entry it was.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedView {
    /// Stable identity. An empty string asks the backend to mint one.
    #[serde(default)]
    pub id: String,
    /// The user-visible name; unique inside the scope, case-insensitively.
    pub name: String,
    /// Where the view belongs.
    pub scope: NamedViewScope,
    /// The **applied** filters, not the draft.
    #[serde(default)]
    pub filters: Vec<FilterCondition>,
    /// `"and"` or `"or"`. The wire keeps it a string so a file written by an
    /// older build still loads; `validate` refuses anything else.
    #[serde(default = "default_filter_logic")]
    pub filter_logic: String,
    /// The sorts that were in effect.
    #[serde(default)]
    pub sorts: Vec<SortCondition>,
    /// The visible columns; `None` means every column is visible.
    #[serde(default)]
    pub visible_columns: Option<Vec<String>>,
    /// The column order; `None` means the driver's default order.
    #[serde(default)]
    pub column_order: Option<Vec<String>>,
    /// At most one view per scope carries this.
    #[serde(default)]
    pub is_default: bool,
    /// When the view was first saved.
    pub created_at: DateTime<Utc>,
    /// When the view was last changed; `None` for a record that never was.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_filter_logic() -> String {
    "and".to_string()
}

impl NamedView {
    /// Check the invariants the shared layer owns.
    ///
    /// The reason is written for a human and never carries a SQL fragment: the
    /// frontend shows it next to the name field.
    ///
    /// The reason also never **quotes what the user typed**. A host message
    /// reaches the IPC payload and the host's log lines, and the redactor does
    /// not cover free text — it covers the shapes it documents (JSON pairs,
    /// query strings, bearer tokens, URL userinfo). The rule is what the host
    /// knows; the offending value stays in the record.
    pub fn validate(&self) -> Result<(), String> {
        if super::rules::normalize_view_name(&self.name).is_empty() {
            return Err("the view name must not be empty".into());
        }
        if self.scope.connection_id.trim().is_empty() {
            return Err("the view scope needs a connection id".into());
        }
        if self.scope.table.trim().is_empty() {
            return Err("the view scope needs a table name".into());
        }
        if !matches!(
            self.filter_logic.trim().to_ascii_lowercase().as_str(),
            "and" | "or"
        ) {
            return Err("the filter logic must be \"and\" or \"or\"".into());
        }
        for filter in &self.filters {
            if filter.column.trim().is_empty() {
                return Err("every filter condition needs a column name".into());
            }
        }
        Ok(())
    }

    /// Bring the record into the shape the shared layer stores.
    ///
    /// The name is trimmed and its whitespace runs are collapsed; the filter
    /// logic is lower-cased. Nothing else is touched: the values, the sorts
    /// and the column lists are the user's, byte for byte.
    pub fn normalize(&mut self) {
        self.name = super::rules::normalize_view_name(&self.name);
        self.filter_logic = self.filter_logic.trim().to_ascii_lowercase();
    }
}
