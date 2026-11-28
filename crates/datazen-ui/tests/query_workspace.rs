use datazen_ui::test_support::*;
use datazen_ui::constants::{QUERY_EDITOR_HEIGHT_DEFAULT, QUERY_EDITOR_HEIGHT_KEY};

#[test]
fn a3_query_editor_height_split_persists_via_layout_setting() {
    let mut state = AppState::default();
    assert_eq!(state.layout_size(QUERY_EDITOR_HEIGHT_KEY), None);
    state
        .set_layout_size(QUERY_EDITOR_HEIGHT_KEY, 360)
        .expect("layout setting");
    assert_eq!(state.layout_size(QUERY_EDITOR_HEIGHT_KEY), Some(360));

    let settings = state.ui_settings();
    let hit = settings.iter().any(|s| {
        matches!(
            s,
            Setting::Layout { key, value } if key == QUERY_EDITOR_HEIGHT_KEY && *value == 360
        )
    });
    assert!(hit, "query editor height is stored as Setting::Layout");
    assert_eq!(QUERY_EDITOR_HEIGHT_DEFAULT, 320);
}

#[test]
fn a3_multi_result_tabs_switch_and_pin_across_runs() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_query_tab("Query").unwrap();
    runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");

    runtime.state.run_query();
    assert_eq!(runtime.state.query_result_tabs.len(), 1);
    let first = runtime.state.query_result_tabs[0].id;
    assert_eq!(runtime.state.selected_query_result, Some(first));
    assert!(runtime.state.result_grid.is_some(), "first run produced a result");
    assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 0), "1");

    // Second run creates another result tab and selects it.
    runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
    runtime.state.run_query();
    assert_eq!(runtime.state.query_result_tabs.len(), 2);
    let second = runtime.state.query_result_tabs[1].id;
    assert_ne!(first, second);
    assert_eq!(runtime.state.selected_query_result, Some(second));

    // Switch back to the first result tab and keep its grid content.
    assert!(runtime.state.select_query_result_tab(first));
    assert_eq!(runtime.state.selected_query_result, Some(first));
    assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 0), "1");

    // Pin the first tab; it remains present after a new run.
    runtime.state.toggle_pin_query_result_tab(first);
    assert!(runtime
        .state
        .query_result_tabs
        .iter()
        .any(|t| t.id == first && t.pinned));

    runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
    runtime.state.run_query();
    assert!(runtime
        .state
        .query_result_tabs
        .iter()
        .any(|t| t.id == first && t.pinned), "pinned tab persists across runs");
    assert!(runtime.state.query_result_tabs.len() >= 3);
}

#[test]
fn a3_query_workspace_renders_toolbar_sidebar_and_result_tabs() {
    use datazen_ui_api::WorkspaceApi;

    let mut runtime = runtime();
    runtime.state.workspace.connect("local").unwrap();
    runtime.state.open_query_tab("Query").unwrap();
    runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
    runtime.state.run_query();
    runtime.settle_all();
    runtime.frame(0.0);

    assert!(runtime.find("query-toolbar").is_some(), "toolbar rendered");
    assert!(runtime.find("sql-run").is_some());
    assert!(runtime.find("sql-format").is_some());
    assert!(runtime.find("sql-explain").is_some());
    assert!(runtime.find("query-sidebar").is_none(), "real query library starts collapsed");
    tap_query(&mut runtime, "query-library-favorites-toggle");
    assert!(runtime.find("query-sidebar").is_some(), "genuine Favorites disclosure opens the library");
    assert!(runtime.find("query-result-tabs").is_some(), "result tabs bar rendered");
    assert!(runtime.find("query-result-tab-1").is_some(), "first result tab rendered");
}
fn query_rect(runtime: &Runtime<AppState>, key: &str) -> [f32; 4] {
    runtime.presented(key).unwrap_or_else(|| panic!("visible {key}")).0
}

fn tap_query(runtime: &mut Runtime<AppState>, key: &str) {
    let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("visible {key}"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.settle_all();
    runtime.frame(0.1);
}

#[test]
fn query_toolbar_is_above_flex_idle_editor_and_run_preserves_the_selected_statement() {
    let mut rt = runtime();
    rt.poll = Some(datazen_ui::app_poll);
    rt.state.workspace.connect("local").unwrap();
    rt.state.open_query_tab("Query").unwrap();
    rt.state.set_layout_size(QUERY_EDITOR_HEIGHT_KEY, 350).unwrap();
    let sql = "SELECT 99;\nSELECT 1";
    rt.state.sql = sql_editor::SqlEditor::from_doc(sql);
    rt.state.sql.set_main_selection(11, sql.len());
    rt.frame(0.1);

    let toolbar = query_rect(&rt, "query-toolbar");
    let editor = query_rect(&rt, "query-editor-pane");
    let viewport = query_rect(&rt, "sql-editor-scroll");
    let gutter_track = query_rect(&rt, "sql-editor-gutter-track");
    let footer = query_rect(&rt, "query-results");
    let main = query_rect(&rt, "query-main");
    assert_eq!(toolbar[3], 36.0);
    assert_eq!(toolbar[1] + toolbar[3], editor[1], "no gap under the top toolbar");
    assert_eq!(editor[1] + editor[3], footer[1]);
    assert_eq!(footer[3], 30.0, "compact idle footer, not an empty result card");
    assert_eq!(footer[1] + footer[3], main[1] + main[3]);
    assert!(editor[3] > 450.0, "idle canvas uses available height: {editor:?}");
    assert_eq!([gutter_track[0], gutter_track[1], gutter_track[3]], [viewport[0], viewport[1], viewport[3]]);
    assert!(rt.find("query-idle-hint").is_some());
    assert!(rt.find("query-result-tabs").is_none());
    assert!(rt.find("splitter:query-editor-height").is_none());
    assert!(rt.find("sql-find-open").is_some());
    assert!(rt.find("sql-find-input").is_none(), "idle Query does not reserve a find bar");
    assert!(rt.find("sql-replace-input").is_none());
    assert_eq!(viewport[1], editor[1], "no gap under the editor pane edge");

    tap_query(&mut rt, "sql-run");
    let grid = rt.state.result_grid.as_ref().expect("toolbar executes the selected statement");
    assert_eq!(grid.cell(0, 0), "1");
    assert_eq!(rt.state.sql.doc(), sql);
    assert!(rt.find("query-idle-hint").is_none());
    assert_eq!(query_rect(&rt, "splitter:query-editor-height")[3], 6.0);
    let after_toolbar = query_rect(&rt, "query-toolbar");
    let after_editor = query_rect(&rt, "query-editor-pane");
    assert_eq!(after_toolbar[1] + after_toolbar[3], after_editor[1]);
    assert!(after_editor[3] <= 350.0, "real results reuse the persisted split");
    assert!(rt.find("result-cell-0-0").is_some());

    tap_query(&mut rt, "query-library-history-toggle");
    assert_eq!(rt.state.query_sidebar_panel, types::QuerySidebarPanel::History);
    assert!(rt.find("query-history-panel").is_some());
    tap_query(&mut rt, "query-library-history-toggle");
    assert!(rt.find("query-sidebar-pane").is_none());
    tap_query(&mut rt, "query-result-close-1");
    assert!(rt.state.result_grid.is_none());
    assert_eq!(query_rect(&rt, "query-editor-pane"), editor);
    assert_eq!(rt.state.layout_size(QUERY_EDITOR_HEIGHT_KEY), Some(350));
    assert!(rt.find("query-idle-hint").is_some());
}

#[test]
fn narrow_query_toolbar_can_scroll_to_its_last_action() {
    let mut rt = runtime();
    rt.set_viewport((900.0, 900.0), 1.0);
    rt.state.workspace.connect("local").unwrap();
    rt.state.open_query_tab("Query").unwrap();
    rt.frame(0.1);
    // The open native library narrows the Query main pane; the scroll
    // surface still exposes all original controls while disclosure stays pinned.
    tap_query(&mut rt, "query-library-favorites-toggle");

    let toolbar = query_rect(&rt, "query-toolbar-scroll");
    let last_before = query_rect(&rt, "sql-tx-rollback");
    let edge = toolbar[0] + toolbar[2];
    assert!(last_before[0] + last_before[2] > edge, "fixture actually overflows");
    let (x, y) = rt.find("query-toolbar-scroll").unwrap();
    rt.wheel_xy(x, y, -1000.0, 0.0);
    rt.frame(0.2);
    let last_after = query_rect(&rt, "sql-tx-rollback");
    assert!(last_after[0] >= toolbar[0] && last_after[0] + last_after[2] <= edge,
        "last action now accessible in toolbar: {last_after:?} vs {toolbar:?}");
    tap_query(&mut rt, "sql-tx-rollback");
    assert!(rt.state.query_notice.is_some(), "the scrolled action still hits its own callback");
}

#[test]
fn real_query_library_disclosures_switch_and_collapse_without_disturbing_the_editor() {
    use datazen_ui_api::WorkspaceApi;

    let mut rt = runtime();
    rt.state.workspace.connect("local").unwrap();
    let query = rt.state.open_query_tab("Existing query").unwrap();
    rt.state.workspace.save_snippet("Supplier", "SELECT 42;").unwrap();
    rt.state.sql = sql_editor::SqlEditor::from_doc("");
    rt.frame(0.1);
    let full = query_rect(&rt, "query-toolbar");
    let parent = query_rect(&rt, "query-tab");
    let actions = [
        ("query-library-favorites-toggle", "query-favorites-panel"),
        ("query-library-history-toggle", "query-history-panel"),
        ("query-library-snippets-toggle", "query-snippets-panel"),
    ];
    let assert_closed = |rt: &Runtime<AppState>| {
        assert!(rt.find("query-sidebar-pane").is_none());
        assert!(rt.find("splitter:query-sidebar-width").is_none());
        let toolbar = query_rect(rt, "query-toolbar");
        assert_eq!([toolbar[0], toolbar[2]], [parent[0], parent[2]]);
    };
    assert_closed(&rt);
    for (toggle, panel) in actions {
        let action = rt.elements().into_iter().find(|e| !e.exiting
            && e.key.as_deref() == Some(toggle)).expect("physical toolbar control");
        assert_eq!(action.role, "button");
        assert!(action.focusable);
        tap_query(&mut rt, toggle);
        assert!(rt.find(panel).is_some());
        assert!(rt.find("splitter:query-sidebar-width").is_some());
        let pane = query_rect(&rt, "query-sidebar-pane");
        let opened = query_rect(&rt, "query-toolbar");
        let separator = query_rect(&rt, "splitter:query-sidebar-width");
        assert_eq!(pane[2], QUERY_SIDEBAR_WIDTH_DEFAULT as f32);
        assert_eq!(separator[2], 1.0);
        assert_eq!([separator[1], separator[3]], [parent[1], parent[3]]);
        assert_eq!([pane[1], pane[3]], [parent[1], parent[3]]);
        assert_eq!(opened[2] + separator[2] + pane[2], full[2]);
        if panel == "query-snippets-panel" {
            assert_eq!(pane[0], full[0], "working Snippets panel still belongs on the left");
            assert_eq!(pane[0] + pane[2], separator[0]);
            assert_eq!(separator[0] + separator[2], opened[0]);
        } else {
            assert_eq!(opened[0], full[0], "drawer doesn't indent the actual Query canvas");
            assert_eq!(opened[0] + opened[2], separator[0]);
            assert_eq!(separator[0] + separator[2], pane[0]);
            assert_eq!(pane[0] + pane[2], full[0] + full[2]);
        }
        for (_, other) in actions.into_iter().filter(|(_, other)| *other != panel) {
            assert!(rt.find(other).is_none(), "only the selected live panel mounts");
        }
        assert_eq!(rt.state.tabs.selected().map(|tab| tab.id), Some(query));
    }
    tap_query(&mut rt, "query-snippet-0");
    assert_eq!(rt.state.sql.doc(), "SELECT 42;");
    assert!(rt.state.result_grid.is_none(), "insertion does not execute");
    tap_query(&mut rt, "query-library-close");
    assert_closed(&rt);
    assert_eq!(rt.state.sql.doc(), "SELECT 42;");
    assert_eq!(rt.state.tabs.selected().map(|tab| tab.id), Some(query));
    assert_eq!(rt.state.layout_size(QUERY_SIDEBAR_WIDTH_KEY), None, "disclosure is session-only");
}

#[test]
fn right_query_drawer_uses_the_existing_layout_key_and_physical_resize_direction() {
    use rune::{Key, KeyEvent, Mods};

    const HANDLE: &str = "splitter:query-sidebar-width";
    const PANE: &str = "query-sidebar-pane";
    const HISTORY: &str = "query-library-history-toggle";

    let mut rt = runtime();
    rt.set_full_keyboard_access(true);
    rt.state.workspace.connect("local").unwrap();
    let query = rt.state.open_query_tab("Existing width").unwrap();
    rt.state.sql = sql_editor::SqlEditor::from_doc("SELECT 42;");
    rt.state.sql.set_main_caret(4);
    rt.state.set_layout_size(QUERY_SIDEBAR_WIDTH_KEY, 288).unwrap();
    rt.frame(0.0);
    let full = query_rect(&rt, "query-toolbar");
    let right = full[0] + full[2];

    let point = |rt: &Runtime<AppState>| {
        let [x, y, w, h] = query_rect(rt, HANDLE);
        (x + w / 2.0, y + h / 2.0)
    };
    let assert_drawer = |rt: &Runtime<AppState>, width: f32| {
        let pane = query_rect(rt, PANE);
        let toolbar = query_rect(rt, "query-toolbar");
        let separator = query_rect(rt, HANDLE);
        assert_eq!([toolbar[0], toolbar[0] + toolbar[2]], [full[0], separator[0]]);
        assert_eq!([separator[2], separator[0] + separator[2]], [1.0, pane[0]]);
        assert_eq!([pane[2], pane[0] + pane[2]], [width, right]);
    };
    let settle = |rt: &mut Runtime<AppState>| {
        rt.settle_all();
        rt.frame(1.0 / 60.0);
    };
    let press = |rt: &mut Runtime<AppState>, key| {
        rt.key(KeyEvent { key, mods: Mods::default() });
        rt.frame(1.0 / 60.0);
    };
    let focused = |rt: &Runtime<AppState>, key: &str| {
        rt.elements().iter().any(|e| !e.exiting && e.key.as_deref() == Some(key) && e.focused)
    };

    tap_query(&mut rt, HISTORY);
    assert_drawer(&rt, 288.0);
    let (x, y) = point(&rt);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_move(x - 32.0, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.splitter_preview, Some((QUERY_SIDEBAR_WIDTH_KEY.into(), 320)));
    assert_drawer(&rt, 320.0);
    assert_eq!(rt.state.layout_size(QUERY_SIDEBAR_WIDTH_KEY), Some(288));
    rt.pointer_up(x - 32.0, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.splitter_preview, None);
    assert_eq!(rt.state.layout_size(QUERY_SIDEBAR_WIDTH_KEY), Some(320));
    settle(&mut rt);
    assert_drawer(&rt, 320.0);

    let (x, y) = point(&rt);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_move(x + 32.0, y);
    rt.frame(1.0 / 60.0);
    assert_drawer(&rt, 288.0);
    press(&mut rt, Key::Escape);
    rt.pointer_up(x + 32.0, y);
    rt.frame(1.0 / 60.0);
    settle(&mut rt);
    assert_eq!(rt.state.splitter_preview, None);
    assert_eq!(rt.state.layout_size(QUERY_SIDEBAR_WIDTH_KEY), Some(320));
    assert_drawer(&rt, 320.0);

    for _ in 0..120 {
        if focused(&rt, HANDLE) {
            break;
        }
        press(&mut rt, Key::Tab);
    }
    assert!(focused(&rt, HANDLE), "right separator is reachable by keyboard");
    press(&mut rt, Key::Right);
    settle(&mut rt);
    assert_eq!(rt.state.layout_size(QUERY_SIDEBAR_WIDTH_KEY), Some(319));
    assert_drawer(&rt, 319.0);

    tap_query(&mut rt, "query-library-close");
    assert!(focused(&rt, HISTORY), "closing the drawer returns focus to its real opener");
    assert!(rt.find(PANE).is_none());
    assert!(rt.find(HANDLE).is_none());
    assert_eq!(query_rect(&rt, "query-toolbar"), full);
    tap_query(&mut rt, "query-library-favorites-toggle");
    assert_drawer(&rt, 319.0);
    assert_eq!(rt.state.tabs.selected().map(|tab| tab.id), Some(query));
    assert_eq!(rt.state.sql.doc(), "SELECT 42;");
    assert_eq!(rt.state.sql.main_head(), 4);
    assert!(rt.state.query_result_tabs.is_empty(), "resizing or opening never executes");
}

#[test]
fn mounted_editor_font_slider_resizes_sql_glyphs_caret_and_gutter() {
    use datazen_ui::types::Section;

    let mut rt = runtime();
    rt.state.open_query_tab("Font proof").unwrap();
    rt.state.sql = sql_editor::SqlEditor::from_doc("SELECT '字号';\nSELECT 2;");
    rt.state.sql.set_main_caret(3);
    rt.state.sql.focus();
    rt.frame(0.1);
    assert_eq!(rt.state.editor_font_size(), 13);
    let before_line = query_rect(&rt, "sql-line-0");
    let before_gutter = query_rect(&rt, "sql-gutter-line-0");
    let before_number = query_rect(&rt, "sql-gutter-number-0");
    let before_caret = query_rect(&rt, "sql-caret");
    assert_eq!(before_line[1], before_gutter[1]);
    assert_eq!(before_line[3], before_gutter[3]);

    // Enter the real route, which invalidates Rune's previous query frame.
    tap_query(&mut rt, "rail-settings");
    assert_eq!(rt.state.active, Section::Settings);
    tap_query(&mut rt, "settings-nav-editor");
    assert_eq!(rt.state.settings_section(), "editor");
    let [x, y, width, height] = query_rect(&rt, "settings-font-size");
    assert!(width > 0.0);
    assert_eq!(height, 36.0);
    let thumb_at_13 = query_rect(&rt, "settings-font-size-thumb");
    assert_eq!(thumb_at_13[2], 16.0);
    let center = |r: [f32; 4]| r[0] + r[2] * 0.5;
    let center_at_13 = x + width * (3.0 / 14.0);
    assert!(
        (center(thumb_at_13) - center_at_13).abs() <= 1.0,
        "default thumb={thumb_at_13:?}, expected x={center_at_13:.2}, track={:?}",
        [x, y, width, height],
    );
    // This slice does not replace the pre-existing live snippets workflow.
    assert!(rt.find("settings-snippets").is_some());
    for key in ["snippet-name-input", "snippet-sql-input", "snippet-save", "snippet-update"] {
        assert!(rt.find(key).is_some(), "existing snippet control disappeared: {key}");
    }
    assert!(rt.find("snippet-edit-dialog").is_none());
    let pos = x + width * (10.0 / 14.0);
    rt.pointer_down(pos, y + height * 0.5);
    rt.pointer_up(pos, y + height * 0.5);
    rt.frame(0.1);
    assert_eq!(rt.state.editor_font_size(), 20);
    // The preference commits as soon as the drag lands, but presented bounds
    // still follow Rune's layout spring. Compare the real thumb at rest.
    let moving_thumb = query_rect(&rt, "settings-font-size-thumb");
    rt.settle_all();
    rt.frame(0.0);
    let track_at_20 = query_rect(&rt, "settings-font-size");
    let thumb_at_20 = query_rect(&rt, "settings-font-size-thumb");
    let center_at_20 = track_at_20[0] + track_at_20[2] * (10.0 / 14.0);
    eprintln!(
        "font slider 20px: click x={pos:.2}; 0.1s thumb={moving_thumb:?}; settled track={track_at_20:?}, thumb={thumb_at_20:?}, expected x={center_at_20:.2}",
    );
    assert_eq!(thumb_at_20[2], 16.0);
    assert!(
        (center(thumb_at_20) - center_at_20).abs() <= 1.0,
        "misaligned settled thumb: actual x={:.2}, expected x={center_at_20:.2}; track={track_at_20:?}, thumb={thumb_at_20:?}",
        center(thumb_at_20),
    );

    tap_query(&mut rt, "settings-back");
    assert_eq!(rt.state.active, Section::Workspace);
    assert_eq!(rt.state.sql.main_head(), 3, "the route preserved the caret");
    // Re-focus the same model anchor (returning from Settings blurs the
    // editor). Do not reposition the caret by clicking elsewhere in the doc.
    rt.update(|state, _| state.sql.focus());
    rt.settle_all();
    rt.frame(0.0);
    assert!(rt.state.sql.focused());
    let after_line = query_rect(&rt, "sql-line-0");
    let after_gutter = query_rect(&rt, "sql-gutter-line-0");
    assert_eq!(after_line[1], after_gutter[1]);
    assert_eq!(after_line[3], after_gutter[3]);
    assert!(after_line[3] > before_line[3]);
    assert!(after_gutter[2] > before_gutter[2]);
    assert!(query_rect(&rt, "sql-gutter-number-0")[2] > before_number[2]);
    let after_caret = query_rect(&rt, "sql-caret");
    assert!(after_caret[3] > before_caret[3]);
    // Same caret byte, more distance from the gutter: the SQL glyphs, not just
    // the line boxes, have increased in size.
    assert!(after_caret[0] - after_line[0] > before_caret[0] - before_line[0]);
    assert_eq!(rt.state.sql.doc(), "SELECT '字号';\nSELECT 2;");
    assert_eq!(rt.state.sql.main_head(), 3);
}
