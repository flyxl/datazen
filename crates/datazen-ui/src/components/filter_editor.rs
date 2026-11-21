//! The filter editor, ported from
//! `packages/frontend/src/components/FilterEditor.tsx` (650 lines, T106,
//! split into the slices below).
//!
//! The editor panes of the filter panel (DBR-04): the collapsed summary
//! row, the structured editor, and the two DBR-04 panes (Any Column, Raw
//! SQL) that only render when the caller offers them.
//!
//! ## Slices
//!
//! * S1 (this file, whole): the shell — the collapsed summary row (the
//!   toggle, the applied badge, the dirty marker, the summary text, the
//!   closed add button, the clear button), the null return, the mode row,
//!   the Any Column pane, the Raw SQL pane, the structured pane's logic
//!   row and the chip list. The model computations (dirty, incomplete,
//!   badge, summary, per-condition logic) are pure methods on
//!   [`FilterEditor`].
//!
//! ## Input boundary (cross-segment)
//!
//! * `FilterConditionEditor` — the expanded per-condition editor — is
//!   T056, segment-1, not ported yet. React renders an incomplete or
//!   being-edited condition through it; this port renders the chip for
//!   every condition (the incomplete one carries its marker and blocks
//!   Apply) and records the would-expand row in
//!   [`FilterEditorState::editing_index`] so T056's swap reads the same
//!   record. Noted at push for the coordinator.
//! * `defaultOperatorForColumn` and `OPERATORS_BY_CELL_TYPE` live in
//!   `lib/filterOperators.ts` (T513, segment-2). The minimal data the
//!   add-empty default needs is embedded here
//!   ([`default_operator_for_column`]) and re-homes with T513.
//! * `SHARED_FILTER_OPERATORS` (`types/filters.ts`) is data — the ten
//!   existing plus the nine shared extensions — ported as
//!   [`SHARED_FILTER_OPERATORS`] until the types port re-homes it.
//!
//! ## Key convention
//!
//! React's `data-testid`s become the element keys:
//!
//! | part | key |
//! |---|---|
//! | root | `filter-editor` |
//! | summary toggle | `filter-summary-toggle` |
//! | closed add button | `filter-add-closed` (React reuses `filter-add`; the open one keeps it) |
//! | clear | `filter-clear` |
//! | mode tabs | `filter-mode-{mode}` |
//! | any column pane / input / apply | `filter-any-column-pane` / `any-column-input` / `any-column-apply` |
//! | raw pane / input / apply / hint | `filter-raw-sql-pane` / `raw-where-input` / `raw-where-apply` / `raw-where-hint` |
//! | logic toggles | `filter-logic-and` / `filter-logic-or` |
//! | connector reset | `filter-connector-reset` |
//! | add (open) | `filter-add` |
//! | export / preview / collapse / apply | `filter-export-filtered` / `filter-preview-sql` / `filter-collapse` / `filter-apply` |
//! | error banners | `filter-error` (per pane) / `filter-incomplete-error` |
//! | condition rows | `filter-row-{index}` (new; React keys by column-operator-index and marks `data-filter-index`) |
//! | incomplete markers | `filter-incomplete` |
//!
//! ## Deliberate boundaries
//!
//! * **`aria-busy` / `aria-disabled`** become the filter bar's loading
//!   shape: the root and every control carry `.disabled(true)`, the
//!   controls leave the tab order, and no press handler fires while
//!   loading.
//! * **The scroll-into-view effect** (`requestAnimationFrame` +
//!   `scrollIntoView`) is the runtime's scroll model, not a component
//!   behavior; the new-row auto-expand (`setEditingIndex(last)`) is
//!   recorded in state for T056.
//! * **The Any Column / Raw SQL inputs** are `ui/field` `TextField`s;
//!   React's Enter-to-apply (`isComposing` guard included) is the
//!   field's submit handler where the wiring owns focus — the buttons
//!   carry the same applies here.
//! * **`title` tooltips** become `help` labels (the a11y name); no hover
//!   tooltip engine exists in the test surface.

// The production consumer is the data-grid wiring (the table browse
// track); until then the component tests are the only callers.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, icon, row, text, El};

use crate::components::filter::{
    condition_summary, FilterCondition, FilterConditionChip, FilterConnector,
    FilterConnectorToggle, FilterValue,
};
use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT, ICON_DOWNLOAD, ICON_PLUS, ICON_TERMINAL};
use crate::support::cell_types::{resolve_cell_type, CellType, CellTypeHints};
use crate::table_data_filter_utils::{filter_draft_equals_applied, is_complete_filter};
use crate::ui::banner::{Banner, BannerVariant};
use crate::ui::kit::kit;

/// A column the filter editor can filter on (`components/filter/types.ts`):
/// the name plus the driver's type string, which decides the operators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FilterColumn {
    pub name: String,
    /// The driver's type string; unknown types fall back to the text
    /// operators.
    pub column_type: Option<String>,
}

/// The pane the panel shows (`FilterPanelMode`): the structured editor,
/// Any Column, or Raw SQL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FilterPanelMode {
    Structured,
    AnyColumn,
    RawSql,
}

impl FilterPanelMode {
    /// The tab's key id (`filter-mode-{value}`).
    pub(crate) fn id(self) -> &'static str {
        match self {
            FilterPanelMode::Structured => "structured",
            FilterPanelMode::AnyColumn => "anyColumn",
            FilterPanelMode::RawSql => "rawSql",
        }
    }
}

/// `SHARED_FILTER_OPERATORS` (`types/filters.ts:38`): the operators the
/// host renders itself — the existing ten plus the nine shared
/// extensions. The default whitelist of a driver.
pub(crate) const SHARED_FILTER_OPERATORS: &[&str] = &[
    "eq",
    "ne",
    "gt",
    "lt",
    "gte",
    "lte",
    "like",
    "in",
    "isNull",
    "isNotNull",
    "notIn",
    "between",
    "notBetween",
    "contains",
    "notContains",
    "startsWith",
    "endsWith",
    "isEmpty",
    "isNotEmpty",
];

/// `OPERATORS_BY_CELL_TYPE` (`lib/filterOperators.ts:101`), the shared
/// rows the add-empty default reads. T513 (segment-2) owns the full
/// table port and re-homes this one.
fn operators_by_cell_type(cell_type: CellType) -> &'static [&'static str] {
    match cell_type {
        CellType::Text | CellType::Unknown => &[
            "eq", "ne", "gt", "lt", "gte", "lte", "like", "contains", "notContains", "startsWith",
            "endsWith", "in", "notIn", "isNull", "isNotNull", "isEmpty", "isNotEmpty",
        ],
        CellType::Enum => &["eq", "ne", "in", "notIn", "isNull", "isNotNull"],
        CellType::Number | CellType::Date | CellType::Time | CellType::DateTime => &[
            "eq", "ne", "gt", "lt", "gte", "lte", "between", "notIn", "in", "isNull", "isNotNull",
        ],
        CellType::Bool | CellType::Json | CellType::Bytes => &["eq", "ne", "isNull", "isNotNull"],
        CellType::Uuid => &["eq", "ne", "in", "notIn", "isNull", "isNotNull"],
    }
}

/// `defaultOperatorForColumn` (`lib/filterOperators.ts:343`): the
/// operator a new condition on the column starts with — `eq` when the
/// column can take it, the first operator otherwise.
pub(crate) fn default_operator_for_column(
    data_type: Option<&str>,
    hints: Option<&CellTypeHints>,
) -> &'static str {
    let operators = operators_by_cell_type(resolve_cell_type(data_type, hints));
    if operators.contains(&"eq") {
        "eq"
    } else {
        operators.first().copied().unwrap_or("eq")
    }
}

/// The empty host state for unwired hosts (the DBR-04 panes read the
/// draft fields; a caller that never wired a state gets an empty draft).
fn filter_editor_empty_state() -> &'static FilterEditorState {
    static EMPTY: std::sync::OnceLock<FilterEditorState> = std::sync::OnceLock::new();
    EMPTY.get_or_init(|| FilterEditorState {
        editing_index: None,
        any_column_edit: rune::TextEdit::new(String::new()),
        raw_where_edit: rune::TextEdit::new(String::new()),
    })
}

/// The component-local state React keeps in `useState`/controlled
/// inputs: the row shown expanded and the two DBR-04 draft fields.
/// Incomplete rows would expand through T056; the record is what its
/// swap reads.
#[derive(Debug, Default)]
pub(crate) struct FilterEditorState {
    /// React's `editingIndex`: the condition shown as full inputs (the
    /// chip's edit action sets it; Apply clears it).
    pub editing_index: Option<usize>,
    /// The Any Column draft (React's controlled `anyColumnValue`).
    pub any_column_edit: rune::TextEdit,
    /// The Raw SQL draft (React's controlled `rawWhereValue`).
    pub raw_where_edit: rune::TextEdit,
}

/// The filter editor. Build it with the props, the host state (for the
/// editing row), and the callbacks; `build` returns `None` for React's
/// null return.
pub(crate) struct FilterEditor<'a, S: 'static> {
    columns: &'a [FilterColumn],
    supported_operators: Option<&'a [&'static str]>,
    cell_type_hints: Option<&'a CellTypeHints>,
    applied_filters: &'a [FilterCondition],
    applied_logic: FilterConnector,
    draft_filters: &'a [FilterCondition],
    draft_logic: FilterConnector,
    open: bool,
    loading: bool,
    apply_error: Option<&'a str>,
    mode: FilterPanelMode,
    raw_where_supported: bool,
    any_column_skipped_count: usize,
    any_column_searchable_count: usize,
    applied_raw_where: Option<&'a str>,
    raw_where_context: Option<&'a str>,
    state: Option<&'a FilterEditorState>,
    host: Option<Rc<dyn Fn(&mut S) -> &mut FilterEditorState>>,
    on_open_change: Option<Rc<dyn Fn(&mut S, bool)>>,
    on_logic_change: Option<Rc<dyn Fn(&mut S, FilterConnector)>>,
    on_change: Option<Rc<dyn Fn(&mut S, usize, FilterCondition)>>,
    on_add: Option<Rc<dyn Fn(&mut S, FilterCondition)>>,
    on_remove: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_apply: Option<Rc<dyn Fn(&mut S)>>,
    on_clear: Option<Rc<dyn Fn(&mut S)>>,
    on_mode_change: Option<Rc<dyn Fn(&mut S, FilterPanelMode)>>,
    on_any_column_value_change: Option<Rc<dyn Fn(&mut S, String)>>,
    on_apply_any_column: Option<Rc<dyn Fn(&mut S, String)>>,
    on_raw_where_value_change: Option<Rc<dyn Fn(&mut S, String)>>,
    on_apply_raw_where: Option<Rc<dyn Fn(&mut S, String)>>,
    on_preview_sql: Option<Rc<dyn Fn(&mut S)>>,
    on_export_filtered: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> FilterEditor<'a, S> {
    /// The editor over the caller's props: the columns, the applied and
    /// draft lists, the open flag, and the DBR-04 inputs.
    pub(crate) fn new(
        columns: &'a [FilterColumn],
        applied_filters: &'a [FilterCondition],
        applied_logic: FilterConnector,
        draft_filters: &'a [FilterCondition],
        draft_logic: FilterConnector,
        open: bool,
    ) -> Self {
        Self {
            columns,
            supported_operators: None,
            cell_type_hints: None,
            applied_filters,
            applied_logic,
            draft_filters,
            draft_logic,
            open,
            loading: false,
            apply_error: None,
            mode: FilterPanelMode::Structured,
            raw_where_supported: false,
            any_column_skipped_count: 0,
            any_column_searchable_count: 0,
            applied_raw_where: None,
            raw_where_context: None,
            state: None,
            host: None,
            on_open_change: None,
            on_logic_change: None,
            on_change: None,
            on_add: None,
            on_remove: None,
            on_apply: None,
            on_clear: None,
            on_mode_change: None,
            on_any_column_value_change: None,
            on_apply_any_column: None,
            on_raw_where_value_change: None,
            on_apply_raw_where: None,
            on_preview_sql: None,
            on_export_filtered: None,
        }
    }

    /// The host state, when the caller provided one.
    fn state(&self) -> Option<&'a FilterEditorState> {
        self.state
    }

    /// The host state or a shared default: the DBR-04 panes read the
    /// draft fields, so a caller that never wired a state still gets an
    /// empty draft (nothing to apply).
    fn state_or_default(&self) -> &FilterEditorState {
        match self.state {
            Some(state) => state,
            None => filter_editor_empty_state(),
        }
    }

    /// The host state carrying the expanded row and the draft fields.
    pub(crate) fn host_state(mut self, state: &'a FilterEditorState) -> Self {
        self.state = Some(state);
        self
    }

    /// The state accessor, so the chip's edit action records the row.
    pub(crate) fn host_mut(
        mut self,
        f: impl Fn(&mut S) -> &mut FilterEditorState + 'static,
    ) -> Self {
        self.host = Some(Rc::new(f));
        self
    }

    pub(crate) fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub(crate) fn apply_error(mut self, error: Option<&'a str>) -> Self {
        self.apply_error = error;
        self
    }

    pub(crate) fn mode(mut self, mode: FilterPanelMode) -> Self {
        self.mode = mode;
        self
    }

    /// The operators this database serves (React's `supportedOperators`):
    /// `None` keeps the shared set. The grid shell (T163) threads the
    /// driver's declaration through, exactly as React's `filterOperators`
    /// prop does.
    pub(crate) fn supported_operators(mut self, operators: &'a [&'static str]) -> Self {
        self.supported_operators = Some(operators);
        self
    }

    pub(crate) fn raw_where_supported(mut self, supported: bool) -> Self {
        self.raw_where_supported = supported;
        self
    }

    /// The Any Column scope counts (the draft itself lives in
    /// [`FilterEditorState::any_column_edit`]).
    pub(crate) fn any_column(mut self, searchable: usize, skipped: usize) -> Self {
        self.any_column_searchable_count = searchable;
        self.any_column_skipped_count = skipped;
        self
    }

    pub(crate) fn raw_where(mut self, applied: Option<&'a str>, context: Option<&'a str>) -> Self {
        self.applied_raw_where = applied;
        self.raw_where_context = context;
        self
    }

    pub(crate) fn cell_type_hints(mut self, hints: Option<&'a CellTypeHints>) -> Self {
        self.cell_type_hints = hints;
        self
    }

    pub(crate) fn on_open_change(
        mut self,
        f: impl Fn(&mut S, bool) + 'static,
    ) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_logic_change(
        mut self,
        f: impl Fn(&mut S, FilterConnector) + 'static,
    ) -> Self {
        self.on_logic_change = Some(Rc::new(f));
        self
    }

    /// React's `onChange(index, next)`.
    pub(crate) fn on_change(
        mut self,
        f: impl Fn(&mut S, usize, FilterCondition) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_add(mut self, f: impl Fn(&mut S, FilterCondition) + 'static) -> Self {
        self.on_add = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_remove(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_remove = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_apply(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_apply = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_clear(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_mode_change(
        mut self,
        f: impl Fn(&mut S, FilterPanelMode) + 'static,
    ) -> Self {
        self.on_mode_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_any_column_value_change(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_any_column_value_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_apply_any_column(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_apply_any_column = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_raw_where_value_change(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_raw_where_value_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_apply_raw_where(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_apply_raw_where = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_preview_sql(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_preview_sql = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_export_filtered(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_export_filtered = Some(Rc::new(f));
        self
    }

    // ---- the model computations (React's useMemo/derived values) ------

    /// The applied raw predicate, when the Raw SQL channel is the applied
    /// one (`appliedRawWhere != null && !== ''`).
    fn raw_applied(&self) -> bool {
        self.applied_raw_where.is_some_and(|raw| !raw.is_empty())
    }

    /// One applied channel at a time (DBR-04): the badge counts either,
    /// never both.
    fn badge_count(&self) -> usize {
        if self.raw_applied() {
            1
        } else {
            self.applied_filters.len()
        }
    }

    /// React's `dirty`: the draft differs from the applied list.
    fn dirty(&self) -> bool {
        !filter_draft_equals_applied(
            self.draft_filters,
            self.draft_logic,
            self.applied_filters,
            self.applied_logic,
        )
    }

    /// The first condition that cannot be sent to the database — it is
    /// marked in the list and it blocks Apply.
    pub(crate) fn incomplete(&self) -> Option<&FilterCondition> {
        self.draft_filters.iter().find(|f| !is_complete_filter(f))
    }

    /// Whether any condition after the first carries a connector of its
    /// own (the request-wide logic row is then disabled).
    fn per_condition_logic(&self) -> bool {
        self.draft_filters
            .iter()
            .skip(1)
            .any(|f| f.connector.is_some())
    }

    /// The collapsed summary: the raw predicate summary, the no-active
    /// placeholder, or the conditions joined with the connector actually
    /// in effect for each.
    pub(crate) fn summary_text(&self) -> String {
        let applied_count = self.applied_filters.len();
        if self.raw_applied() && applied_count == 0 {
            return t("filter.rawAppliedSummary")
                .replacen("{predicate}", self.applied_raw_where.unwrap_or(""), 1);
        }
        if applied_count == 0 {
            return t("filter.noActive").to_string();
        }
        self.applied_filters
            .iter()
            .enumerate()
            .map(|(index, filter)| {
                let condition = condition_summary(filter);
                if index == 0 {
                    condition
                } else {
                    let connector = filter.connector.unwrap_or(self.applied_logic);
                    let word = connector.label().to_uppercase();
                    format!("{word} {condition}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Whether the mode row is rendered at all: only with someone to
    /// switch to.
    fn mode_row_visible(&self) -> bool {
        self.on_mode_change.is_some()
            && (self.on_apply_any_column.is_some() || self.raw_where_supported)
    }

    /// React's `addEmpty`: a new condition on the first column with the
    /// column's default operator.
    fn add_empty_condition(&self) -> FilterCondition {
        let column = self.columns.first();
        FilterCondition::new(
            column.map(|c| c.name.clone()).unwrap_or_default(),
            default_operator_for_column(
                column.and_then(|c| c.column_type.as_deref()),
                self.cell_type_hints,
            ),
            FilterValue::Single(String::new()),
        )
    }

    // ---- the render -----------------------------------------------------

    /// The editor, or `None` when closed with nothing applied, drafted,
    /// or raw-applied (React's null return).
    pub(crate) fn build(self, dark: bool) -> Option<El> {
        if !self.open
            && self.applied_filters.is_empty()
            && self.draft_filters.is_empty()
            && !self.raw_applied()
        {
            return None;
        }
        let k = kit(dark);
        let mut root = col()
            .key("filter-editor")
            .w_full()
            .shrink()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .fill(k.c.surface_base)
            .items_stretch();
        if self.loading {
            root = root.disabled(true);
        }
        root = root.child(self.header_row(dark));
        if self.open {
            root = root.child(self.open_body(dark));
        }
        Some(root)
    }

    /// The collapsed summary row: `flex h-8 items-center gap-1.5 px-2`.
    fn header_row(&self, dark: bool) -> El {
        let k = kit(dark);
        let badge_count = self.badge_count();
        let mut header = row()
            .key("filter-header")
            .w_full()
            .h(32.0)
            .items_center()
            .gap(6.0)
            .px(8.0);

        // The summary toggle: chevron + FILTER + badge + dirty + summary.
        let chevron = if self.open {
            &ICON_CHEVRON_DOWN
        } else {
            &ICON_CHEVRON_RIGHT
        };
        let summary = self.summary_text();
        let mut toggle = row()
            .key("filter-summary-toggle")
            .role("button")
            .label(t("filter.filter"))
            .expanded(self.open)
            .grow(1.0)
            .min_w(0.0)
            .items_center()
            .gap(6.0)
            .px(4.0)
            .py(2.0)
            .radius(k.radius.sm)
            .child(icon(chevron, 14.0).key("chevron").color(k.c.text_muted).a11y_hidden())
            .child(
                text(t("filter.filter"))
                    .key("label")
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_muted)
                    .shrink(),
            );
        if badge_count > 0 {
            let badge_text = if self.raw_applied() {
                t("filter.rawBadge").to_string()
            } else {
                t("filter.activeCount")
                    .replacen("{count}", &self.applied_filters.len().to_string(), 1)
            };
            toggle = toggle.child(
                text(badge_text)
                    .key("badge")
                    .px(6.0)
                    .radius(k.radius.xs)
                    .font(k.ty.small)
                    .color(k.c.text_accent)
                    .fill(k.alpha(k.c.bg_accent, 0.15))
                    .shrink(),
            );
        }
        if self.dirty() {
            toggle = toggle.child(
                text(t("filter.unapplied"))
                    .key("unapplied")
                    .font(k.ty.small)
                    .color(crate::ui::palette::ai_amber_text(dark))
                    .shrink(),
            );
        }
        toggle = toggle.child(
            text(summary)
                .key("summary")
                .min_w(0.0)
                .grow(1.0)
                .truncate()
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint),
        );
        if let Some(on_open_change) = &self.on_open_change {
            if !self.loading {
                let open = self.open;
                let on_open_change = Rc::clone(on_open_change);
                toggle = toggle.on_press(move |s: &mut S| on_open_change(s, !open));
            }
        } else {
            toggle = toggle.disabled(true);
        }
        header = header.child(toggle);

        // The closed add button: open + add-empty in one press (React
        // calls `onOpenChange(true)` then `addEmpty()`).
        if !self.open {
            let add_condition = self.add_empty_condition();
            let add_open = self.on_open_change.clone();
            let add_on = self.on_add.clone().map(|on_add| {
                Rc::new(move |s: &mut S| {
                    if let Some(on_open_change) = &add_open {
                        on_open_change(s, true);
                    }
                    on_add(s, add_condition.clone());
                }) as Rc<dyn Fn(&mut S)>
            });
            header = header.child(small_ghost_button(
                "filter-add-closed",
                Some(&ICON_PLUS),
                t("filter.add"),
                self.columns.is_empty() || self.loading,
                add_on,
            ));
        }
        // The clear button, with a badge to clear.
        if badge_count > 0 {
            header = header.child(small_text_button(
                "filter-clear",
                t("filter.clear"),
                self.loading,
                self.on_clear.clone(),
            ));
        }
        header
    }
}

impl<'a, S: 'static> FilterEditor<'a, S> {
    /// The open body: `border-t px-2 pb-2 pt-1.5` with the mode row and
    /// the active pane.
    fn open_body(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut body = col()
            .key("filter-body")
            .w_full()
            .px(8.0)
            .pb(8.0)
            .pt(6.0)
            .gap(6.0)
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
            .items_stretch();
        if self.mode_row_visible() {
            body = body.child(self.mode_row(dark));
        }
        if self.mode == FilterPanelMode::AnyColumn && self.on_apply_any_column.is_some() {
            body = body.child(self.any_column_pane(dark));
        } else if self.mode == FilterPanelMode::RawSql
            && self.raw_where_supported
            && self.on_apply_raw_where.is_some()
        {
            body = body.child(self.raw_sql_pane(dark));
        } else {
            body = body.child(self.structured_pane(dark));
        }
        body
    }

    /// The mode row (`role="tablist"`): the structured tab plus Any
    /// Column when offered and Raw SQL when supported.
    fn mode_row(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut modes = vec![FilterPanelMode::Structured];
        if self.on_apply_any_column.is_some() {
            modes.push(FilterPanelMode::AnyColumn);
        }
        if self.raw_where_supported {
            modes.push(FilterPanelMode::RawSql);
        }
        let mut row_el = row()
            .key("filter-mode-row")
            .role("tablist")
            .label(t("filter.modeRowLabel"))
            .radius(k.radius.sm)
            .border(1.0, k.c.border_base)
            .shrink();
        for mode in modes {
            let selected = self.mode == mode;
            let mut tab = row()
                .key(format!("filter-mode-{}", mode.id()))
                .role("tab")
                .focus_selected(selected)
                .px(6.0)
                .py(2.0)
                .child(
                    text(self.mode_label(mode))
                        .key("label")
                        .font(k.ty.small)
                        .color(if selected { k.c.text_accent } else { k.c.text_faint }),
                );
            if selected {
                tab = tab.fill(k.alpha(k.c.bg_accent, 0.15));
            }
            if !self.loading {
                if let Some(on_mode_change) = &self.on_mode_change {
                    let on_mode_change = Rc::clone(on_mode_change);
                    tab = tab.on_press(move |s: &mut S| on_mode_change(s, mode));
                }
            } else {
                tab = tab.disabled(true);
            }
            row_el = row_el.child(tab);
        }
        row_el
    }

    fn mode_label(&self, mode: FilterPanelMode) -> &'static str {
        match mode {
            FilterPanelMode::Structured => t("filter.modeStructured"),
            FilterPanelMode::AnyColumn => t("filter.modeAnyColumn"),
            FilterPanelMode::RawSql => t("filter.modeRawSql"),
        }
    }

    /// The Any Column pane: the error banner, the search field with its
    /// apply button, and the scope line.
    fn any_column_pane(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut pane = col()
            .key("filter-any-column-pane")
            .w_full()
            .gap(6.0)
            .items_stretch();
        if let Some(banner) = error_banner("filter-error", self.apply_error, &k) {
            pane = pane.child(banner);
        }
        let mut field = row().key("any-column-field").w_full().items_center().gap(6.0);
        // The search field: React's `h-7 min-w-0 flex-1 rounded border
        // border-edge bg-surface px-2 text-xs`. The draft lives in the
        // host state's `any_column_edit` (a controlled input), so the
        // binding and the value read go through the state accessor.
        let mut input = crate::ui::field::TextField::new("any-column-input", &self.state_or_default().any_column_edit)
            .height(28.0)
            .placeholder(t("filter.anyColumnPlaceholder"))
            .grow();
        if self.loading {
            input = input.disabled(true);
        }
        if let Some(host) = &self.host {
            let host = Rc::clone(host);
            input = input.on_edit(move |s: &mut S| &mut host(s).any_column_edit);
        }
        field = field.child(input.build(dark));
        let can_apply = !self.state_or_default().any_column_edit.text.trim().is_empty() && !self.loading;
        let on_apply_any_column = self.on_apply_any_column.clone();
        let apply_host = self.host.clone();
        let any_apply = on_apply_any_column.map(|on_apply| {
            Rc::new(move |s: &mut S| {
                let value = apply_host
                    .as_ref()
                    .map(|host| host(s).any_column_edit.text.clone())
                    .unwrap_or_default();
                on_apply(s, value);
            }) as Rc<dyn Fn(&mut S)>
        });
        field = field.child(small_ghost_button(
            "any-column-apply",
            None,
            t("filter.apply"),
            !can_apply,
            any_apply,
        ));
        pane = pane.child(field);
        let scope = t("filter.anyColumnScope")
            .replacen("{searchable}", &self.any_column_searchable_count.to_string(), 1)
            .replacen("{skipped}", &self.any_column_skipped_count.to_string(), 1);
        pane = pane.child(
            text(scope)
                .key("any-column-scope")
                .font(k.ty.small)
                .color(k.c.text_muted),
        );
        pane
    }

    /// The Raw SQL pane: the error banner, the predicate field with its
    /// apply button, and the context/hint line.
    fn raw_sql_pane(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut pane = col()
            .key("filter-raw-sql-pane")
            .w_full()
            .gap(6.0)
            .items_stretch();
        if let Some(banner) = error_banner("filter-error", self.apply_error, &k) {
            pane = pane.child(banner);
        }
        let mut field = row().key("raw-where-field").w_full().items_center().gap(6.0);
        let mut input = crate::ui::field::TextField::new("raw-where-input", &self.state_or_default().raw_where_edit)
            .height(28.0)
            .placeholder(t("filter.rawPlaceholder"))
            .grow();
        if self.loading {
            input = input.disabled(true);
        }
        if let Some(host) = &self.host {
            let host = Rc::clone(host);
            input = input.on_edit(move |s: &mut S| &mut host(s).raw_where_edit);
        }
        field = field.child(input.build(dark));
        let can_apply = !self.state_or_default().raw_where_edit.text.trim().is_empty() && !self.loading;
        let on_apply_raw_where = self.on_apply_raw_where.clone();
        let apply_host = self.host.clone();
        let raw_apply = on_apply_raw_where.map(|on_apply| {
            Rc::new(move |s: &mut S| {
                let value = apply_host
                    .as_ref()
                    .map(|host| host(s).raw_where_edit.text.clone())
                    .unwrap_or_default();
                on_apply(s, value);
            }) as Rc<dyn Fn(&mut S)>
        });
        field = field.child(small_ghost_button(
            "raw-where-apply",
            None,
            t("filter.apply"),
            !can_apply,
            raw_apply,
        ));
        pane = pane.child(field);
        let hint = match self.raw_where_context {
            Some(context) => t("filter.rawQueryContext").replacen("{context}", context, 1),
            None => t("filter.rawHint").to_string(),
        };
        pane = pane.child(
            text(hint)
                .key("raw-where-hint")
                .font(k.ty.small)
                .color(k.c.text_muted),
        );
        pane
    }
}

impl<'a, S: 'static> FilterEditor<'a, S> {
    /// The structured pane: the logic row (the and/or toggles, the
    /// connector reset, add, export/preview, collapse, apply) and the
    /// chip list with its banners.
    fn structured_pane(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut pane = col()
            .key("filter-structured-pane")
            .w_full()
            .gap(6.0)
            .items_stretch();
        pane = pane.child(self.logic_row(dark));
        pane = pane.child(self.chip_list(dark));
        pane
    }

    /// The logic row: `mb-1.5 flex flex-nowrap items-center gap-1.5` with
    /// the and/or toggles, the connector reset, add, and the right side
    /// (export / preview / collapse / apply).
    fn logic_row(&self, dark: bool) -> El {
        let k = kit(dark);
        let per_condition_logic = self.per_condition_logic();
        let mut row_el = row()
            .key("filter-logic-row")
            .w_full()
            .items_center()
            .gap(6.0);

        // The and/or toggle pair in a bordered pill.
        let mut toggles = row()
            .key("filter-logic")
            .radius(k.radius.sm)
            .border(1.0, k.c.border_base)
            .shrink();
        for (connector, key) in [
            (FilterConnector::And, "filter-logic-and"),
            (FilterConnector::Or, "filter-logic-or"),
        ] {
            let selected = self.draft_logic == connector;
            let mut toggle = row()
                .key(key)
                .role("button")
                .label(connector.label())
                .px(6.0)
                .py(2.0)
                .child(
                    text(connector.label())
                        .key("label")
                        .font(k.ty.small)
                        .color(if selected { k.c.text_accent } else { k.c.text_faint }),
                );
            if selected {
                toggle = toggle.fill(k.alpha(k.c.bg_accent, 0.15));
            }
            // The request-wide logic is meaningless while some condition
            // carries its own; React disables and titles it.
            if self.loading || per_condition_logic {
                toggle = toggle.disabled(true);
                if per_condition_logic {
                    toggle = toggle.help(t("filter.globalLogicDisabledHint"));
                }
            } else if let Some(on_logic_change) = &self.on_logic_change {
                let on_logic_change = Rc::clone(on_logic_change);
                toggle = toggle.on_press(move |s: &mut S| on_logic_change(s, connector));
            }
            toggles = toggles.child(toggle);
        }
        row_el = row_el.child(toggles);

        // The connector reset, only while some condition carries its own.
        if per_condition_logic {
            let label = t("filter.connectorReset")
                .replacen("{logic}", &self.draft_logic.label().to_uppercase(), 1);
            row_el = row_el.child(small_text_button(
                "filter-connector-reset",
                &label,
                self.loading,
                self.reset_connectors_callback(),
            ));
        }

        // The add button.
        row_el = row_el.child(small_ghost_button(
            "filter-add",
            Some(&ICON_PLUS),
            t("filter.add"),
            self.columns.is_empty() || self.loading,
            self.on_add.as_ref().map(|on_add| {
                let on_add = Rc::clone(on_add);
                let condition = self.add_empty_condition();
                Rc::new(move |s: &mut S| on_add(s, condition.clone())) as Rc<dyn Fn(&mut S)>
            }),
        ));

        // The right-aligned group (React's `ml-auto`): a grow filler
        // pushes it to the row's right edge.
        row_el = row_el.child(row().key("filter-actions-spacer").grow(1.0).min_w(0.0));
        let mut right = row().key("filter-actions").gap(6.0).shrink();
        if let Some(on_export) = &self.on_export_filtered {
            right = right.child(small_ghost_button(
                "filter-export-filtered",
                Some(&ICON_DOWNLOAD),
                t("filter.exportFiltered"),
                self.loading,
                Some(Rc::clone(on_export)),
            ));
        }
        if let Some(on_preview) = &self.on_preview_sql {
            right = right.child(small_ghost_button(
                "filter-preview-sql",
                Some(&ICON_TERMINAL),
                "SQL",
                self.loading,
                Some(Rc::clone(on_preview) as Rc<dyn Fn(&mut S)>),
            ));
        }
        right = right.child(small_ghost_button(
            "filter-collapse",
            None,
            t("filter.collapse"),
            self.loading,
            self.on_open_change.as_ref().map(|on_open_change| {
                let on_open_change = Rc::clone(on_open_change);
                Rc::new(move |s: &mut S| on_open_change(s, false)) as Rc<dyn Fn(&mut S)>
            }),
        ));
        // Apply is blocked by an incomplete condition; its title says
        // which column.
        let incomplete = self.incomplete();
        let apply_disabled = !self.dirty() || self.loading || incomplete.is_some();
        let on_apply = self.on_apply.clone();
        let apply_host = self.host.clone();
        let apply = on_apply.map(|on_apply| {
            Rc::new(move |s: &mut S| {
                if let Some(host) = &apply_host {
                    host(s).editing_index = None;
                }
                on_apply(s);
            }) as Rc<dyn Fn(&mut S)>
        });
        right = right.child(small_ghost_button(
            "filter-apply",
            None,
            t("filter.apply"),
            apply_disabled,
            apply,
        ))
        .help(match incomplete {
            Some(filter) => t("filter.errorIncomplete")
                .replacen("{column}", &filter.column, 1),
            None => String::new(),
        });
        let _ = &self.state;
        row_el = row_el.child(right);
        row_el
    }

    /// React's `resetConnectors`: drop every own connector — one
    /// `onChange` per rewritten condition.
    fn reset_connectors_callback(&self) -> Option<Rc<dyn Fn(&mut S)>> {
        let on_change = self.on_change.clone()?;
        let conditions: Vec<(usize, FilterCondition)> = self
            .draft_filters
            .iter()
            .enumerate()
            .filter_map(|(index, filter)| {
                if filter.connector.is_some() {
                    let mut rest = filter.clone();
                    rest.connector = None;
                    Some((index, rest))
                } else {
                    None
                }
            })
            .collect();
        Some(Rc::new(move |s: &mut S| {
            for (index, next) in &conditions {
                on_change(s, *index, next.clone());
            }
        }))
    }

    /// The chip list: `flex flex-wrap content-start gap-1.5` capped at
    /// two rows (76pt) with the error banners, the no-active empty
    /// state, and one row per condition — the connector toggle, the
    /// condition (chip; the expanded editor is T056's), and the
    /// incomplete marker.
    fn chip_list(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut list = row()
            .key("filter-list")
            .w_full()
            .flex_wrap()
            .items_start()
            .gap(6.0)
            .max_h(76.0);
        if let Some(banner) = error_banner("filter-error", self.apply_error, &k) {
            list = list.child(banner);
        }
        if let Some(incomplete) = self.incomplete() {
            let message =
                t("filter.errorIncomplete").replacen("{column}", &incomplete.column, 1);
            if let Some(banner) = error_banner("filter-incomplete-error", Some(&message), &k) {
                list = list.child(banner);
            }
        }
        if self.draft_filters.is_empty() {
            list = list.child(
                text(t("filter.noActive"))
                    .key("filter-no-active")
                    .px(4.0)
                    .py(4.0)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        } else {
            for (index, filter) in self.draft_filters.iter().enumerate() {
                let complete = is_complete_filter(filter);
                let connector = if index > 0 {
                    Some(filter.connector.unwrap_or(self.draft_logic))
                } else {
                    None
                };
                let mut condition_row = row()
                    .key(format!("filter-row-{index}"))
                    .items_center()
                    .gap(4.0)
                    .shrink();
                if let Some(connector) = connector {
                    let mut toggle = FilterConnectorToggle::new(
                        format!("filter-connector-{index}"),
                        index,
                        connector,
                    )
                    .explicit(filter.connector.is_some())
                    .differs_from_global(filter.connector.is_some() && connector != self.draft_logic)
                    .disabled(self.loading);
                    if let Some(on_connector) =
                        set_connector_callback(self.on_change.clone(), index, filter)
                    {
                        let on_and = Rc::clone(&on_connector);
                        let on_or = on_connector;
                        toggle = toggle
                            .on_and(move |s: &mut S| on_and(s, FilterConnector::And))
                            .on_or(move |s: &mut S| on_or(s, FilterConnector::Or));
                    }
                    condition_row = condition_row.child(toggle.build(dark));
                }
                let mut chip = FilterConditionChip::new(
                    format!("filter-chip-{index}"),
                    filter.clone(),
                )
                .disabled(self.loading);
                if let Some(on_edit) = chip_edit_callback(self.host.clone(), index) {
                    chip = chip.on_edit(move |s: &mut S| on_edit(s));
                }
                if let Some(on_remove) = remove_callback(self.on_remove.clone(), index) {
                    chip = chip.on_remove(move |s: &mut S| on_remove(s));
                }
                condition_row = condition_row.child(chip.build(dark));
                if !complete {
                    condition_row = condition_row.child(
                        text(t("filter.incomplete"))
                            .key("filter-incomplete")
                            .ml(4.0)
                            .px(4.0)
                            .radius(k.radius.xs)
                            .font(k.ty.small)
                            .color(k.c.warning.fg)
                            .fill(k.alpha(k.c.warning.fg, 0.15)),
                    );
                }
                list = list.child(condition_row);
            }
        }
        list
    }
}

// ---- the small shared builders -----------------------------------------------

/// React's `setConnector(index, next)`: one condition rewritten with
/// the connector. The condition itself is captured at build time — the
/// rewrite only sets the connector, keeping the rest of the shape.
fn set_connector_callback<S: 'static>(
    on_change: Option<Rc<dyn Fn(&mut S, usize, FilterCondition)>>,
    index: usize,
    condition: &FilterCondition,
) -> Option<Rc<dyn Fn(&mut S, FilterConnector)>> {
    let on_change = on_change?;
    let base = condition.clone();
    Some(Rc::new(move |s: &mut S, connector: FilterConnector| {
        let mut next = base.clone();
        next.connector = Some(connector);
        on_change(s, index, next);
    }))
}

/// The chip's edit action: record the row the editor would expand (T056).
fn chip_edit_callback<S: 'static>(
    host: Option<Rc<dyn Fn(&mut S) -> &mut FilterEditorState>>,
    index: usize,
) -> Option<Rc<dyn Fn(&mut S)>> {
    host.map(|host| {
        Rc::new(move |s: &mut S| {
            host(s).editing_index = Some(index);
        }) as Rc<dyn Fn(&mut S)>
    })
}

/// The chip's remove action.
fn remove_callback<S: 'static>(
    on_remove: Option<Rc<dyn Fn(&mut S, usize)>>,
    index: usize,
) -> Option<Rc<dyn Fn(&mut S)>> {
    on_remove.map(|on_remove| Rc::new(move |s: &mut S| on_remove(s, index)) as Rc<dyn Fn(&mut S)>)
}

/// A small ghost button (React's `h-6 ... text-xs` ghost rows).
fn small_ghost_button<S: 'static>(
    key: &'static str,
    glyph: Option<&'static rune::Icon>,
    label: &'static str,
    disabled: bool,
    on_press: Option<Rc<dyn Fn(&mut S)>>,
) -> El {
    let k = kit(false);
    let _ = &k;
    let mut button = row()
        .key(key)
        .role("button")
        .label(label)
        .h(24.0)
        .items_center()
        .gap(4.0)
        .px(if glyph.is_some() { 6.0 } else { 10.0 })
        .radius(k.radius.sm)
        .child(
            text(label)
                .key("label")
                .font(k.ty.text_xsmall)
                .color(k.c.text_base),
        );
    if let Some(glyph) = glyph {
        button = button.child(
            icon(glyph, 12.0)
                .key("icon")
                .color(k.c.text_muted)
                .a11y_hidden(),
        );
    }
    if disabled {
        return button.disabled(true).layer_opacity(0.5);
    }
    if let Some(on_press) = on_press {
        button = button.on_press(move |s: &mut S| on_press(s));
    }
    button
}

/// A small text-only button (React's bare `text-xs` links).
fn small_text_button<S: 'static>(
    key: &'static str,
    label: &str,
    disabled: bool,
    on_press: Option<Rc<dyn Fn(&mut S)>>,
) -> El {
    let k = kit(false);
    let mut button = row()
        .key(key)
        .role("button")
        .label(label.clone())
        .h(24.0)
        .items_center()
        .px(6.0)
        .child(
            text(label.to_string())
                .key("label")
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint),
        );
    if disabled {
        return button.disabled(true).layer_opacity(0.5);
    }
    if let Some(on_press) = on_press {
        button = button.on_press(move |s: &mut S| on_press(s));
    }
    button
}

/// The error banner (`filter-error` shape): the `Banner` base in its
/// plain inline variant.
fn error_banner(key: &'static str, message: Option<&str>, k: &rune_kit::Kit) -> Option<El> {
    let message = message?;
    let banner: Option<El> = Banner::new(key, message)
        .variant(BannerVariant::Plain)
        .build(k);
    banner
}

/// An `Option`-child helper that keeps the builder chain readable.
fn child_or_nothing(el: El, child: Option<El>) -> El {
    match child {
        Some(child) => el.child(child),
        None => el,
    }
}
