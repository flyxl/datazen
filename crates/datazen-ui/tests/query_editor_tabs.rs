use datazen_ui::{app_ime, app_key, app_poll, test_support::*};
use rune::{Ime, Key, KeyEvent, Mods, Tx};

fn tap(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("visible {key}"));
    tap_point(rt, x, y);
}

fn tap_point(rt: &mut Runtime<AppState>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn tap_tab(rt: &mut Runtime<AppState>, id: u32) {
    let [x, y, w, h] = rt.presented(&format!("tab-{id}")).expect("tab mounted").0;
    let count = rt.state.tabs.tabs().len();
    // Use the leading label area, not the tab's nested close button.
    tap_point(rt, x + 8.0_f32.min(w / 4.0), y + h / 2.0);
    assert_eq!(rt.state.tabs.tabs().len(), count, "selecting a tab cannot close it");
    assert_eq!(rt.state.tabs.selected().unwrap().id, id, "clicked tab is selected");
}

fn key(rt: &mut Runtime<AppState>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    rt.frame(0.0);
}

#[test]
fn tab_changes_keep_native_drafts_ranges_undo_redo_and_ignore_late_ime() {
    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.state.workspace.connect("local").unwrap();

    let a = rt.state.open_query_tab("Query").unwrap();
    assert_eq!(rt.state.sql.doc(), "", "a new query is blank");
    rt.frame(0.0);
    tap(&mut rt, "sql-editor-scroll");
    key(&mut rt, Key::Text("SELECT 101".into()), Mods::default());
    rt.state.sql.set_main_selection(7, 10);
    let a_sel = rt.state.sql.selection().main();

    // Uncommitted preedit belongs only to the outgoing editing surface.
    assert!(app_ime(&mut rt.state, &Ime::Preedit("未提交".into(), None), &mut Tx::default()));
    assert_eq!(rt.state.sql.composition(), Some("未提交"));
    let b = rt.state.open_query_tab("Query").unwrap();
    assert_ne!(a, b, "identical titles do not identify the document");
    rt.frame(0.0);
    assert!(!app_ime(&mut rt.state, &Ime::Commit("迟到".into()), &mut Tx::default()));
    assert_eq!(rt.state.sql.doc(), "", "a late commit can't hit a newly opened tab");
    assert_eq!(rt.state.sql.composition(), None);

    tap(&mut rt, "sql-editor-scroll");
    // MemoryWorkspace only executes SELECT 1 (real SQLite proof separately).
    key(&mut rt, Key::Text("SELECT 1".into()), Mods::default());
    key(&mut rt, Key::Command('z'), Mods::default());
    assert_eq!(rt.state.sql.doc(), "");

    tap_tab(&mut rt, a);
    assert_eq!(rt.state.sql.doc(), "SELECT 101");
    assert_eq!(rt.state.sql.selection().main(), a_sel);
    assert_eq!(rt.state.sql.composition(), None, "preedit is not durable draft text");
    assert!(!rt.state.sql.focused());
    tap(&mut rt, "sql-editor-scroll");
    key(&mut rt, Key::Command('z'), Mods::default());
    assert_eq!(rt.state.sql.doc(), "", "undo only visits A's own previous edit");
    key(&mut rt, Key::Command('z'), Mods { shift: true, ..Mods::default() });
    assert_eq!(rt.state.sql.doc(), "SELECT 101");

    tap_tab(&mut rt, b);
    assert_eq!(rt.state.sql.doc(), "");
    tap(&mut rt, "sql-editor-scroll");
    key(&mut rt, Key::Command('z'), Mods { shift: true, ..Mods::default() });
    assert_eq!(rt.state.sql.doc(), "SELECT 1", "B's redo survived while parked");
    key(&mut rt, Key::Enter, Mods { cmd: true, ..Mods::default() });
    assert_eq!(rt.state.result_grid.as_ref().unwrap().cell(0, 0), "1");

    assert!(rt.state.close_tab(b));
    assert_eq!(rt.state.tabs.selected().unwrap().id, a);
    assert_eq!(rt.state.sql.doc(), "SELECT 101", "closing B restores surviving A");
    let c = rt.state.open_query_tab("Query").unwrap();
    assert_ne!(c, b);
    assert_eq!(rt.state.sql.doc(), "", "a replacement cannot inherit a closed tab's draft");
    rt.state.disconnect();
    assert!(rt.state.tabs.tabs().is_empty());
    assert_eq!(rt.state.sql.doc(), "");
}

#[test]
fn scroll_offsets_follow_the_selected_tab_and_survive_hiding_workspace() {
    fn editor_offset(rt: &Runtime<AppState>) -> [f32; 2] {
        // Rune reports the ancestor scrolls which carry a node, not the
        // scroll offset of the viewport on the viewport's element itself.
        rt.elements()
            .into_iter()
            .find(|node| node.key.as_deref() == Some("sql-editor-field") && !node.exiting)
            .expect("document is mounted in the editor viewport")
            .scrolled
    }

    fn wheel_editor(rt: &mut Runtime<AppState>, offset: [f32; 2]) {
        let (x, y) = rt.find("sql-editor-scroll").expect("editor viewport is visible");
        // The first predominant axis owns a Rune wheel gesture. Deliver
        // separate unphased gestures, as with horizontal then vertical wheel.
        rt.wheel_xy(x, y, -offset[0], 0.0);
        rt.frame(0.15);
        rt.wheel_xy(x, y, 0.0, -offset[1]);
        rt.frame(0.15);
    }

    let mut rt = runtime();
    rt.poll = Some(app_poll);
    rt.state.workspace.connect("local").unwrap();
    let a = rt.state.open_query_tab("Query").unwrap();
    let long = format!("SELECT '{}'\n{}", "x".repeat(300), "-- scroll\n".repeat(100));
    rt.state.sql = sql_editor::SqlEditor::from_doc(&long);
    rt.frame(0.05);
    assert_eq!(editor_offset(&rt), [0.0, 0.0]);
    let a_offset = [100.0, 340.0];
    wheel_editor(&mut rt, a_offset);
    assert_eq!(editor_offset(&rt), a_offset, "both wheel axes move A");

    let b = rt.state.open_query_tab("Query").unwrap();
    // Make B scrollable too; a clamped shared offset would otherwise pass.
    rt.state.sql = sql_editor::SqlEditor::from_doc(&long);
    rt.frame(0.05);
    assert_eq!(editor_offset(&rt), [0.0, 0.0]);
    let b_offset = [45.0, 180.0];
    wheel_editor(&mut rt, b_offset);
    assert_eq!(editor_offset(&rt), b_offset, "B uses its own offset");

    tap_tab(&mut rt, a);
    rt.frame(0.05);
    assert_eq!(editor_offset(&rt), a_offset, "A restores both saved axes");
    tap(&mut rt, "rail-dashboards");
    assert!(rt.find("sql-editor-scroll").is_none(), "editor actually unmounted");
    tap(&mut rt, "rail-workspace");
    rt.frame(0.05);
    assert_eq!(editor_offset(&rt), a_offset, "unmounting the editor cannot reset the draft");
    tap_tab(&mut rt, b);
    rt.frame(0.05);
    assert_eq!(editor_offset(&rt), b_offset, "B restores a different saved viewport");
}

#[test]
fn refreshed_catalog_preserves_draft_and_explicit_target_but_new_profile_cannot_run_it() {
    let mut s = AppState::default();
    s.desktop_connection = Some("profile-a".into());
    let a = s.open_query_tab("Query").unwrap();
    s.sql = sql_editor::SqlEditor::from_doc("SELECT 314");
    s.query_database_targets.insert(a, NativeQueryDatabase {
        connection_id: "profile-a".into(),
        database: "explicit-db".into(),
    });

    for current in ["profile-a", "profile-b"] {
        s.deliver_host_job(AdapterDelivery::ConnectionOpened {
            generation: s.read_generation,
            id: current.into(),
            catalog: Ok(Catalog { databases: vec![] }),
        });
        assert_eq!(s.tabs.selected().unwrap().id, a);
        assert_eq!(s.sql.doc(), "SELECT 314");
        assert_eq!(s.query_database_targets[&a].database, "explicit-db");
        let generation = s.query_generation;
        s.run_query();
        if current == "profile-a" {
            // Default adapter cannot open this explicit target; the memory
            // workspace must not become a fallback.
            assert_eq!(s.query_notice.as_deref(), Some("The selected query database is no longer available."));
        } else {
            assert!(s.query_notice.as_deref().unwrap().contains("belongs to a different connection"));
            assert_eq!(s.query_generation, generation, "no new execution generation on wrong profile");
        }
        assert!(s.result_grid.is_none());
    }
    let b = s.open_query_tab("Query").unwrap();
    assert_ne!(a, b);
    assert_eq!(s.sql.doc(), "");
    assert!(!s.query_database_targets.contains_key(&b));
}

#[test]
fn sql_gutter_numbers_follow_document_rows_and_stay_left_while_scrolling() {
    let mut rt = runtime();
    rt.poll = Some(app_poll);
    rt.state.workspace.connect("local").unwrap();
    rt.state.open_query_tab("Query").unwrap();
    // Keep enough rows for the full 80pt scroll even with the taller idle editor.
    let long = format!("SELECT '{}';\n{}", "x".repeat(300), "-- next\n".repeat(64));
    rt.state.sql = sql_editor::SqlEditor::from_doc(long);
    rt.state.sql.set_main_selection(0, 6);
    rt.frame(0.05);
    assert!(rt.find("sql-cursor-count").is_none(), "no development status row");
    assert!(rt.find("sql-selected-0-0").is_some());
    assert!(rt.find("sql-find-bar").is_none(), "the dock is opt-in");

    let box_of = |rt: &Runtime<AppState>, key: &str| rt.presented(key).expect(key).0;
    let view = box_of(&rt, "sql-editor-scroll");
    let gutter = box_of(&rt, "sql-editor-gutter");
    let first = box_of(&rt, "sql-gutter-line-0");
    assert_eq!(first[1] - view[1], 12.0, "document and gutter share top padding");
    for index in [0, 1, 9, 11, 31] {
        let line = box_of(&rt, &format!("sql-line-{index}"));
        let number = box_of(&rt, &format!("sql-gutter-line-{index}"));
        assert_eq!([line[1], line[3]], [number[1], number[3]], "line {index}");
        let number_key = format!("sql-gutter-number-{index}");
        let n = rt.elements().into_iter()
            .find(|el| el.key.as_deref() == Some(number_key.as_str())).expect("number mounted");
        assert_eq!(n.text.as_deref(), Some((index + 1).to_string().as_str()));
    }

    let (x, y) = rt.find("sql-editor-scroll").unwrap();
    rt.wheel_xy(x, y, -100.0, 0.0);
    rt.frame(0.15);
    let moved = box_of(&rt, "sql-line-11");
    assert_eq!(box_of(&rt, "sql-editor-gutter")[0], gutter[0], "sticky horizontal gutter");
    assert!(moved[0] < first[0], "code actually scrolled under the gutter");
    rt.wheel_xy(x, y, 0.0, -80.0);
    rt.frame(0.15);
    let number = box_of(&rt, "sql-gutter-line-11");
    let document = box_of(&rt, "sql-line-11");
    assert_eq!(moved[1] - document[1], 80.0);
    assert_eq!(number[1], document[1], "vertical scroll stays aligned");
}

#[test]
fn find_dock_scopes_real_shortcuts_and_returns_focus_without_editing_the_draft() {
    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.state.workspace.connect("local").unwrap();
    let a = rt.state.open_query_tab("Query").unwrap();
    let sql = "SELECT '中文';\nSELECT '中文';";
    rt.state.sql = sql_editor::SqlEditor::from_doc(sql);
    rt.frame(0.0);
    let doc = |rt: &Runtime<AppState>| rt.presented("sql-editor-scroll").unwrap().0;
    let closed = doc(&rt);
    assert!(rt.find("sql-find-bar").is_none());
    assert!(rt.find("sql-find-open").is_some());

    let command = Mods { cmd: true, ..Mods::default() };
    tap(&mut rt, "sql-editor-scroll");
    key(&mut rt, Key::Command('f'), command);
    let open = doc(&rt);
    let dock = rt.presented("sql-find-bar").unwrap().0;
    assert_eq!(open[1], closed[1], "search does not shift the top of the document");
    assert!((open[1] + open[3] - dock[1]).abs() < 0.1, "dock immediately below the viewport");
    assert!((dock[3] - 52.0).abs() < 0.1, "compact two-row dock");
    assert!((closed[3] - open[3] - dock[3]).abs() < 0.1);
    for key in ["sql-find-prev", "sql-find-next", "sql-add-cursor", "sql-replace-input", "sql-replace-all", "sql-find-close"] {
        assert!(rt.find(key).is_some(), "{key} still reachable");
    }
    assert!(rt.state.sql.find_input.focused);
    key(&mut rt, Key::Text("中文".into()), Mods::default());
    assert_eq!(rt.state.sql.find_input.text, "中文");
    key(&mut rt, Key::Enter, command);
    assert!(rt.state.query_result_tabs.is_empty(), "search input does not run SQL");
    key(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.sql.selection().main().from(), 8);
    assert_eq!(rt.state.sql.selected_text(), "中文");
    assert!(rt.state.sql.find_input.focused, "Enter retains search focus");
    key(&mut rt, Key::Function(3), Mods::default());
    assert_eq!(rt.state.sql.selection().main().from(), 25);
    key(&mut rt, Key::Command('g'), Mods { shift: true, ..command });
    assert_eq!(rt.state.sql.selection().main().from(), 8);
    tap(&mut rt, "sql-find-next");
    assert_eq!(rt.state.sql.selection().main().from(), 25);
    key(&mut rt, Key::Enter, command);
    assert!(rt.state.query_result_tabs.is_empty(), "search buttons do not run SQL either");
    key(&mut rt, Key::Escape, Mods::default());
    assert!(rt.find("sql-find-bar").is_none());
    assert!(rt.state.sql.focused(), "Escape returns to the actual editor");
    assert_eq!(doc(&rt), closed, "dock doesn't reserve idle space");
    assert_eq!(rt.state.sql.doc(), sql);
    assert!(rt.state.sql.undo_stack.is_empty(), "search is not a document edit");

    tap(&mut rt, "sql-find-open");
    assert!(rt.state.sql.find_input.focused, "the visible control opens and focuses Find");
    assert_eq!(rt.state.sql.find_input.text, "中文");
    tap(&mut rt, "toolbar-new-query");
    let b = rt.state.tabs.selected().expect("new Query mounted").id;
    assert_ne!(a, b);
    assert!(rt.find("sql-find-bar").is_none());
    assert!(rt.state.sql.find_input.text.is_empty());
    tap_tab(&mut rt, a);
    assert!(rt.find("sql-find-bar").is_none());
    assert_eq!(rt.state.sql.find_input.text, "中文", "needle belongs to its own query draft");
    assert_eq!(rt.state.sql.doc(), sql);
    assert!(rt.state.sql.undo_stack.is_empty());

    tap(&mut rt, "sql-find-open");
    tap(&mut rt, "sql-find-close");
    assert!(rt.find("sql-find-bar").is_none());
    assert!(rt.state.sql.focused(), "explicit close restores the editor focus");
    let match_range = rt.state.sql.selection().main();
    assert!(match_range.from() < match_range.to(), "closing preserves the selected match");
    key(&mut rt, Key::Left, Mods::default());
    let caret = rt.state.sql.selection().main();
    assert_eq!((caret.from(), caret.to()), (match_range.from(), match_range.from()),
        "ArrowLeft in the editor collapses the match to its leading edge");
}

#[test]
fn tab_accepts_only_a_current_completion_in_the_focused_query_canvas() {
    let mut rt = runtime();
    rt.set_full_keyboard_access(true);
    rt.on_key = Some(app_key);
    rt.state.workspace.connect("local").unwrap();
    let a = rt.state.open_query_tab("A").unwrap();
    rt.frame(0.0);
    let focused_canvas = |rt: &Runtime<AppState>| rt.elements().iter()
        .any(|node| node.key.as_deref() == Some("sql-editor-scroll") && node.focused);
    let plain = Mods::default();

    tap(&mut rt, "sql-editor-scroll");
    assert!(focused_canvas(&rt));
    key(&mut rt, Key::Tab, plain);
    assert!(!focused_canvas(&rt), "without a suggestion Rune still moves focus");
    assert_eq!(rt.state.sql.doc(), "");

    tap(&mut rt, "sql-editor-scroll");
    key(&mut rt, Key::Text("s".into()), plain);
    assert!(rt.find("sql-completion").is_some());
    key(&mut rt, Key::Tab, Mods { shift: true, ..plain });
    assert!(!focused_canvas(&rt), "Shift+Tab keeps reverse focus traversal");
    assert_eq!(rt.state.sql.doc(), "s");
    key(&mut rt, Key::Tab, plain);
    assert_eq!(rt.state.sql.doc(), "s", "Tab from outside cannot accept");

    tap(&mut rt, "sql-editor-scroll");
    key(&mut rt, Key::Down, plain);
    key(&mut rt, Key::Tab, plain);
    assert!(focused_canvas(&rt));
    assert_eq!(rt.state.sql.doc(), "SET");
    assert!(rt.find("sql-completion").is_none());
    key(&mut rt, Key::Command('z'), plain);
    assert_eq!(rt.state.sql.doc(), "s", "acceptance is one edit");

    key(&mut rt, Key::Text("e".into()), plain);
    assert!(rt.find("sql-completion").is_some());
    assert!(app_ime(&mut rt.state, &Ime::Preedit("未提交".into(), None), &mut Tx::default()));
    key(&mut rt, Key::Tab, plain);
    assert_eq!(rt.state.sql.doc(), "se", "an IME preedit is not a completion");
    assert_eq!(rt.state.sql.composition(), Some("未提交"));
    rt.state.sql.preedit("");

    tap(&mut rt, "sql-editor-scroll");
    key(&mut rt, Key::Text("l".into()), plain);
    assert!(rt.find("sql-completion").is_some());
    let b = rt.state.open_query_tab("B").unwrap();
    rt.frame(0.0);
    key(&mut rt, Key::Tab, plain);
    assert_eq!(rt.state.tabs.selected().unwrap().id, b);
    assert_eq!(rt.state.sql.doc(), "", "the previous draft's popup cannot write B");
    tap_tab(&mut rt, a);
    assert_eq!(rt.state.sql.doc(), "sel");
    assert!(rt.find("sql-completion").is_none());
    assert!(rt.state.query_result_tabs.is_empty(), "no key in this journey runs SQL");
}

#[test]
fn native_execute_shortcut_requires_the_focused_tab_and_no_ime_composition() {
    struct OneBinding;
    impl DesktopAdapter for OneBinding {
        fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
            Ok(Vec::new())
        }

        fn read_table_page(&mut self, _: &str, _: u32, _: u32,
                           _: Option<&ColumnSort>, _: Option<&ColumnFilter>) -> Result<TablePage, String> {
            Err(DESKTOP_NOT_IMPLEMENTED.into())
        }

        fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
            Err(DESKTOP_NOT_IMPLEMENTED.into())
        }

        fn native_execute_shortcut(&self) -> Option<NativeExecuteShortcut> {
            Some(NativeExecuteShortcut::ModR)
        }
    }
    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.state.desktop = Box::new(OneBinding);
    rt.state.workspace.connect("local").unwrap();
    let a = rt.state.open_query_tab("A").unwrap();
    rt.state.sql = sql_editor::SqlEditor::from_doc("SELECT 1");
    rt.frame(0.05);
    tap(&mut rt, "sql-editor-scroll");
    let primary = Mods { cmd: true, ..Mods::default() };
    let unchanged_generation = rt.state.query_generation;

    assert!(app_ime(&mut rt.state, &Ime::Preedit("未提交".into(), None), &mut Tx::default()));
    key(&mut rt, Key::Command('r'), primary);
    assert_eq!(rt.state.sql.composition(), Some("未提交"));
    assert_eq!(rt.state.query_generation, unchanged_generation);
    assert_eq!(rt.state.sql.doc(), "SELECT 1");

    rt.state.sql.preedit("");
    key(&mut rt, Key::Enter, primary);
    assert_eq!(rt.state.sql.doc(), "SELECT 1", "the inactive shortcut must not insert a newline");
    key(&mut rt, Key::Command('r'), Mods { shift: true, ..primary });
    assert_eq!(rt.state.query_generation, unchanged_generation);

    let b = rt.state.open_query_tab("B").unwrap();
    rt.frame(0.05);
    // Parked editor A's focus cannot be borrowed by B.
    key(&mut rt, Key::Command('r'), primary);
    assert_eq!(rt.state.query_generation, unchanged_generation);
    assert_eq!(rt.state.tabs.selected().unwrap().id, b);

    tap_tab(&mut rt, a);
    tap(&mut rt, "sql-editor-scroll");
    key(&mut rt, Key::Command('r'), primary);
    assert_ne!(rt.state.query_generation, unchanged_generation);
    assert_eq!(rt.state.result_grid.as_ref().unwrap().cell(0, 0), "1");
}

#[test]
fn dock_replace_seeks_then_edits_through_real_button_and_replacement_enter() {
    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.state.workspace.connect("local").unwrap();
    let a = rt.state.open_query_tab("A").unwrap();
    let sql = "SELECT 'one one';";
    rt.state.sql = sql_editor::SqlEditor::from_doc(sql);
    rt.frame(0.0);

    tap(&mut rt, "sql-find-open");
    key(&mut rt, Key::Text("one".into()), Mods::default());
    tap(&mut rt, "sql-replace-input");
    key(&mut rt, Key::Text("gone".into()), Mods::default());
    tap(&mut rt, "sql-replace-current");
    assert_eq!(rt.state.sql.doc(), sql);
    assert_eq!(rt.state.sql.selected_text(), "one");
    assert_eq!(rt.state.sql.selection().main().from(), 8);
    assert!(rt.state.sql.undo_stack.is_empty());

    tap(&mut rt, "sql-replace-input");
    key(&mut rt, Key::Enter, Mods { cmd: true, ..Mods::default() });
    assert_eq!(rt.state.sql.doc(), sql, "the replacement field cannot execute SQL");
    assert!(rt.state.query_result_tabs.is_empty());
    key(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.sql.doc(), "SELECT 'gone one';");
    assert_eq!(rt.state.sql.selection().main().from(), 13);
    assert_eq!(rt.state.sql.selected_text(), "one");
    assert!(rt.state.sql.replace_input.focused);

    key(&mut rt, Key::Escape, Mods::default());
    assert!(rt.find("sql-find-bar").is_none());
    assert!(rt.state.sql.focused());
    let replaced_range = rt.state.sql.selection().main();
    tap(&mut rt, "toolbar-new-query");
    let b = rt.state.tabs.selected().unwrap().id;
    assert_ne!(a, b);
    assert_eq!(rt.state.sql.doc(), "");
    assert!(rt.state.sql.replace_input.text.is_empty());
    tap_tab(&mut rt, a);
    assert_eq!(rt.state.sql.doc(), "SELECT 'gone one';");
    assert_eq!(rt.state.sql.selection().main(), replaced_range);
    assert_eq!(rt.state.sql.replace_input.text, "gone");

    tap(&mut rt, "sql-editor-scroll");
    key(&mut rt, Key::Command('z'), Mods { cmd: true, ..Mods::default() });
    assert_eq!(rt.state.sql.doc(), sql);
    assert!(rt.state.sql.undo_stack.is_empty());
    tap_tab(&mut rt, b);
    assert_eq!(rt.state.sql.doc(), "", "replacement undo belongs to A only");
}

#[test]
fn dock_find_wraps_the_only_match_and_navigates_overlapping_matches_by_edge() {
    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.state.workspace.connect("local").unwrap();
    let _ = rt.state.open_query_tab("Find").unwrap();
    let sole = "SELECT '猫';";
    rt.state.sql = sql_editor::SqlEditor::from_doc(sole);
    rt.frame(0.0);
    rt.state.sql.set_main_selection(11, 8);
    rt.frame(0.0);
    tap(&mut rt, "sql-find-open");
    assert_eq!(rt.state.sql.find_input.text, "猫");
    for direction in ["sql-find-next", "sql-find-prev"] {
        rt.state.sql.set_main_selection(11, 8);
        rt.frame(0.0);
        tap(&mut rt, direction);
        let actual = rt.state.sql.selection().main();
        // The original command dispatches even when wrap finds the same
        // bounds. A backward selection now has a forward anchor and head.
        assert_eq!((actual.anchor, actual.head), (8, 11), "{direction}");
        assert_eq!(rt.state.sql.selected_text(), "猫");
    }
    assert_eq!(rt.state.sql.doc(), sole);
    assert!(rt.state.sql.undo_stack.is_empty());

    let overlap = "SELECT 'aabbbba' AS overlap;";
    rt.state.sql = sql_editor::SqlEditor::from_doc(overlap);
    rt.frame(0.0);
    rt.state.sql.set_main_selection(overlap.len(), 0);
    rt.frame(0.0);
    tap(&mut rt, "sql-find-open");
    key(&mut rt, Key::Text("bb".into()), Mods::default());
    assert_eq!(rt.state.sql.find_input.text, "bb");
    key(&mut rt, Key::Enter, Mods::default());
    let first = rt.state.sql.selection().main();
    assert_eq!((first.anchor, first.head), (10, 12));
    rt.state.sql.set_main_selection(overlap.len(), 0);
    rt.frame(0.0);
    key(&mut rt, Key::Enter, Mods { shift: true, ..Mods::default() });
    let last = rt.state.sql.selection().main();
    assert_eq!((last.anchor, last.head), (12, 14));
    key(&mut rt, Key::Escape, Mods::default());
    assert!(rt.state.sql.focused());
    assert_eq!(rt.state.sql.doc(), overlap);
    assert!(rt.state.sql.undo_stack.is_empty());
    assert!(rt.state.query_result_tabs.is_empty());
}

#[test]
fn dock_literal_short_multiline_selection_seeds_the_live_search_field() {
    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.state.workspace.connect("local").unwrap();
    let _a = rt.state.open_query_tab("A").unwrap();
    let sql = "猫 a\n\tb / a\n\tb";
    rt.state.sql = sql_editor::SqlEditor::from_doc(sql);
    rt.frame(0.0);
    tap(&mut rt, "sql-editor-scroll");
    rt.state.sql.set_main_selection(4, 8);
    rt.frame(0.0);
    key(&mut rt, Key::Command('f'), Mods { cmd: true, ..Mods::default() });
    assert_eq!(rt.state.sql.find_input.text, "a\\n\tb");
    assert!(rt.state.sql.find_input.focused);
    key(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.sql.selection().main().from(), 11);
    assert_eq!(rt.state.sql.selected_text(), "a\n\tb");
    key(&mut rt, Key::Enter, Mods { shift: true, ..Mods::default() });
    assert_eq!(rt.state.sql.selection().main().from(), 4);
    key(&mut rt, Key::Escape, Mods::default());
    assert!(rt.state.sql.focused());
    assert_eq!(rt.state.sql.doc(), sql);
    assert!(rt.state.sql.undo_stack.is_empty());
    assert!(rt.state.query_result_tabs.is_empty());
}

#[test]
fn dock_unicode_literal_match_case_is_real_and_belongs_to_its_query_draft() {
    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.state.workspace.connect("local").unwrap();
    let a = rt.state.open_query_tab("Alpha").unwrap();
    let sql = "FI / ﬃ / fi";
    rt.state.sql = sql_editor::SqlEditor::from_doc(sql);
    rt.frame(0.0);
    tap(&mut rt, "sql-find-open");
    key(&mut rt, Key::Text("fi".into()), Mods::default());
    key(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.sql.selected_text(), "FI");
    let case = rt.elements().into_iter().find(|el| el.key.as_deref() == Some("sql-find-match-case")).unwrap();
    assert_eq!(case.role, "checkbox");
    tap(&mut rt, "sql-find-match-case");
    tap(&mut rt, "sql-find-next");
    assert_eq!(rt.state.sql.selected_text(), "ﬃ");
    tap(&mut rt, "sql-find-next");
    assert_eq!(rt.state.sql.selected_text(), "fi");
    tap(&mut rt, "sql-find-next");
    assert_eq!(rt.state.sql.selected_text(), "ﬃ", "wrap excludes uppercase FI");
    key(&mut rt, Key::Escape, Mods::default());
    assert!(rt.state.sql.focused());
    assert!(rt.state.sql.undo_stack.is_empty());

    let b = rt.state.open_query_tab("Beta").unwrap();
    assert_ne!(a, b);
    rt.state.sql = sql_editor::SqlEditor::from_doc("FI");
    rt.frame(0.0);
    tap(&mut rt, "sql-find-open");
    key(&mut rt, Key::Text("fi".into()), Mods::default());
    key(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.sql.selected_text(), "FI", "a new draft uses the default");
    key(&mut rt, Key::Escape, Mods::default());

    tap_tab(&mut rt, a);
    rt.state.sql.set_main_selection(0, 0);
    tap(&mut rt, "sql-find-open");
    assert_eq!(rt.state.sql.find_input.text, "fi");
    key(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.sql.selected_text(), "ﬃ", "tab A kept Match Case");
    assert_eq!(rt.state.sql.doc(), sql);
    assert!(rt.state.query_result_tabs.is_empty());
}

#[test]
fn dock_unicode_literal_reports_invalid_selection_without_changing_the_editor() {
    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.state.workspace.connect("local").unwrap();
    let _ = rt.state.open_query_tab("Query").unwrap();
    let sql = "SELECT 'x\u{1d15e}y';";
    rt.state.sql = sql_editor::SqlEditor::from_doc(sql);
    rt.frame(0.0);
    tap(&mut rt, "sql-find-open");
    key(&mut rt, Key::Text("\u{1d165}".into()), Mods::default());
    key(&mut rt, Key::Enter, Mods::default());
    assert!(rt.find("sql-find-unsafe-unicode").is_some());
    assert_eq!(rt.state.sql.selected_text(), "");
    tap(&mut rt, "sql-replace-input");
    key(&mut rt, Key::Text("Z".into()), Mods::default());
    tap(&mut rt, "sql-replace-all");
    assert!(rt.find("sql-find-unsafe-unicode").is_some());
    assert_eq!(rt.state.sql.doc(), sql);
    assert!(rt.state.sql.undo_stack.is_empty());
    assert!(rt.state.query_result_tabs.is_empty());
    key(&mut rt, Key::Escape, Mods::default());
    assert!(rt.find("sql-find-unsafe-unicode").is_none());
}

#[test]
fn find_whole_word_and_case_are_live_per_query_draft_options() {
    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.poll = Some(app_poll);
    rt.state.workspace.connect("local").unwrap();
    let a = rt.state.open_query_tab("whole word draft").unwrap();
    let text = "xcat CAT cat_ cat";
    rt.state.sql = sql_editor::SqlEditor::from_doc(text);
    rt.settle_all();
    rt.frame(0.0);
    tap(&mut rt, "sql-find-open");
    rt.paste("cat");
    rt.frame(0.0);
    assert!(rt.elements().iter().any(|n| !n.exiting
        && n.key.as_deref() == Some("sql-find-whole-word") && n.role == "checkbox"));
    tap(&mut rt, "sql-find-whole-word");
    tap(&mut rt, "sql-find-next");
    assert_eq!(rt.state.sql.selected_text(), "CAT");
    tap(&mut rt, "sql-find-match-case");
    tap(&mut rt, "sql-find-next");
    assert_eq!(rt.state.sql.selection().main().from(), 14);
    assert_eq!(rt.state.sql.selected_text(), "cat");
    assert_eq!(rt.state.sql.doc(), text);
    assert!(rt.state.sql.undo_stack.is_empty());

    let b = rt.state.open_query_tab("untouched options").unwrap();
    rt.state.sql = sql_editor::SqlEditor::from_doc(text);
    rt.frame(0.0);
    tap(&mut rt, "sql-find-open");
    rt.paste("cat");
    rt.frame(0.0);
    tap(&mut rt, "sql-find-next");
    assert_eq!(rt.state.sql.selection().main().from(), 1, "new tab defaults to substring mode");
    assert_eq!(rt.state.tabs.selected().unwrap().id, b);

    assert!(rt.state.select_tab(a));
    rt.state.sql.set_main_caret(0);
    rt.frame(0.0);
    tap(&mut rt, "sql-find-open");
    tap(&mut rt, "sql-find-next");
    assert_eq!(rt.state.sql.selection().main().from(), 14, "A kept both checked options");
    assert_eq!(rt.state.sql.doc(), text);
    assert!(rt.state.sql.undo_stack.is_empty());
}

#[test]
fn manual_completion_opens_only_in_a_focused_query_not_dock_or_composition() {
    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.state.workspace.connect("local").unwrap();
    let query = rt.state.open_query_tab("Explicit").unwrap();
    rt.frame(0.05);
    tap(&mut rt, "sql-editor-scroll");
    let plain = Mods::default();
    let ctrl = Mods { ctrl: true, ..plain };
    key(&mut rt, Key::Text(" ".into()), Mods { ctrl: true, shift: true, ..plain });
    assert!(!rt.state.sql.has_completion(), "not a fuzzy chord");

    key(&mut rt, Key::Text(" ".into()), plain);
    assert_eq!(rt.state.sql.doc(), " ");
    assert!(!rt.state.sql.has_completion(), "ordinary Space does not start empty-prefix completion");
    key(&mut rt, Key::Text(" ".into()), ctrl);
    assert!(rt.find("sql-completion").is_some());
    assert_eq!(rt.state.sql.doc(), " ", "manual invocation cannot insert a Space");
    key(&mut rt, Key::Escape, plain);
    assert!(!rt.state.sql.has_completion());

    assert!(app_ime(&mut rt.state, &Ime::Preedit("输入".into(), None), &mut Tx::default()));
    key(&mut rt, Key::Text(" ".into()), ctrl);
    assert!(!rt.state.sql.has_completion());
    assert_eq!(rt.state.sql.composition(), Some("输入"));
    rt.state.sql.preedit("");

    key(&mut rt, Key::Command('f'), Mods { cmd: true, ..plain });
    assert!(rt.find("sql-find-input").is_some());
    key(&mut rt, Key::Text(" ".into()), ctrl);
    assert!(!rt.state.sql.has_completion(), "Find cannot start Query completion");
    assert_eq!(rt.state.sql.doc(), " ");
    key(&mut rt, Key::Escape, plain);
    rt.state.result_focused = true;
    key(&mut rt, Key::Text(" ".into()), ctrl);
    assert!(rt.state.result_focused, "the chord must not transfer Results focus");
    assert!(!rt.state.sql.has_completion());

    let other = rt.state.open_query_tab("Other").unwrap();
    rt.frame(0.05);
    key(&mut rt, Key::Text(" ".into()), ctrl);
    assert_eq!(rt.state.tabs.selected().unwrap().id, other);
    assert!(!rt.state.sql.has_completion(), "a parked draft's focus is not inherited");
    tap_tab(&mut rt, query);
    assert_eq!(rt.state.sql.doc(), " ");
    assert!(rt.state.query_result_tabs.is_empty());
}

#[test]
fn manual_completion_from_catalog_requires_current_tab_target_and_session_generation() {
    struct CatalogAdapter;

    impl DesktopAdapter for CatalogAdapter {
        fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
            Ok(Vec::new())
        }
        fn read_table_page(
            &mut self, _: &str, _: u32, _: u32, _: Option<&ColumnSort>, _: Option<&ColumnFilter>,
        ) -> Result<TablePage, String> {
            Err(DESKTOP_NOT_IMPLEMENTED.into())
        }
        fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
            Err(DESKTOP_NOT_IMPLEMENTED.into())
        }
        fn native_from_catalog_target(&self) -> Option<NativeQueryDatabase> {
            Some(NativeQueryDatabase { connection_id: "fixture".into(), database: "main".into() })
        }
    }

    let mut rt = runtime();
    rt.on_key = Some(app_key);
    rt.state.desktop = Box::new(CatalogAdapter);
    rt.state.desktop_connection = Some("fixture".into());
    rt.state.desktop_catalog = Some(Catalog {
        databases: vec![
            DatabaseNode { name: "decoy".into(), schemas: vec![SchemaNode {
                name: "public".into(), tables: vec!["wrong_customer".into()], views: Vec::new(),
            }] },
            DatabaseNode { name: "main".into(), schemas: vec![SchemaNode {
                name: "main".into(), tables: vec!["customers".into()], views: Vec::new(),
            }] },
        ],
    });
    rt.state.navigator_value_search_session_generation = Some(17);
    let tab = rt.state.open_query_tab("Explicit catalog").unwrap();
    let prefix = "SELECT * FROM ";
    rt.state.sql = sql_editor::SqlEditor::from_doc(prefix);
    rt.state.sql.set_main_caret(prefix.len());
    rt.frame(0.05);
    tap(&mut rt, "sql-editor-scroll");
    rt.state.sql.set_main_caret(prefix.len());
    let ctrl = Mods { ctrl: true, ..Mods::default() };
    key(&mut rt, Key::Text(" ".into()), ctrl);
    assert_eq!(rt.state.sql.completion_items(), &["\"main\".\"customers\""]);
    assert_eq!(rt.state.sql.doc(), prefix);

    rt.state.navigator_value_search_session_generation = Some(18);
    key(&mut rt, Key::Tab, Mods::default());
    assert_eq!(rt.state.sql.doc(), prefix, "a stale same-target generation cannot accept");
    tap(&mut rt, "sql-editor-scroll");
    rt.state.sql.set_main_caret(prefix.len());
    rt.state.query_database_targets.insert(tab, NativeQueryDatabase {
        connection_id: "other-profile".into(), database: "main".into(),
    });
    key(&mut rt, Key::Text(" ".into()), ctrl);
    assert!(!rt.state.sql.has_completion(), "a stale override cannot fall back to the default DB");

    rt.state.query_database_targets.remove(&tab);
    key(&mut rt, Key::Text(" ".into()), ctrl);
    assert!(rt.state.sql.has_completion());
    key(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.sql.doc(), format!("{prefix}\"main\".\"customers\""));
    assert!(!rt.state.sql.has_completion());
    key(&mut rt, Key::Command('z'), Mods::default());
    assert_eq!(rt.state.sql.doc(), prefix);
    assert!(rt.state.query_result_tabs.is_empty());
}
