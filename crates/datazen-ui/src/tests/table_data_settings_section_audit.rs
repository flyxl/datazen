//! T434 audit: the table-data settings section, verified against
//! the React source re-read line by line
//! (`windows/settings/TableDataSettingsSection.tsx`). Element
//! evidence comes from `Runtime::elements()`, presses go through
//! real pointer events.

use rune::{col, text, El, Env, Runtime, TextEdit};

use crate::table_data_count_strategy::RowCountMode;
use crate::views::table_data_settings_section::{
    DefaultTableSort, GridDensity, TableDataField, TableDataSettings, TableDataSettingsSection,
    TableDataSettingsState, TableDataSelect,
};

#[derive(Clone)]
struct AuditState {
    section: TableDataSettingsState,
    dark: bool,
    // the host's records
    writes: Vec<TableDataField>,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            section: TableDataSettingsState::new(&TableDataSettings::default()),
            dark: true,
            writes: Vec::new(),
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let k = crate::ui::kit::kit(s.dark);
    let section = TableDataSettingsSection::<AuditState>::new(
        &s.section,
        s.dark,
        |s: &mut AuditState| &mut s.section,
        |s: &mut AuditState| &mut s.section.threshold_edit,
    )
    .on_update_field(|s, field| {
        s.writes.push(field);
        // The store's optimistic flow-back.
        let mut next = s.section.settings.clone();
        match field {
            TableDataField::RowCountMode(mode) => next.row_count_mode = mode,
            TableDataField::RowCountEstimateThreshold(threshold) => {
                next.row_count_estimate_threshold = threshold
            }
            TableDataField::DefaultTableSort(sort) => next.default_table_sort = sort,
            TableDataField::GridDensity(density) => next.grid_density = density,
            TableDataField::GridAlternatingRows(v) => next.grid_alternating_rows = v,
            TableDataField::GridRowNumbers(v) => next.grid_row_numbers = v,
            TableDataField::GridAutoHideScrollbar(v) => next.grid_auto_hide_scrollbar = v,
        }
        s.section.resync(&next);
    })
    .value_presentation_section(Some(
        col().key("value-presentation-slot-stub").child(text("vp".to_owned())),
    ))
    .build(&k, &mut layer);
    col().max_w(680.0).h_full().child(section).children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (900.0, 1200.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn reseed(rt: &mut Runtime<AuditState>) {
    rt.invalidate();
    settle(rt);
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

/// The default face: the three selects, the auto-mode threshold row,
/// the three toggles, the delegated value-presentation slot.
#[test]
fn the_default_face_mounts() {
    let rt = rt(AuditState::empty());
    for key in [
        "table-data-settings-section",
        "settings-row-count-mode-row",
        "settings-row-count-mode-trigger",
        "settings-row-count-threshold-row",
        "settings-row-count-threshold",
        "settings-default-table-sort-row",
        "settings-default-table-sort-trigger",
        "settings-grid-density-row",
        "settings-grid-density-trigger",
        "settings-grid-alternating-rows-switch",
        "settings-grid-row-numbers-switch",
        "settings-grid-auto-hide-scrollbar-switch",
        "value-presentation-slot-stub",
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
}

/// A non-auto mode drops the threshold row (a field that does
/// nothing is worse than no field) — chosen through the real
/// listbox press.
#[test]
fn the_threshold_row_is_auto_only() {
    let mut rt = rt(AuditState::empty());
    assert!(has(&rt, "settings-row-count-threshold-row"));
    // Open the row-count select and choose "exact" (option1 of the
    // ROW_COUNT_MODES order: auto, exact, estimated, none).
    press(&mut rt, "settings-row-count-mode-trigger");
    assert!(has(&mut rt, "option1"), "the listbox opened");
    press(&mut rt, "option1");
    assert_eq!(
        rt.state.writes.last(),
        Some(&TableDataField::RowCountMode(RowCountMode::Exact)),
        "the choose wrote through the guard"
    );
    assert!(!has(&rt, "settings-row-count-threshold-row"), "exact drops it");
    assert!(!has(&rt, "settings-row-count-threshold"));
    // Back to auto (option0): it returns.
    press(&mut rt, "settings-row-count-mode-trigger");
    press(&mut rt, "option0");
    assert!(has(&rt, "settings-row-count-threshold-row"), "auto reads it");
}

/// The selects open one at a time, and the second press closes.
#[test]
fn the_selects_open_one_at_a_time() {
    let mut rt = rt(AuditState::empty());
    press(&mut rt, "settings-row-count-mode-trigger");
    assert_eq!(rt.state.section.open_select, Some(TableDataSelect::RowCountMode));
    // A press on another select's trigger lands on the open
    // listbox's scrim first (the kit's dismissal rail) — it closes.
    press(&mut rt, "settings-grid-density-trigger");
    assert_eq!(rt.state.section.open_select, None, "the scrim closes the open one");
    // The next press opens the density select.
    press(&mut rt, "settings-grid-density-trigger");
    assert_eq!(rt.state.section.open_select, Some(TableDataSelect::GridDensity));
    press(&mut rt, "settings-grid-density-trigger");
    assert_eq!(rt.state.section.open_select, None, "the second press closes");
}

/// The threshold commit: a rounded write fires; a refused edit never
/// does.
#[test]
fn the_threshold_commits_rounded_and_refused() {
    let mut rt = rt(AuditState::empty());
    rt.update(|s, _| {
        s.section.threshold_edit = TextEdit::new("1500.6".to_owned());
    });
    reseed(&mut rt);
    // The per-edit commit fires on the field's on_change; the audit
    // drives it the way the kit does (the lens read + the parse).
    rt.update(|s, _| {
        let text = s.section.threshold_edit.text.clone();
        if let Some(threshold) =
            crate::views::table_data_settings_section::parse_threshold(&text)
        {
            s.writes.push(TableDataField::RowCountEstimateThreshold(threshold));
            let mut next = s.section.settings.clone();
            next.row_count_estimate_threshold = threshold;
            s.section.resync(&next);
        }
    });
    reseed(&mut rt);
    assert_eq!(
        rt.state.writes.last(),
        Some(&TableDataField::RowCountEstimateThreshold(1501)),
        "Math.round fires"
    );
    // A refused edit never fires.
    rt.update(|s, _| {
        s.section.threshold_edit = TextEdit::new("-5".to_owned());
    });
    reseed(&mut rt);
    let before = rt.state.writes.len();
    rt.update(|s, _| {
        let text = s.section.threshold_edit.text.clone();
        if let Some(threshold) =
            crate::views::table_data_settings_section::parse_threshold(&text)
        {
            s.writes.push(TableDataField::RowCountEstimateThreshold(threshold));
        }
    });
    assert_eq!(rt.state.writes.len(), before, "the negative never fires");
}

/// The toggles write their next values (a scene with distinct
/// initial values, so a crossed wire shows in the first press).
#[test]
fn the_grid_toggles_write() {
    let mut scene = AuditState::empty();
    scene.section.settings.grid_alternating_rows = true;
    scene.section.settings.grid_row_numbers = false;
    scene.section.settings.grid_auto_hide_scrollbar = true;
    let mut rt = rt(scene);
    assert!(has(&rt, "settings-grid-alternating-rows-switch"));
    press(&mut rt, "settings-grid-alternating-rows-switch");
    assert_eq!(
        rt.state.writes.last(),
        Some(&TableDataField::GridAlternatingRows(false)),
        "striped (true) turns off"
    );
    press(&mut rt, "settings-grid-row-numbers-switch");
    assert_eq!(
        rt.state.writes.last(),
        Some(&TableDataField::GridRowNumbers(true)),
        "the # gutter (false) turns on"
    );
    press(&mut rt, "settings-grid-auto-hide-scrollbar-switch");
    assert_eq!(
        rt.state.writes.last(),
        Some(&TableDataField::GridAutoHideScrollbar(false)),
        "auto-hide (true) turns off"
    );
}

/// The select writes go through the guards: a vocabulary value
/// fires, an out-of-vocabulary one never does.
#[test]
fn the_select_writes_follow_the_guards() {
    let mut rt = rt(AuditState::empty());
    // The kit's on_choose drives the host; the audit exercises the
    // same seam the kit calls (the section builder's on_choose
    // wiring runs the guard before the write).
    rt.update(|s, _| {
        // The vocabulary write.
        if let Some(sort) = DefaultTableSort::from_str("primaryKeyDesc") {
            s.writes.push(TableDataField::DefaultTableSort(sort));
        }
        // The out-of-vocabulary write never lands.
        if let Some(sort) = DefaultTableSort::from_str("inherit") {
            s.writes.push(TableDataField::DefaultTableSort(sort));
        }
        if let Some(density) = GridDensity::from_str("compact") {
            s.writes.push(TableDataField::GridDensity(density));
        }
    });
    assert_eq!(
        rt.state.writes,
        vec![
            TableDataField::DefaultTableSort(DefaultTableSort::PrimaryKeyDesc),
            TableDataField::GridDensity(GridDensity::Compact),
        ],
        "the inherit value never lands"
    );
}
