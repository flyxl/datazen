//! Real mounted Settings switch, backed by the stock disposable settings store.
use std::{fs, path::Path, sync::Arc};

use datazen_rune_host::{FileConnectionRepository, LiveAdapter};
use datazen_ui::{start_shared_app_state, view, AppState};
use rune::{ElementInfo, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::{json, Value};

const SWITCH: &str = "settings-grid-alternating-rows";

fn settled(rt: &mut Runtime<AppState>) {
    rt.settle_all();
    rt.frame(1.0 / 60.0);
}

fn press(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing live {key}"));
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    settled(rt);
}

fn find(rt: &Runtime<AppState>, key: &str) -> ElementInfo {
    rt.elements().into_iter().find(|el| !el.exiting && el.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("missing mounted {key}"))
}

fn open(data: &Path, dark: bool) -> Runtime<AppState> {
    let repo = FileConnectionRepository::open(&data.join("connections.json")).unwrap();
    let adapter = LiveAdapter::with_data_dir(Arc::new(repo), data);
    let mut rt = Runtime::new(start_shared_app_state(Some(Box::new(adapter))),
        view, (1440.0, 900.0), Env {system_dark: !dark, scale: 1.0});
    rt.set_full_keyboard_access(true);
    rt.frame(0.0);
    press(&mut rt, "rail-settings");
    press(&mut rt, "settings-nav-dataBrowsing");
    assert_eq!(rt.state.settings_section(), "dataBrowsing");
    assert_eq!(rt.state.dark_theme(), dark);
    rt
}

fn assert_switch(rt: &Runtime<AppState>, enabled: bool) {
    let item = find(rt, SWITCH);
    assert_eq!(item.role, "switch");
    assert_eq!(item.label.as_deref(), Some("Alternating row backgrounds"));
    let c = &datazen_ui::ui::kit::kit(rt.state.dark_theme()).c;
    assert_eq!(item.fill, if enabled { c.bg_accent } else { c.border_base });
    assert_eq!(item.rect[2..], [36.0, 20.0]);
    assert!(item.focusable);
}

#[test]
fn live_grid_striping_switch_uses_exact_key_and_rejects_failed_atomic_write() {
    for (dark, theme) in [(false, "light"), (true, "dark")] {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        let original = json!({
            "ui": {"language":"en", "themeMode":theme, "tablePageSize":50},
            "defaultTableSort":"primaryKeyAsc","rowCountMode":"exact","unrelated":{"pin":1}
        });
        let initial = serde_json::to_vec(&original).unwrap();
        fs::write(&path, &initial).unwrap();

        let mut rt = open(profile.path(), dark);
        assert_switch(&rt, true);
        assert_eq!(fs::read(&path).unwrap(), initial, "mount does not seed default");
        let order = ["settings-page-size-preset-trigger", "settings-row-count-mode-trigger",
            "settings-default-table-sort-trigger", SWITCH, "settings-max-rows-input",
            "settings-workflow-step-order-trigger", "settings-data-tools"];
        for near in order.windows(2) {
            assert!(find(&rt, near[0]).rect[1] < find(&rt, near[1]).rect[1]);
        }

        press(&mut rt, SWITCH);
        assert_switch(&rt, false);
        let mut expected = original.clone();
        expected["gridAlternatingRows"] = json!(false);
        let disabled = fs::read(&path).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&disabled).unwrap(), expected);

        // Atomic store failure cannot flip the visible/cached value.
        let collision = path.with_extension("json.tmp");
        fs::create_dir(&collision).unwrap();
        press(&mut rt, SWITCH);
        assert_switch(&rt, false);
        let prefix = format!("{}/", find(&rt, "settings-error").path);
        assert!(rt.elements().iter().any(|e| !e.exiting && e.path.starts_with(&prefix)
            && e.text.as_deref() == Some("Could not save alternating row backgrounds.")));
        assert_eq!(fs::read(&path).unwrap(), disabled);
        fs::remove_dir(collision).unwrap();

        drop(rt);
        let mut reopened = open(profile.path(), dark);
        assert_switch(&reopened, false);
        for _ in 0..100 {
            if find(&reopened, SWITCH).focused { break; }
            reopened.key(KeyEvent { key: Key::Tab, mods: Mods::default() });
            reopened.frame(1.0 / 60.0);
        }
        assert!(find(&reopened, SWITCH).focused);
        reopened.key(KeyEvent { key: Key::Text(" ".into()), mods: Mods::default() });
        settled(&mut reopened);
        assert_switch(&reopened, true);
        expected["gridAlternatingRows"] = json!(true);
        assert_eq!(serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(), expected);

        // A current malformed document must never be overwritten by the valid boot cache.
        let corrupt = b"{ incomplete";
        fs::write(&path, corrupt).unwrap();
        press(&mut reopened, SWITCH);
        assert_switch(&reopened, true);
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }
}

#[test]
fn live_row_number_setting_changes_the_actual_table_and_query_grids_without_changing_cells() {
    use datazen_ui::{data_grid::DataGrid, Section, TableView};
    use datazen_ui_api::{TabKind, TablePage};

    const ROW_NUMBERS: &str = "settings-grid-row-numbers";
    let profile = tempfile::tempdir().unwrap();
    let path = profile.path().join("settings.json");
    let original = json!({
        "ui": {"language":"en", "themeMode":"light"},
        "gridAlternatingRows": false, "unrelated":{"pin":1}
    });
    let initial = serde_json::to_vec(&original).unwrap();
    fs::write(&path, &initial).unwrap();

    let page = TablePage {
        table: "sample".into(),
        columns: vec!["id".into(), "name".into()],
        rows: vec![vec!["1".into(), "Ada".into()], vec!["2".into(), "Bob".into()]],
        page: 0,
        page_size: 25,
        total_rows: 2,
        sort: None,
        filter: None,
        primary_key: vec!["id".into()],
        read_only: None,
    };
    let mut rt = open(profile.path(), false);
    rt.on_key = Some(datazen_ui::app_key);
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    fn copy_by_key(rt: &mut Runtime<AppState>, clipboard: &rune::clipboard::MemoryClipboard) -> Option<String> {
        rt.key(KeyEvent { key: Key::Command('c'), mods: Mods::default() });
        settled(rt);
        clipboard.text().map(|text| text.to_string())
    }
    let toggle = find(&rt, ROW_NUMBERS);
    assert_eq!(toggle.role, "switch");
    assert_eq!(toggle.label.as_deref(), Some("Row numbers"));
    assert!(toggle.focusable);
    assert_eq!(toggle.fill, datazen_ui::ui::kit::kit(false).c.bg_accent);
    assert!(toggle.rect[1] > find(&rt, SWITCH).rect[1]);
    assert_eq!(fs::read(&path).unwrap(), initial, "mount must not write the default");

    rt.update(|state, _| {
        state.active = Section::Workspace;
        let _ = state.tabs.open(TabKind::Table, "sample").expect("sample table tab");
        state.table_view = TableView::Data;
        state.grid.load_page(&page);
    });
    settled(&mut rt);
    assert!(rt.find("head-rownum").is_some() && rt.find("rownum-0").is_some());
    let first_before = find(&rt, "cell-0-0").rect;
    let second_before = find(&rt, "cell-0-1").rect;
    press(&mut rt, "rownum-1");
    assert_eq!(rt.state.grid.selected_row(), Some(1));
    assert_eq!(rt.state.grid.copy_tsv(), "2\tBob");

    rt.update(|state, _| state.active = Section::Settings);
    settled(&mut rt);
    press(&mut rt, ROW_NUMBERS);
    assert_eq!(find(&rt, ROW_NUMBERS).fill, datazen_ui::ui::kit::kit(false).c.border_base);
    let mut expected = original.clone();
    expected["gridRowNumbers"] = json!(false);
    let disabled = fs::read(&path).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&disabled).unwrap(), expected);

    let collision = path.with_extension("json.tmp");
    fs::create_dir(&collision).unwrap();
    press(&mut rt, ROW_NUMBERS);
    assert_eq!(find(&rt, ROW_NUMBERS).fill, datazen_ui::ui::kit::kit(false).c.border_base);
    assert!(rt.find("settings-error").is_some());
    assert_eq!(fs::read(&path).unwrap(), disabled);
    fs::remove_dir(collision).unwrap();

    rt.update(|state, _| state.active = Section::Workspace);
    settled(&mut rt);
    assert!(rt.find("head-rownum").is_none() && rt.find("rownum-0").is_none());
    let first_after = find(&rt, "cell-0-0").rect;
    let second_after = find(&rt, "cell-0-1").rect;
    assert_eq!(first_before[0] - first_after[0], 40.0);
    assert_eq!(second_before[0] - second_after[0], 40.0);
    assert_eq!(first_before[2..], first_after[2..]);
    assert_eq!(rt.state.grid.selected_row(), Some(1), "appearance does not clear selection");
    assert_eq!(rt.state.grid.copy_tsv(), "2\tBob");

    // Both row-background and ordinary cell clicks go through actual pointer
    // hit testing with the gutter off. A cell drag remains a cell range.
    press(&mut rt, "grid-row-background-0");
    assert_eq!(rt.state.grid.selected_row(), Some(0));
    assert_eq!(rt.state.grid.copy_tsv(), "1\tAda");
    press(&mut rt, "cell-0-0");
    assert_eq!(rt.state.grid.selected_row(), None);
    assert_eq!(rt.state.grid.focus(), (0, 0));
    let (x, y) = rt.find("cell-0-0").unwrap();
    let (end_x, end_y) = rt.find("cell-1-1").unwrap();
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_move(end_x, end_y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(end_x, end_y);
    settled(&mut rt);
    assert_eq!(rt.state.grid.rect(), (0, 1, 0, 1));
    assert_eq!(rt.state.grid.copy_tsv(), "1\tAda\n2\tBob");
    assert_eq!(copy_by_key(&mut rt, &clipboard).as_deref(), Some("1\tAda\n2\tBob"));

    let mut wide_page = page.clone();
    wide_page.columns = (0..20).map(|index| format!("column{index}")).collect();
    wide_page.rows = (0..2)
        .map(|row| (0..20).map(|col| format!("{row}:{col}")).collect())
        .collect();
    rt.update(|state, _| state.grid.load_page(&wide_page));
    settled(&mut rt);
    assert!(rt.find("grid-row-background-0").is_none(), "no fake tail on a full-width row");
    let scroll_key = "data-grid-scroll";
    let left = find(&rt, scroll_key).rect[0];
    rt.update(|_, tx| tx.scroll_to_x(scroll_key, 300.0));
    settled(&mut rt);
    assert_eq!(find(&rt, "head-0").rect[0], left, "frozen header has no hidden gutter");
    assert_eq!(find(&rt, "cell-0-0").rect[0], left, "frozen cell has no hidden gutter");

    rt.update(|state, _| {
        let _ = state.tabs.open(TabKind::Query, "Result").expect("result tab");
        // Exercise the same stock Cmd+C after the live query grid takes
        // the pinned pane's available height.
        state.set_layout_size(datazen_ui::constants::QUERY_EDITOR_HEIGHT_KEY, 240).unwrap();
        let mut results = DataGrid::new();
        results.load_page(&page);
        state.result_grid = Some(results);
    });
    settled(&mut rt);
    assert!(rt.find("result-head-rownum").is_none() && rt.find("result-rownum-0").is_none());
    press(&mut rt, "result-cell-1-1");
    assert_eq!(rt.state.result_grid.as_ref().unwrap().copy_tsv(), "Bob");
    assert_eq!(copy_by_key(&mut rt, &clipboard).as_deref(), Some("Bob"));

    drop(rt);
    let reopened = open(profile.path(), false);
    assert_eq!(find(&reopened, ROW_NUMBERS).fill, datazen_ui::ui::kit::kit(false).c.border_base);
}

#[test]
fn live_grid_density_selector_keeps_durable_choice_on_failed_save() {
    use datazen_ui_api::GridDensity;
    const DENSITY: &str = "settings-grid-density-trigger";
    const OPTIONS: &str = "settings-grid-density-content";

    fn choose(rt: &mut Runtime<AppState>, label: &str) {
        press(rt, DENSITY);
        let prefix = format!("{}/", find(rt, OPTIONS).path);
        let option = rt.elements().into_iter().find(|item| {
            !item.exiting && item.role == "option" && item.path.starts_with(&prefix)
                && item.label.as_deref() == Some(label)
        }).unwrap_or_else(|| panic!("missing density option {label}"));
        press(rt, &option.path);
        assert!(rt.find(OPTIONS).is_none());
    }

    let profile = tempfile::tempdir().unwrap();
    let path = profile.path().join("settings.json");
    let initial = json!({"ui": {"language":"en", "themeMode":"light"},
                         "gridDensity":"legacy-invalid","unrelated":[1,2]});
    let original = serde_json::to_vec(&initial).unwrap();
    fs::write(&path, &original).unwrap();
    let mut rt = open(profile.path(), false);
    assert_eq!(rt.state.desktop.read_native_grid_density(), Ok(GridDensity::Standard));
    assert_eq!(find(&rt, DENSITY).role, "combobox");
    assert_eq!(fs::read(&path).unwrap(), original);

    choose(&mut rt, "Compact");
    assert_eq!(rt.state.desktop.read_native_grid_density(), Ok(GridDensity::Compact));
    let mut expected = initial.clone();
    expected["gridDensity"] = json!("compact");
    let saved = fs::read(&path).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&saved).unwrap(), expected);

    let collision = path.with_extension("json.tmp");
    fs::create_dir(&collision).unwrap();
    choose(&mut rt, "Relaxed");
    assert!(rt.find("settings-error").is_some());
    assert_eq!(rt.state.desktop.read_native_grid_density(), Ok(GridDensity::Compact));
    assert_eq!(fs::read(&path).unwrap(), saved);
    fs::remove_dir(collision).unwrap();

    drop(rt);
    let mut reopened = open(profile.path(), false);
    assert_eq!(reopened.state.desktop.read_native_grid_density(), Ok(GridDensity::Compact));
    choose(&mut reopened, "Relaxed");
    assert_eq!(reopened.state.desktop.read_native_grid_density(), Ok(GridDensity::Relaxed));
    expected["gridDensity"] = json!("relaxed");
    assert_eq!(serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(), expected);

    // Exercise real keyboard copy from a cell actually laid out at relaxed
    // query-result density (the separate Native journey loads real SQLite).
    reopened.on_key = Some(datazen_ui::app_key);
    let clipboard = rune::clipboard::memory();
    reopened.use_clipboard(&clipboard);
    reopened.update(|state, _| {
        state.active = datazen_ui::Section::Workspace;
        let _ = state.tabs.open(datazen_ui_api::TabKind::Query, "Relaxed result").expect("query tab");
        state.load_result_page(vec!["label".into()], vec![vec!["selected result".into()]]);
    });
    settled(&mut reopened);
    assert_eq!(find(&reopened, "result-cell-0-0").rect[3], 40.0);
    press(&mut reopened, "result-cell-0-0");
    reopened.key(KeyEvent { key: Key::Command('c'), mods: Mods::default() });
    settled(&mut reopened);
    assert_eq!(clipboard.text().as_deref().map(String::as_str), Some("selected result"));
}
