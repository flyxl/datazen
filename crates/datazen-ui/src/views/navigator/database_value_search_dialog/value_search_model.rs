//! Pure VMs and rules for the value-search dialog (split from the view).

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use rune::TextEdit;


/// React `ValueSearchMode` + its i18n label key.
#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValueSearchMode {
    Contains,
    NotContains,
    Exact,
    Prefix,
    Suffix,
}

impl ValueSearchMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::NotContains => "notContains",
            Self::Exact => "exact",
            Self::Prefix => "prefix",
            Self::Suffix => "suffix",
        }
    }

    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::Contains => "valueSearch.mode.contains",
            Self::NotContains => "valueSearch.mode.notContains",
            Self::Exact => "valueSearch.mode.exact",
            Self::Prefix => "valueSearch.mode.prefix",
            Self::Suffix => "valueSearch.mode.suffix",
        }
    }
}

pub(crate) fn mode_from_str(value: &str) -> ValueSearchMode {
    match value {
        "notContains" => ValueSearchMode::NotContains,
        "exact" => ValueSearchMode::Exact,
        "prefix" => ValueSearchMode::Prefix,
        "suffix" => ValueSearchMode::Suffix,
        _ => ValueSearchMode::Contains,
    }
}

pub(crate) const MODES: [ValueSearchMode; 5] = [
    ValueSearchMode::Contains,
    ValueSearchMode::NotContains,
    ValueSearchMode::Exact,
    ValueSearchMode::Prefix,
    ValueSearchMode::Suffix,
];

/// React `ValueSearchTypeScope`.
#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeScope {
    Text,
    TextNumber,
    TextDatetime,
    TextNumberDatetime,
}

impl TypeScope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::TextNumber => "text+number",
            Self::TextDatetime => "text+datetime",
            Self::TextNumberDatetime => "text+number+datetime",
        }
    }

    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::Text => "valueSearch.types.text",
            Self::TextNumber => "valueSearch.types.textNumber",
            Self::TextDatetime => "valueSearch.types.textDatetime",
            Self::TextNumberDatetime => "valueSearch.types.textNumberDatetime",
        }
    }
}

pub(crate) fn type_scope_from_str(value: &str) -> TypeScope {
    match value {
        "text+number" => TypeScope::TextNumber,
        "text+datetime" => TypeScope::TextDatetime,
        "text+number+datetime" => TypeScope::TextNumberDatetime,
        _ => TypeScope::Text,
    }
}

pub(crate) fn type_scopes_for_driver(driver: Option<&str>) -> Vec<TypeScope> {
    // React's DB_REGISTRY declaration: MySQL/MariaDB/SQLite can text-match
    // numbers and datetimes; PostgreSQL (and unknown drivers) declare nothing.
    let extra = matches!(driver, Some("mysql") | Some("mariadb") | Some("sqlite"));
    let mut scopes = vec![TypeScope::Text];
    if extra {
        scopes.push(TypeScope::TextNumber);
        scopes.push(TypeScope::TextDatetime);
        scopes.push(TypeScope::TextNumberDatetime);
    }
    scopes
}

pub(crate) fn cell_types_for_scope(scope: TypeScope) -> BTreeSet<&'static str> {
    let mut types = BTreeSet::from(["string", "enum", "text", "char", "varchar"]);
    match scope {
        TypeScope::TextNumber | TypeScope::TextNumberDatetime => {
            types.insert("number");
        }
        _ => {}
    }
    match scope {
        TypeScope::TextDatetime | TypeScope::TextNumberDatetime => {
            for cell in ["date", "time", "datetime"] {
                types.insert(cell);
            }
        }
        _ => {}
    }
    types
}

/// React `Scope`.
#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchScope {
    All,
    Selected,
}

/// React `ValueSearchStatus`.
#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SearchStatus {
    #[default]
    Idle,
    Running,
    Paused,
    Done,
    Cancelled,
}

impl SearchStatus {
    pub(crate) fn status_key(self) -> &'static str {
        match self {
            Self::Idle => "valueSearch.status.idle",
            Self::Running => "valueSearch.status.running",
            Self::Paused => "valueSearch.status.paused",
            Self::Done => "valueSearch.status.done",
            Self::Cancelled => "valueSearch.status.cancelled",
        }
    }
}

#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchTarget {
    pub table: String,
    pub schema: Option<String>,
}

/// React `targetKey` — the schema-qualified identity.
#[allow(dead_code)] // pure rule; pinned by the dialog tests.
pub(crate) fn target_key(target: &SearchTarget) -> String {
    match target.schema.as_deref() {
        Some(schema) if !schema.is_empty() => format!("{schema}.{}", target.table),
        _ => target.table.clone(),
    }
}

#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Debug)]
pub(crate) struct TableVm {
    pub name: String,
    pub schema: Option<String>,
    pub table_type: String,
}

pub(crate) fn value_search_targets(tables: &[TableVm]) -> Vec<SearchTarget> {
    tables
        .iter()
        .filter(|table| table.table_type == "table")
        .map(|table| SearchTarget {
            table: table.name.clone(),
            schema: table.schema.clone(),
        })
        .collect()
}

#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchResult {
    pub table: String,
    pub schema: Option<String>,
    /// `None` means the table could not be searched.
    pub matches: Option<i64>,
    pub columns: Vec<String>,
    pub reason: String,
}

#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SearchProgress {
    pub status: SearchStatus,
    pub completed: usize,
    pub total: usize,
    pub current: Option<String>,
    pub results: Vec<SearchResult>,
    pub elapsed_ms: Option<u64>,
}

pub(crate) fn with_hits(results: &[SearchResult]) -> Vec<SearchResult> {
    let mut hits: Vec<SearchResult> = results
        .iter()
        .filter(|result| result.matches.is_some_and(|count| count > 0))
        .cloned()
        .collect();
    hits.sort_by(|a, b| b.matches.unwrap_or(0).cmp(&a.matches.unwrap_or(0)));
    hits
}

/// React `rankTables` — the three highest hit counts first.
#[allow(dead_code)] // pure rule; pinned by the dialog tests.
pub(crate) fn rank_tables(results: &[SearchResult], limit: usize) -> Vec<SearchResult> {
    with_hits(results).into_iter().take(limit).collect()
}

/// What a locate click asks the host for: the scan's own question.
#[allow(dead_code)] // host port; consumer: the navigator host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocateIntent {
    pub table: String,
    pub schema: Option<String>,
    pub database: String,
    pub value: String,
    pub mode: ValueSearchMode,
    pub columns: Vec<String>,
}

/// A host-executed scan request (the driver + IPC live in the host).
#[allow(dead_code)] // host port; consumer: the navigator host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StartRequest {
    pub db_session_id: String,
    pub database: String,
    pub targets: Vec<SearchTarget>,
    pub value: String,
    pub mode: ValueSearchMode,
    pub cell_types: BTreeSet<&'static str>,
}

#[allow(dead_code)] // host port; consumer: the navigator host (not wired).
pub(crate) struct ValueSearchHostPort<S> {
    pub on_close: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_start: Option<Rc<dyn Fn(&mut S, StartRequest) + 'static>>,
    pub on_pause: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_resume: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_cancel: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_locate: Option<Rc<dyn Fn(&mut S, LocateIntent) + 'static>>,
    pub on_mode: Option<Rc<dyn Fn(&mut S, ValueSearchMode) + 'static>>,
    pub on_scope: Option<Rc<dyn Fn(&mut S, SearchScope) + 'static>>,
    pub on_type_scope: Option<Rc<dyn Fn(&mut S, TypeScope) + 'static>>,
    pub on_toggle_target: Option<Rc<dyn Fn(&mut S, String) + 'static>>,
    pub value_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
    pub picker_filter_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
    pub select_open: Option<Rc<dyn for<'b> Fn(&'b mut S, &str) -> &'b mut bool + 'static>>,
}

impl<S> Default for ValueSearchHostPort<S> {
    fn default() -> Self {
        Self {
            on_close: None,
            on_start: None,
            on_pause: None,
            on_resume: None,
            on_cancel: None,
            on_locate: None,
            on_mode: None,
            on_scope: None,
            on_type_scope: None,
            on_toggle_target: None,
            value_edit: None,
            picker_filter_edit: None,
            select_open: None,
        }
    }
}

/// The dialog's observable state (React's `useState` values).
#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Debug)]
pub(crate) struct ValueSearchVm {
    pub open: bool,
    pub db_session_id: String,
    pub database: String,
    pub driver_type: Option<String>,
    pub tables: Vec<TableVm>,
    pub value: TextEdit,
    pub mode: ValueSearchMode,
    pub scope: SearchScope,
    pub selected_keys: BTreeSet<String>,
    pub picker_filter: TextEdit,
    pub type_scope: TypeScope,
    pub progress: Option<SearchProgress>,
    /// The question the last run actually asked (never the editable input).
    pub last_run: Option<(String, ValueSearchMode)>,
    pub select_open_values: BTreeMap<String, bool>,
}

impl Default for ValueSearchVm {
    fn default() -> Self {
        Self {
            open: false,
            db_session_id: String::new(),
            database: String::new(),
            driver_type: None,
            tables: Vec::new(),
            value: TextEdit::new(String::new()),
            mode: ValueSearchMode::Contains,
            scope: SearchScope::All,
            selected_keys: BTreeSet::new(),
            picker_filter: TextEdit::new(String::new()),
            type_scope: TypeScope::Text,
            progress: None,
            last_run: None,
            select_open_values: BTreeMap::new(),
        }
    }
}

pub(crate) fn is_busy(progress: Option<&SearchProgress>) -> bool {
    matches!(
        progress.map(|p| p.status),
        Some(SearchStatus::Running) | Some(SearchStatus::Paused)
    )
}
