//! Port of
//! `packages/frontend/src/windows/settings/TableDataSettingsSection.tsx`
//! (T434) — the settings a table read consults and the settings a
//! grid looks like, in the "Data Browsing" section.
//!
//! They live there because that is where the user already expects
//! the page size — the row count, the order of a table and the look
//! of its grid are the same kind of decision.
//!
//! ## What maps where
//!
//! - The three `Select`s (row count mode / default sort / grid
//!   density) are the kit selects over their typed unions, keyed by
//!   the React `data-testid`s; the `is*` guards are the unions'
//!   `from_str` (an out-of-vocabulary answer never fires
//!   `updateSettings` — the `if (is…)` shape).
//! - The `auto`-only threshold input is the conditional row: the
//!   draft `TextEdit` + the per-edit commit (`Number(v)`, the
//!   finite and `>= 0` guards, `Math.round`) — the same per-edit
//!   clamp pattern the pool-size input carries; a refused edit
//!   never fires.
//! - The three `ToggleRow`s are `settings_ui`'s toggle rows, keyed
//!   by the React `testId`s.
//! - `updateSettings` is the `on_update_field` slot (the
//!   `TableDataField` payloads — the T390 `SettingsField` shape);
//!   the store's answer flows back through
//!   [`TableDataSettingsState::resync`].
//! - `<ValuePresentationSettingsSection />` (T474, not landed) is
//!   the delegated `value_presentation_section` slot — the fragment
//!   tail the host mounts.
//! - The select-open machine (`one open at a time`) is the state's
//!   `open_select` (the T390 `SettingsSelect` shape).
//!
//! ## Keys
//!
//! The selects and toggles keep the React `data-testid`s verbatim
//! (`settings-row-count-mode`, `settings-row-count-threshold`,
//! `settings-default-table-sort`, `settings-grid-density`,
//! `settings-grid-alternating-rows`, `settings-grid-row-numbers`,
//! `settings-grid-auto-hide-scrollbar`).
//!
//! Known deltas: the threshold input's `type="number"` spinners and
//! `step=1000` arrows are browser chrome the kit field does not
//! carry (the commit rule is the port's whole contract); the
//! `tabular-nums` face renders as the plain field face (the digits
//! are the layout's).

// No production consumer yet (the settings content shell T390
// mounts it through its `table_data_section` slot); this crate's
// tests are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, El, TextEdit};
use rune_kit::Kit;

use crate::i18n::t;
use crate::table_data_count_strategy::{
    DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD, DEFAULT_ROW_COUNT_MODE, RowCountMode,
};
use crate::ui::field::TextField;
use crate::ui::select::Select;
use crate::views::settings::settings_ui::{setting_row, toggle_row, ToggleRow as ToggleRowSpec};

/// `DefaultTableSort` (`types/settings.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DefaultTableSort {
    PrimaryKeyAsc,
    PrimaryKeyDesc,
    None,
}

impl DefaultTableSort {
    /// The wire spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PrimaryKeyAsc => "primaryKeyAsc",
            Self::PrimaryKeyDesc => "primaryKeyDesc",
            Self::None => "none",
        }
    }

    /// `isDefaultTableSort`.
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "primaryKeyAsc" => Some(Self::PrimaryKeyAsc),
            "primaryKeyDesc" => Some(Self::PrimaryKeyDesc),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    /// `DEFAULT_SORTS`, in declaration order.
    pub(crate) const ALL: [Self; 3] = [Self::PrimaryKeyAsc, Self::PrimaryKeyDesc, Self::None];

    /// The option label's i18n key.
    fn label_key(self) -> &'static str {
        match self {
            Self::PrimaryKeyAsc => "settings.defaultTableSort.primaryKeyAsc",
            Self::PrimaryKeyDesc => "settings.defaultTableSort.primaryKeyDesc",
            Self::None => "settings.defaultTableSort.none",
        }
    }
}

/// `GridDensity` (`types/settings.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GridDensity {
    Compact,
    Standard,
    Relaxed,
}

impl GridDensity {
    /// The wire spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Standard => "standard",
            Self::Relaxed => "relaxed",
        }
    }

    /// `isGridDensity`.
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "compact" => Some(Self::Compact),
            "standard" => Some(Self::Standard),
            "relaxed" => Some(Self::Relaxed),
            _ => None,
        }
    }

    /// `GRID_DENSITIES`, in declaration order.
    pub(crate) const ALL: [Self; 3] = [Self::Compact, Self::Standard, Self::Relaxed];

    /// The option label's i18n key.
    fn label_key(self) -> &'static str {
        match self {
            Self::Compact => "settings.gridDensity.compact",
            Self::Standard => "settings.gridDensity.standard",
            Self::Relaxed => "settings.gridDensity.relaxed",
        }
    }
}

/// The row-count modes, in `ROW_COUNT_MODES` order.
fn row_count_modes() -> [RowCountMode; 4] {
    [RowCountMode::Auto, RowCountMode::Exact, RowCountMode::Estimated, RowCountMode::None]
}

/// `isRowCountMode` (the count-strategy enum's own vocabulary).
pub(crate) fn is_row_count_mode(value: &str) -> bool {
    matches!(
        value,
        "auto" | "exact" | "estimated" | "none"
    )
}

fn row_count_mode_from_str(value: &str) -> Option<RowCountMode> {
    match value {
        "auto" => Some(RowCountMode::Auto),
        "exact" => Some(RowCountMode::Exact),
        "estimated" => Some(RowCountMode::Estimated),
        "none" => Some(RowCountMode::None),
        _ => None,
    }
}

fn row_count_mode_as_str(mode: RowCountMode) -> &'static str {
    match mode {
        RowCountMode::Auto => "auto",
        RowCountMode::Exact => "exact",
        RowCountMode::Estimated => "estimated",
        RowCountMode::None => "none",
    }
}

fn row_count_mode_label_key(mode: RowCountMode) -> &'static str {
    match mode {
        RowCountMode::Auto => "settings.rowCountMode.auto",
        RowCountMode::Exact => "settings.rowCountMode.exact",
        RowCountMode::Estimated => "settings.rowCountMode.estimated",
        RowCountMode::None => "settings.rowCountMode.none",
    }
}

/// The threshold input's commit rule: `Number(v)`, the finite and
/// `>= 0` guards, `Math.round` — `None` refuses the edit (nothing
/// fires).
pub(crate) fn parse_threshold(text: &str) -> Option<u64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let next: f64 = trimmed.parse().ok()?;
    if !next.is_finite() || next < 0.0 {
        return None;
    }
    Some(next.round() as u64)
}

/// Which of the section's selects is open (one at a time).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableDataSelect {
    RowCountMode,
    DefaultTableSort,
    GridDensity,
}

/// The section's settings mirror (the store fields it renders).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TableDataSettings {
    pub row_count_mode: RowCountMode,
    pub row_count_estimate_threshold: u64,
    pub default_table_sort: DefaultTableSort,
    pub grid_density: GridDensity,
    pub grid_alternating_rows: bool,
    pub grid_row_numbers: bool,
    pub grid_auto_hide_scrollbar: bool,
}

impl Default for TableDataSettings {
    /// `DEFAULT_ROW_COUNT_MODE` / `DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD`
    /// / `DEFAULT_TABLE_SORT` / `DEFAULT_GRID_DENSITY` /
    /// `DEFAULT_GRID_*` (the DB-17 block: what the grids did before
    /// the settings existed, so a fresh install sees exactly the
    /// grid it always saw).
    fn default() -> Self {
        Self {
            row_count_mode: DEFAULT_ROW_COUNT_MODE,
            row_count_estimate_threshold: DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD as u64,
            default_table_sort: DefaultTableSort::PrimaryKeyAsc,
            grid_density: GridDensity::Standard,
            grid_alternating_rows: true,
            grid_row_numbers: true,
            grid_auto_hide_scrollbar: false,
        }
    }
}

/// The `updateSettings` payloads (the T390 `SettingsField` shape).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableDataField {
    RowCountMode(RowCountMode),
    RowCountEstimateThreshold(u64),
    DefaultTableSort(DefaultTableSort),
    GridDensity(GridDensity),
    GridAlternatingRows(bool),
    GridRowNumbers(bool),
    GridAutoHideScrollbar(bool),
}

/// The section's state.
#[derive(Clone, PartialEq)]
pub(crate) struct TableDataSettingsState {
    /// The settings mirror (the store's slice).
    pub(crate) settings: TableDataSettings,
    /// The threshold input's draft (the controlled `<input type=
    /// "number">`'s text).
    pub(crate) threshold_edit: TextEdit,
    /// The open select (one at a time).
    pub(crate) open_select: Option<TableDataSelect>,
}

impl std::fmt::Debug for TableDataSettingsState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableDataSettingsState")
            .field("settings", &self.settings)
            .field("threshold_edit", &self.threshold_edit.text)
            .field("open_select", &self.open_select)
            .finish()
    }
}

impl TableDataSettingsState {
    pub(crate) fn new(settings: &TableDataSettings) -> Self {
        Self {
            settings: settings.clone(),
            threshold_edit: TextEdit::new(settings.row_count_estimate_threshold.to_string()),
            open_select: None,
        }
    }

    /// The store's next read (the settings prop's flow-back).
    pub(crate) fn resync(&mut self, settings: &TableDataSettings) {
        self.settings = settings.clone();
        self.threshold_edit = TextEdit::new(settings.row_count_estimate_threshold.to_string());
    }

    /// Whether the threshold row renders (the `auto`-only rule: the
    /// other modes do not read it, and a field that does nothing is
    /// worse than no field).
    pub(crate) fn threshold_visible(&self) -> bool {
        self.settings.row_count_mode == RowCountMode::Auto
    }

    /// The trigger's press: open, or close when already open.
    pub(crate) fn toggle_select(&mut self, which: TableDataSelect) {
        self.open_select = if self.open_select == Some(which) {
            None
        } else {
            Some(which)
        };
    }

    /// The listbox's dismissal.
    pub(crate) fn close_select(&mut self) {
        self.open_select = None;
    }
}

/// `TableDataSettingsSection()`: the section over the state, the
/// threshold draft's lens and the host's update slot.
pub(crate) struct TableDataSettingsSection<'a, S: 'static> {
    state: &'a TableDataSettingsState,
    dark: bool,
    section_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TableDataSettingsState>,
    threshold_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    /// `updateSettings` — the field writes.
    on_update_field: Option<Rc<dyn Fn(&mut S, TableDataField)>>,
    /// `<ValuePresentationSettingsSection />` (T474) — the delegated
    /// tail.
    value_presentation_section: Option<El>,
}

impl<'a, S: 'static> TableDataSettingsSection<'a, S> {
    pub(crate) fn new(
        state: &'a TableDataSettingsState,
        dark: bool,
        section_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TableDataSettingsState + 'static,
        threshold_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self {
            state,
            dark,
            section_edit: Rc::new(section_edit),
            threshold_edit: Rc::new(threshold_edit),
            on_update_field: None,
            value_presentation_section: None,
        }
    }

    /// The `updateSettings` slot.
    pub(crate) fn on_update_field(
        mut self,
        f: impl Fn(&mut S, TableDataField) + 'static,
    ) -> Self {
        self.on_update_field = Some(Rc::new(f));
        self
    }

    /// The delegated `<ValuePresentationSettingsSection />` (T474).
    pub(crate) fn value_presentation_section(mut self, section: Option<El>) -> Self {
        self.value_presentation_section = section;
        self
    }

    /// One typed select over its vocabulary (the labels, the values,
    /// the current label, the open state, the guards and the write).
    fn typed_select(
        &self,
        key: &'static str,
        which: TableDataSelect,
        entries: &[(String, String, String)], // (value, label, wire)
        current_value: &str,
    ) -> Select {
        let labels: Vec<String> = entries.iter().map(|(_, label, _)| label.clone()).collect();
        let values: Vec<String> = entries.iter().map(|(value, _, _)| value.clone()).collect();
        let current_label = entries
            .iter()
            .find(|(value, _, _)| value == current_value)
            .map(|(_, label, _)| label.clone())
            .unwrap_or_else(|| current_value.to_owned());
        let mut select = Select::new(key, &labels)
            .values(&values)
            .current(Some(current_label.as_str()))
            .width(160.0)
            .open(self.state.open_select == Some(which));
        // The kit contract: an enabled select opens only through its
        // on_toggle; the scrim closes through on_close.
        let section_edit = Rc::clone(&self.section_edit);
        select = select.on_toggle(move |s: &mut S| {
            section_edit(s).toggle_select(which);
        });
        let section_edit = Rc::clone(&self.section_edit);
        select = select.on_close(move |s: &mut S| {
            section_edit(s).close_select();
        });
        select
    }

    /// The section (the React fragment of rows).
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let mut body = col()
            .key("table-data-settings-section")
            .w_full()
            .gap(20.0);

        // The row count mode select.
        let entries: Vec<(String, String, String)> = row_count_modes()
            .iter()
            .map(|mode| {
                (
                    row_count_mode_as_str(*mode).to_owned(),
                    t(row_count_mode_label_key(*mode)).to_owned(),
                    row_count_mode_as_str(*mode).to_owned(),
                )
            })
            .collect();
        let mut select = self.typed_select(
            "settings-row-count-mode",
            TableDataSelect::RowCountMode,
            &entries,
            row_count_mode_as_str(state.settings.row_count_mode),
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            let section_edit = Rc::clone(&self.section_edit);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                // `if (isRowCountMode(value))` — the guard.
                if let Some(mode) = row_count_mode_from_str(value) {
                    f(s, TableDataField::RowCountMode(mode));
                }
                // The listbox closes on choose (the app's call —
                // the wrapper's own contract).
                section_edit(s).close_select();
            });
        }
        let select_el = select.build(k, layer);
        body = body.child(setting_row(
            k,
            layer,
            "settings-row-count-mode-row".to_owned(),
            t("settings.rowCountMode"),
            None,
            select_el,
        ));

        // The threshold input (the `auto` mode only).
        if state.threshold_visible() {
            let lens = Rc::clone(&self.threshold_edit);
            let mut field = TextField::new("settings-row-count-threshold", &state.threshold_edit)
                .height(36.0)
                .width(128.0) // w-32
                .on_edit::<S>(move |s: &mut S| lens(s));
            if let Some(f) = &self.on_update_field {
                let f = Rc::clone(f);
                let read = Rc::clone(&self.threshold_edit);
                field = field.on_change(move |s: &mut S, _tx| {
                    // Number(v); the finite and >= 0 guards; round.
                    let text = read(s).text.clone();
                    if let Some(threshold) = parse_threshold(&text) {
                        f(s, TableDataField::RowCountEstimateThreshold(threshold));
                    }
                });
            }
            body = body.child(setting_row(
                k,
                layer,
                "settings-row-count-threshold-row".to_owned(),
                t("settings.rowCountEstimateThreshold"),
                None,
                field.build(self.dark),
            ));
        }

        // The default sort select.
        let entries: Vec<(String, String, String)> = DefaultTableSort::ALL
            .iter()
            .map(|sort| (sort.as_str().to_owned(), t(sort.label_key()).to_owned(), sort.as_str().to_owned()))
            .collect();
        let mut select = self.typed_select(
            "settings-default-table-sort",
            TableDataSelect::DefaultTableSort,
            &entries,
            state.settings.default_table_sort.as_str(),
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            let section_edit = Rc::clone(&self.section_edit);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                if let Some(sort) = DefaultTableSort::from_str(value) {
                    f(s, TableDataField::DefaultTableSort(sort));
                }
                section_edit(s).close_select();
            });
        }
        let select_el = select.build(k, layer);
        body = body.child(setting_row(
            k,
            layer,
            "settings-default-table-sort-row".to_owned(),
            t("settings.defaultTableSort"),
            Some(t("settings.defaultTableSort.hint")),
            select_el,
        ));

        // The grid density select.
        let entries: Vec<(String, String, String)> = GridDensity::ALL
            .iter()
            .map(|density| {
                (
                    density.as_str().to_owned(),
                    t(density.label_key()).to_owned(),
                    density.as_str().to_owned(),
                )
            })
            .collect();
        let mut select = self.typed_select(
            "settings-grid-density",
            TableDataSelect::GridDensity,
            &entries,
            state.settings.grid_density.as_str(),
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            let section_edit = Rc::clone(&self.section_edit);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                if let Some(density) = GridDensity::from_str(value) {
                    f(s, TableDataField::GridDensity(density));
                }
                section_edit(s).close_select();
            });
        }
        let select_el = select.build(k, layer);
        body = body.child(setting_row(
            k,
            layer,
            "settings-grid-density-row".to_owned(),
            t("settings.gridDensity"),
            Some(t("settings.gridDensity.hint")),
            select_el,
        ));

        // The three grid toggles (DB-17's defaults: striped rows, the
        // `#` gutter, visible scrollbars).
        let toggles: [(&'static str, &'static str, &'static str, bool, fn(bool) -> TableDataField); 3] = [
            (
                "settings-grid-alternating-rows",
                "settings.gridAlternatingRows",
                "settings.gridAlternatingRows.hint",
                state.settings.grid_alternating_rows,
                TableDataField::GridAlternatingRows,
            ),
            (
                "settings-grid-row-numbers",
                "settings.gridRowNumbers",
                "settings.gridRowNumbers.hint",
                state.settings.grid_row_numbers,
                TableDataField::GridRowNumbers,
            ),
            (
                "settings-grid-auto-hide-scrollbar",
                "settings.gridAutoHideScrollbar",
                "settings.gridAutoHideScrollbar.hint",
                state.settings.grid_auto_hide_scrollbar,
                TableDataField::GridAutoHideScrollbar,
            ),
        ];
        for (key, label_key, hint_key, checked, field) in toggles {
            let handler: Rc<dyn Fn(&mut S, bool)> = match &self.on_update_field {
                Some(f) => {
                    let f = Rc::clone(f);
                    Rc::new(move |s: &mut S, v: bool| f(s, field(v)))
                }
                None => Rc::new(|_s: &mut S, _v: bool| {}),
            };
            body = body.child(toggle_row(
                k,
                layer,
                ToggleRowSpec {
                    key: key.to_owned(),
                    label: t(label_key),
                    hint: Some(t(hint_key)),
                    checked,
                    disabled: false,
                },
                handler,
            ));
        }

        // `<ValuePresentationSettingsSection />` (T474's slot).
        if let Some(section) = &self.value_presentation_section {
            body = body.child(section.clone());
        }

        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guards_recognize_only_their_vocabularies() {
        for value in ["auto", "exact", "estimated", "none"] {
            assert!(is_row_count_mode(value), "{value} is a row count mode");
        }
        assert!(!is_row_count_mode("yolo"));
        assert_eq!(DefaultTableSort::from_str("primaryKeyAsc"), Some(DefaultTableSort::PrimaryKeyAsc));
        assert_eq!(DefaultTableSort::from_str("primaryKeyDesc"), Some(DefaultTableSort::PrimaryKeyDesc));
        assert_eq!(DefaultTableSort::from_str("none"), Some(DefaultTableSort::None));
        assert_eq!(DefaultTableSort::from_str("inherit"), None, "the toolbar-only value is not a default");
        assert_eq!(GridDensity::from_str("compact"), Some(GridDensity::Compact));
        assert_eq!(GridDensity::from_str("relaxed"), Some(GridDensity::Relaxed));
        assert_eq!(GridDensity::from_str("cozy"), None);
    }

    #[test]
    fn the_threshold_commit_rounds_and_refuses() {
        assert_eq!(parse_threshold("1500"), Some(1500));
        assert_eq!(parse_threshold("1500.6"), Some(1501), "Math.round");
        assert_eq!(parse_threshold(" 42 "), Some(42), "trimmed");
        assert_eq!(parse_threshold("0"), Some(0));
        assert_eq!(parse_threshold("-5"), None, "negative refused");
        assert_eq!(parse_threshold("abc"), None, "NaN refused");
        assert_eq!(parse_threshold(""), None, "empty refused");
    }

    #[test]
    fn the_defaults_are_the_pre_settings_grid() {
        let settings = TableDataSettings::default();
        assert_eq!(settings.row_count_mode, RowCountMode::Auto);
        assert_eq!(settings.row_count_estimate_threshold, 200_000);
        assert_eq!(settings.default_table_sort, DefaultTableSort::PrimaryKeyAsc);
        assert_eq!(settings.grid_density, GridDensity::Standard);
        assert!(settings.grid_alternating_rows, "striped rows");
        assert!(settings.grid_row_numbers, "the # gutter");
        assert!(!settings.grid_auto_hide_scrollbar, "visible scrollbars");
    }

    #[test]
    fn the_threshold_row_is_the_auto_mode_only() {
        let mut state = TableDataSettingsState::new(&TableDataSettings::default());
        assert!(state.threshold_visible());
        state.resync(&TableDataSettings {
            row_count_mode: RowCountMode::Exact,
            ..TableDataSettings::default()
        });
        assert!(!state.threshold_visible(), "exact does not read it");
        state.resync(&TableDataSettings {
            row_count_mode: RowCountMode::None,
            ..TableDataSettings::default()
        });
        assert!(!state.threshold_visible());
        state.resync(&TableDataSettings::default());
        assert!(state.threshold_visible(), "auto reads it");
        // The draft re-seeds on resync (the controlled input): a
        // stale draft never survives the store's flow-back.
        state.threshold_edit = TextEdit::new("999".to_owned());
        state.resync(&TableDataSettings {
            row_count_estimate_threshold: 300_000,
            ..TableDataSettings::default()
        });
        assert_eq!(state.threshold_edit.text, "300000", "the draft mirrors the store");
    }

    #[test]
    fn the_selects_open_one_at_a_time() {
        let mut state = TableDataSettingsState::new(&TableDataSettings::default());
        state.toggle_select(TableDataSelect::RowCountMode);
        assert_eq!(state.open_select, Some(TableDataSelect::RowCountMode));
        // A second press closes.
        state.toggle_select(TableDataSelect::RowCountMode);
        assert_eq!(state.open_select, None);
        // Another select's press switches.
        state.toggle_select(TableDataSelect::GridDensity);
        state.toggle_select(TableDataSelect::DefaultTableSort);
        assert_eq!(state.open_select, Some(TableDataSelect::DefaultTableSort));
        // The dismissal closes.
        state.close_select();
        assert_eq!(state.open_select, None);
    }
}
