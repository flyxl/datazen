use serde_json::Value;

use crate::{FilterCondition, FilterConnector, SortCondition};

pub const NAMED_VIEW_EXPORT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedViewScope {
    pub connection_id: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedView {
    pub id: String,
    pub name: String,
    pub scope: NamedViewScope,
    pub filters: Vec<FilterCondition>,
    pub filter_logic: String,
    pub raw_where: Option<String>,
    pub sorts: Vec<SortCondition>,
    pub visible_columns: Option<Vec<String>>,
    pub column_order: Option<Vec<String>>,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedViewExportFile {
    pub version: u32,
    pub exported_at: String,
    pub views: Vec<NamedView>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableStateForView {
    pub scope: NamedViewScope,
    pub filters: Vec<FilterCondition>,
    pub filter_logic: String,
    pub raw_where: Option<String>,
    pub sorts: Vec<SortCondition>,
    pub visible_columns: Option<Vec<String>>,
    pub column_order: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortFilterCondition {
    pub column: String,
    pub operator: String,
    pub value: Option<Value>,
    pub connector: Option<FilterConnector>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewFilterReadout {
    pub filters: Vec<FilterCondition>,
    pub unknown_operators: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackendClientCapabilities {
    pub list_named_views_unsupported: bool,
}

pub fn named_views_available(client: Option<&BackendClientCapabilities>) -> bool {
    client.is_some_and(|capabilities| !capabilities.list_named_views_unsupported)
}
