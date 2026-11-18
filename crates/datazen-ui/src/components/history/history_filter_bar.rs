//! Filter row for the global query-history dialog.
//!
//! Ported from `packages/frontend/src/components/history/HistoryFilterBar.tsx`.
//! Purely presentational: owns no state, emits changes via callbacks.
//!
//! This component currently has no production consumer; it will be wired by the
//! query-history dialog/view migration track. Reusable API items are kept with
//! per-item `#[allow(dead_code)]` (not file-level) until wired.
//!
//! Search icon placement: React absolutely positions the icon inside the input.
//! Rune `TextField` base is reused for the editable input; the search icon is
//! positioned absolutely inside the search input container with left inset so its
//! rect falls inside the input rect (icon.x >= input.x, icon.x + icon.w <= input.x + 40),
//! with `TextField` `pad_x` reserving text space (Rune layout equivalent).
//!
//! ## Keys
//!
//! - `history-filter-bar`: root row
//! - `history-filter-bar-search`: search input container (focusable text field inside)
//! - `history-filter-bar-search-icon`: search icon (inside input bounds)
//! - `history-filter-bar-search-field`: underlying text field element
//! - `history-filter-bar-connection-trigger`: connection scope select trigger
//! - `history-filter-bar-database-trigger`: database select trigger (conditional)
//! - `history-filter-bar-schema-trigger`: schema select trigger (conditional)
//! - `history-filter-bar-status-trigger`: status select trigger
//! - `history-filter-bar-range-trigger`: time range select trigger
//! - `history-filter-bar-sort-trigger`: sort select trigger
//! - `history-filter-bar-reset-filters`: reset button (conditional)

use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::ICON_SEARCH;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;

/// Time range filter. Mirrors frontend `HistoryRange`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) enum HistoryRange {
    #[default]
    All,
    Today,
    D7,
    D30,
    D90,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl HistoryRange {
    pub(crate) fn as_value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Today => "today",
            Self::D7 => "7d",
            Self::D30 => "30d",
            Self::D90 => "90d",
        }
    }

    pub(crate) fn from_value(v: &str) -> Self {
        match v {
            "today" => Self::Today,
            "7d" => Self::D7,
            "30d" => Self::D30,
            "90d" => Self::D90,
            _ => Self::All,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => t("query.historyRangeAll"),
            Self::Today => t("query.historyRangeToday"),
            Self::D7 => t("query.historyRange7d"),
            Self::D30 => t("query.historyRange30d"),
            Self::D90 => t("query.historyRange90d"),
        }
    }
}

/// Sort order. Mirrors frontend `HistorySort`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) enum HistorySort {
    #[default]
    Recent,
    Oldest,
    Slowest,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl HistorySort {
    pub(crate) fn as_value(self) -> &'static str {
        match self {
            Self::Recent => "recent",
            Self::Oldest => "oldest",
            Self::Slowest => "slowest",
        }
    }

    pub(crate) fn from_value(v: &str) -> Self {
        match v {
            "oldest" => Self::Oldest,
            "slowest" => Self::Slowest,
            _ => Self::Recent,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Recent => t("query.historySortRecent"),
            Self::Oldest => t("query.historySortOldest"),
            Self::Slowest => t("query.historySortSlowest"),
        }
    }
}

/// Status filter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) enum HistoryStatus {
    #[default]
    All,
    Success,
    Failed,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl HistoryStatus {
    pub(crate) fn as_value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn from_value(v: &str) -> Self {
        match v {
            "success" => Self::Success,
            "failed" => Self::Failed,
            _ => Self::All,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => t("query.historyStatusAll"),
            Self::Success => t("query.historyStatusSuccess"),
            Self::Failed => t("query.historyStatusFailed"),
        }
    }
}

/// Filter/query state. Mirrors frontend `HistoryQueryState`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct HistoryQueryState {
    pub search: String,
    pub connection_id: String,
    pub database: String,
    pub schema: String,
    pub status: HistoryStatus,
    pub range: HistoryRange,
    pub sort: HistorySort,
}

impl Default for HistoryQueryState {
    fn default() -> Self {
        Self {
            search: String::new(),
            connection_id: "all".to_string(),
            database: String::new(),
            schema: String::new(),
            status: HistoryStatus::All,
            range: HistoryRange::All,
            sort: HistorySort::Recent,
        }
    }
}

/// Connection option for scope select.
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct HistoryConnectionOption {
    pub id: String,
    pub name: String,
}

/// Which select dropdown is currently open (owned by caller state).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) enum HistoryFilterOpen {
    #[default]
    None,
    Connection,
    Database,
    Schema,
    Status,
    Range,
    Sort,
}

/// Pure-presentational filter bar builder.
///
/// Select dropdowns render into `layer` (caller provides overlay layer vector).
/// `open` indicates which dropdown is open; callers toggle/close via callbacks.
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct HistoryFilterBar<'a, S: 'static> {
    value: HistoryQueryState,
    search_edit: &'a TextEdit,
    connections: Vec<HistoryConnectionOption>,
    databases: Vec<String>,
    schemas: Vec<String>,
    open: HistoryFilterOpen,
    on_search_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
    on_set_connection: Option<Box<dyn Fn(&mut S, String) + 'static>>,
    on_set_database: Option<Box<dyn Fn(&mut S, String) + 'static>>,
    on_set_schema: Option<Box<dyn Fn(&mut S, String) + 'static>>,
    on_set_status: Option<Box<dyn Fn(&mut S, HistoryStatus) + 'static>>,
    on_set_range: Option<Box<dyn Fn(&mut S, HistoryRange) + 'static>>,
    on_set_sort: Option<Box<dyn Fn(&mut S, HistorySort) + 'static>>,
    on_reset: Option<Box<dyn Fn(&mut S) + 'static>>,
    on_toggle_open: Option<Box<dyn Fn(&mut S, HistoryFilterOpen) + 'static>>,
    on_close: Option<Box<dyn Fn(&mut S) + 'static>>,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl<'a, S: 'static> HistoryFilterBar<'a, S> {
    pub(crate) fn new(value: HistoryQueryState, search_edit: &'a TextEdit) -> Self {
        Self {
            value,
            search_edit,
            connections: Vec::new(),
            databases: Vec::new(),
            schemas: Vec::new(),
            open: HistoryFilterOpen::None,
            on_search_edit: None,
            on_set_connection: None,
            on_set_database: None,
            on_set_schema: None,
            on_set_status: None,
            on_set_range: None,
            on_set_sort: None,
            on_reset: None,
            on_toggle_open: None,
            on_close: None,
        }
    }

    pub(crate) fn connections(mut self, connections: Vec<HistoryConnectionOption>) -> Self {
        self.connections = connections;
        self
    }

    pub(crate) fn databases(mut self, databases: Vec<String>) -> Self {
        self.databases = databases;
        self
    }

    pub(crate) fn schemas(mut self, schemas: Vec<String>) -> Self {
        self.schemas = schemas;
        self
    }

    pub(crate) fn open(mut self, open: HistoryFilterOpen) -> Self {
        self.open = open;
        self
    }

    pub(crate) fn on_search_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_search_edit = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_set_connection(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_set_connection = Some(Box::new(f));
        self
    }

    pub(crate) fn on_set_database(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_set_database = Some(Box::new(f));
        self
    }

    pub(crate) fn on_set_schema(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_set_schema = Some(Box::new(f));
        self
    }

    pub(crate) fn on_set_status(mut self, f: impl Fn(&mut S, HistoryStatus) + 'static) -> Self {
        self.on_set_status = Some(Box::new(f));
        self
    }

    pub(crate) fn on_set_range(mut self, f: impl Fn(&mut S, HistoryRange) + 'static) -> Self {
        self.on_set_range = Some(Box::new(f));
        self
    }

    pub(crate) fn on_set_sort(mut self, f: impl Fn(&mut S, HistorySort) + 'static) -> Self {
        self.on_set_sort = Some(Box::new(f));
        self
    }

    pub(crate) fn on_reset(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_reset = Some(Box::new(f));
        self
    }

    pub(crate) fn on_toggle_open(
        mut self,
        f: impl Fn(&mut S, HistoryFilterOpen) + 'static,
    ) -> Self {
        self.on_toggle_open = Some(Box::new(f));
        self
    }

    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Box::new(f));
        self
    }

    /// Build the filter bar. `layer` receives select dropdown overlays.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let value = self.value.clone();

        // search: TextField base reused as the editable input. React places icon with
        // absolute positioning inside the input; Rune here uses an absolutely positioned
        // icon inside the search container so icon.x >= input.x and icon.x + icon.w <= input.x + 40,
        // with TextField pad_x leaving visual space (equivalent placement).
        let search_input = if let Some(edit) = &self.on_search_edit {
            let edit = edit.clone();
            TextField::new("history-filter-bar-search-field", self.search_edit)
                .height(32.0)
                .pad_x(28.0)
                .placeholder(t("query.searchHistory"))
                .on_edit(move |s: &mut S| edit(s))
                .build(dark)
        } else {
            row()
                .key("history-filter-bar-search-field")
                .h(32.0)
                .w_full()
                .px(28.0)
                .center_y()
                .child(
                    text(t("query.searchHistory"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_faint),
                )
        };

        let search = row()
            .key("history-filter-bar-search")
            .grow(1.0)
            .min_w(140.0)
            .h(32.0)
            .items_center()
            .child(search_input.grow(1.0).min_w(0.0))
            .child(
                icon(&ICON_SEARCH, 16.0)
                    .key("history-filter-bar-search-icon")
                    .color(k.c.text_faint)
                    .a11y_hidden()
                    .absolute(8.0, 8.0),
            );

        let mut bar = row()
            .key("history-filter-bar-row")
            .w_full()
            .items_center()
            .gap(8.0)
            .px(0.0)
            .py(0.0)
            .child(search);

        let on_toggle_open = self.on_toggle_open.map(std::rc::Rc::new);
        let on_close = self.on_close.map(std::rc::Rc::new);
        let on_set_connection = self.on_set_connection.map(std::rc::Rc::new);
        let on_set_database = self.on_set_database.map(std::rc::Rc::new);
        let on_set_schema = self.on_set_schema.map(std::rc::Rc::new);
        let on_set_status = self.on_set_status.map(std::rc::Rc::new);
        let on_set_range = self.on_set_range.map(std::rc::Rc::new);
        let on_set_sort = self.on_set_sort.map(std::rc::Rc::new);

        // connection select (w-32)
        {
            let labels: Vec<String> = std::iter::once(t("query.historyScopeAll").to_string())
                .chain(self.connections.iter().map(|c| c.name.clone()))
                .collect();
            let values: Vec<String> = std::iter::once("all".to_string())
                .chain(self.connections.iter().map(|c| c.id.clone()))
                .collect();
            let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let value_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
            let current_label = if value.connection_id == "all" {
                t("query.historyScopeAll").to_string()
            } else {
                self.connections
                    .iter()
                    .find(|c| c.id == value.connection_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| t("query.historyScopeAll").to_string())
            };
            let mut sel = Select::new("history-filter-bar-connection", &label_refs)
                .values(&value_refs)
                .current(Some(&current_label))
                .width(120.0)
                .open(self.open == HistoryFilterOpen::Connection);
            if let Some(tg) = &on_toggle_open {
                let tg = tg.clone();
                sel = sel.on_toggle(move |s: &mut S| tg(s, HistoryFilterOpen::Connection));
            }
            if let Some(cl) = &on_close {
                let cl = cl.clone();
                sel = sel.on_close(move |s: &mut S| cl(s));
            }
            if let Some(set) = &on_set_connection {
                let set = set.clone();
                sel = sel
                    .on_choose(move |s: &mut S, v: &str, _tx: &mut rune::Tx| set(s, v.to_string()));
            }
            bar = bar.child(sel.build(&k, layer));
        }

        // database select (conditional)
        if value.connection_id != "all" && !self.databases.is_empty() {
            let labels: Vec<String> = std::iter::once(t("query.historyAllDatabases").to_string())
                .chain(self.databases.iter().cloned())
                .collect();
            let values: Vec<String> = std::iter::once("all".to_string())
                .chain(self.databases.iter().cloned())
                .collect();
            let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let value_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
            let current_label = if value.database.is_empty() {
                t("query.historyAllDatabases").to_string()
            } else {
                value.database.clone()
            };
            let mut sel = Select::new("history-filter-bar-database", &label_refs)
                .values(&value_refs)
                .current(Some(&current_label))
                .width(104.0)
                .open(self.open == HistoryFilterOpen::Database);
            if let Some(tg) = &on_toggle_open {
                let tg = tg.clone();
                sel = sel.on_toggle(move |s: &mut S| tg(s, HistoryFilterOpen::Database));
            }
            if let Some(cl) = &on_close {
                let cl = cl.clone();
                sel = sel.on_close(move |s: &mut S| cl(s));
            }
            if let Some(set) = &on_set_database {
                let set = set.clone();
                sel = sel.on_choose(move |s: &mut S, v: &str, _tx: &mut rune::Tx| {
                    let val = if v == "all" {
                        String::new()
                    } else {
                        v.to_string()
                    };
                    set(s, val)
                });
            }
            bar = bar.child(sel.build(&k, layer));
        }

        // schema select (conditional)
        if value.connection_id != "all" && !self.schemas.is_empty() {
            let labels: Vec<String> = std::iter::once(t("query.historyAllSchemas").to_string())
                .chain(self.schemas.iter().cloned())
                .collect();
            let values: Vec<String> = std::iter::once("all".to_string())
                .chain(self.schemas.iter().cloned())
                .collect();
            let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let value_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
            let current_label = if value.schema.is_empty() {
                t("query.historyAllSchemas").to_string()
            } else {
                value.schema.clone()
            };
            let mut sel = Select::new("history-filter-bar-schema", &label_refs)
                .values(&value_refs)
                .current(Some(&current_label))
                .width(104.0)
                .open(self.open == HistoryFilterOpen::Schema);
            if let Some(tg) = &on_toggle_open {
                let tg = tg.clone();
                sel = sel.on_toggle(move |s: &mut S| tg(s, HistoryFilterOpen::Schema));
            }
            if let Some(cl) = &on_close {
                let cl = cl.clone();
                sel = sel.on_close(move |s: &mut S| cl(s));
            }
            if let Some(set) = &on_set_schema {
                let set = set.clone();
                sel = sel.on_choose(move |s: &mut S, v: &str, _tx: &mut rune::Tx| {
                    let val = if v == "all" {
                        String::new()
                    } else {
                        v.to_string()
                    };
                    set(s, val)
                });
            }
            bar = bar.child(sel.build(&k, layer));
        }

        // status select
        {
            let opts = [
                HistoryStatus::All,
                HistoryStatus::Success,
                HistoryStatus::Failed,
            ];
            let labels: Vec<String> = opts.iter().map(|o| o.label().to_string()).collect();
            let values: Vec<String> = opts.iter().map(|o| o.as_value().to_string()).collect();
            let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let value_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
            let mut sel = Select::new("history-filter-bar-status", &label_refs)
                .values(&value_refs)
                .current(Some(value.status.label()))
                .width(104.0)
                .open(self.open == HistoryFilterOpen::Status);
            if let Some(tg) = &on_toggle_open {
                let tg = tg.clone();
                sel = sel.on_toggle(move |s: &mut S| tg(s, HistoryFilterOpen::Status));
            }
            if let Some(cl) = &on_close {
                let cl = cl.clone();
                sel = sel.on_close(move |s: &mut S| cl(s));
            }
            if let Some(set) = &on_set_status {
                let set = set.clone();
                sel = sel.on_choose(move |s: &mut S, v: &str, _tx: &mut rune::Tx| {
                    set(s, HistoryStatus::from_value(v))
                });
            }
            bar = bar.child(sel.build(&k, layer));
        }

        // range select
        {
            let opts = [
                HistoryRange::All,
                HistoryRange::Today,
                HistoryRange::D7,
                HistoryRange::D30,
                HistoryRange::D90,
            ];
            let labels: Vec<String> = opts.iter().map(|o| o.label().to_string()).collect();
            let values: Vec<String> = opts.iter().map(|o| o.as_value().to_string()).collect();
            let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let value_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
            let mut sel = Select::new("history-filter-bar-range", &label_refs)
                .values(&value_refs)
                .current(Some(value.range.label()))
                .width(120.0)
                .open(self.open == HistoryFilterOpen::Range);
            if let Some(tg) = &on_toggle_open {
                let tg = tg.clone();
                sel = sel.on_toggle(move |s: &mut S| tg(s, HistoryFilterOpen::Range));
            }
            if let Some(cl) = &on_close {
                let cl = cl.clone();
                sel = sel.on_close(move |s: &mut S| cl(s));
            }
            if let Some(set) = &on_set_range {
                let set = set.clone();
                sel = sel.on_choose(move |s: &mut S, v: &str, _tx: &mut rune::Tx| {
                    set(s, HistoryRange::from_value(v))
                });
            }
            bar = bar.child(sel.build(&k, layer));
        }

        // sort select
        {
            let opts = [
                HistorySort::Recent,
                HistorySort::Oldest,
                HistorySort::Slowest,
            ];
            let labels: Vec<String> = opts.iter().map(|o| o.label().to_string()).collect();
            let values: Vec<String> = opts.iter().map(|o| o.as_value().to_string()).collect();
            let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let value_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
            let mut sel = Select::new("history-filter-bar-sort", &label_refs)
                .values(&value_refs)
                .current(Some(value.sort.label()))
                .width(120.0)
                .open(self.open == HistoryFilterOpen::Sort);
            if let Some(tg) = &on_toggle_open {
                let tg = tg.clone();
                sel = sel.on_toggle(move |s: &mut S| tg(s, HistoryFilterOpen::Sort));
            }
            if let Some(cl) = &on_close {
                let cl = cl.clone();
                sel = sel.on_close(move |s: &mut S| cl(s));
            }
            if let Some(set) = &on_set_sort {
                let set = set.clone();
                sel = sel.on_choose(move |s: &mut S, v: &str, _tx: &mut rune::Tx| {
                    set(s, HistorySort::from_value(v))
                });
            }
            bar = bar.child(sel.build(&k, layer));
        }

        // reset button (conditional) - kit Button base; key kept for compatibility/tests.
        let def = HistoryQueryState::default();
        let changed = value.search != def.search
            || value.connection_id != def.connection_id
            || value.database != def.database
            || value.schema != def.schema
            || value.status != def.status
            || value.range != def.range;
        if changed {
            let mut reset = Button::new(t("common.reset"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small);
            if let Some(on_reset) = self.on_reset {
                reset = reset.on_press(move |s: &mut S| on_reset(s));
            }
            bar = bar.child(
                reset
                    .build(&k)
                    .key("history-filter-bar-reset-filters"),
            );
        }

        // outer container with bottom border like React (border-b border-edge pb-3)
        col()
            .key("history-filter-bar")
            .w_full()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .border_layout()
            .pb(12.0)
            .child(bar.wrap())
    }
}
