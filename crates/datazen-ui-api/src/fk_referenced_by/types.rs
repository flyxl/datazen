use std::collections::BTreeMap;

use crate::{FilterCondition, GridViewport, Value};

pub const MAX_REFERENCING_TABS: usize = 4;

pub const PROBE_FAILED_MESSAGE: &str =
    "grid.fk.probeFailed: the referencing-table catalog did not settle";

pub const NO_REFERENCES_EMPTY: &str = "grid.fk.noReferences: no table references '";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutcomeErrorCode {
    Classified(String),
    Unknown,
    NoReferences,
}

impl OutcomeErrorCode {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Classified(code) => code,
            Self::Unknown => "unknown",
            Self::NoReferences => "grid.fk.noReferences",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeError {
    pub code: OutcomeErrorCode,
    pub raw: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferencedByOutcome {
    pub ok: bool,
    pub panel_ids: Vec<String>,
    pub error: Option<OutcomeError>,
    pub skipped: usize,
}

impl ReferencedByOutcome {
    pub fn refusal(error: OutcomeError) -> Self {
        Self {
            ok: false,
            panel_ids: Vec::new(),
            error: Some(error),
            skipped: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboundForeignKeyRef {
    pub table: String,
    pub columns: Vec<String>,
    pub referenced_columns: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InboundForeignKeyCatalog {
    pub references: Vec<InboundForeignKeyRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogScope {
    pub db_session_id: String,
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenReferencingRowsInput {
    pub source_panel_id: String,
    pub source_table: String,
    pub db_session_id: String,
    pub database: String,
    pub schema: Option<String>,
    pub connection_id: Option<String>,
    pub driver_type: Option<String>,
    pub row: BTreeMap<String, Value>,
    pub source_viewport: Option<GridViewport>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourcePanelInfo {
    pub connection_id: Option<String>,
    pub db_session_id: Option<String>,
    pub connection_name: Option<String>,
    pub database_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedPanel {
    pub id: String,
    pub table_name: String,
    pub database: String,
    pub table_schema: Option<String>,
    pub sub_tab: String,
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableChangeContext {
    pub connection_id: String,
    pub db_session_id: String,
    pub driver_type: String,
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavOrigin {
    pub source_panel_id: String,
    pub source_table: String,
    pub local_columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavEntry {
    pub panel_id: String,
    pub nav_key: String,
    pub origin: NavOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkOrigin {
    pub source_panel_id: String,
    pub source_table: String,
    pub local_columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
    pub nav_entry_id: String,
}

pub trait ReferencingRowsHost {
    fn next_panel_id(&mut self) -> String;
    fn source_panel(&self, source_panel_id: &str) -> Option<SourcePanelInfo>;
    fn record_viewport(&mut self, panel_id: &str, viewport: &GridViewport);
    fn add_panel(&mut self, panel: &CreatedPanel);
    fn seed_panel(
        &mut self,
        panel_id: &str,
        context: &TableChangeContext,
        filters: &[FilterCondition],
    );
    fn push_nav(&mut self, entry: &NavEntry) -> String;
    fn update_panel(&mut self, panel_id: &str, origin: &FkOrigin);
}

pub type InboundCatalogFetcher =
    dyn FnMut(&CatalogScope) -> Result<InboundForeignKeyCatalog, String>;

pub type GridErrorClassifier = dyn Fn(&str) -> Option<String>;
