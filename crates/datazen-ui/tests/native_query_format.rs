use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use datazen_ui::{app_ime, app_key, app_poll, test_support::*};
use datazen_ui_api::native_sql_format::{
    options_from_settings, NativeSqlFormatContext, NativeSqlFormatDelivery, NativeSqlFormatError,
    NativeSqlFormatLanguage, NativeSqlFormatRequest,
    NATIVE_SQL_FORMAT_MAX_OUTPUT_BYTES,
};
use rune::{Ime, Key, KeyEvent, Mods, Tx};
use serde_json::json;

#[derive(Default)]
struct Wire {
    context: Option<NativeSqlFormatContext>,
    requests: Vec<NativeSqlFormatRequest>,
    cancelled: Vec<u64>,
    replies: VecDeque<NativeSqlFormatDelivery>,
    picker: Option<QueryDatabaseContext>,
}

struct Adapter(Rc<RefCell<Wire>>);

impl DesktopAdapter for Adapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> { Ok(Vec::new()) }
    fn read_table_page(
        &mut self, _: &str, _: u32, _: u32, _: Option<&ColumnSort>, _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> { Err(DESKTOP_NOT_IMPLEMENTED.to_owned()) }
    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.to_owned())
    }
    fn native_query_database_context(&self) -> Option<QueryDatabaseContext> {
        self.0.borrow().picker.clone()
    }
    fn native_sql_format_context(&self) -> Option<NativeSqlFormatContext> {
        self.0.borrow().context.clone()
    }
    fn begin_native_sql_format(&mut self, request: NativeSqlFormatRequest) -> bool {
        self.0.borrow_mut().requests.push(request);
        true
    }
    fn poll_native_sql_format(&mut self) -> Option<NativeSqlFormatDelivery> {
        self.0.borrow_mut().replies.pop_front()
    }
    fn cancel_native_sql_format(&mut self, generation: u64) {
        self.0.borrow_mut().cancelled.push(generation);
    }
}

fn mounted(doc: &str, settings: serde_json::Value, shortcut: &str)
    -> (Runtime<AppState>, Rc<RefCell<Wire>>, u32)
{
    let mut rt = runtime();
    let target = NativeQueryDatabase {
        connection_id: "local".into(), database: "main".into(),
    };
    let wire = Rc::new(RefCell::new(Wire {
        context: Some(NativeSqlFormatContext {
            session_generation: 7, default_target: target.clone(),
            language: NativeSqlFormatLanguage::Sqlite,
            options: options_from_settings(&settings).unwrap(),
            shortcut: Some(shortcut.to_owned()),
        }),
        picker: Some(QueryDatabaseContext { current: target, choices: vec!["main".into(), "other".into()] }),
        ..Default::default()
    }));
    rt.on_key = Some(app_key);
    rt.poll = Some(app_poll);
    rt.state.desktop = Box::new(Adapter(Rc::clone(&wire)));
    rt.state.workspace.connect("local").unwrap();
    rt.state.desktop_connection = Some("local".into());
    rt.state.navigator_value_search_session_generation = Some(7);
    let a = rt.state.open_query_tab("Formatter A").unwrap();
    rt.state.sql = sql_editor::SqlEditor::from_doc(doc);
    rt.frame(0.0);
    (rt, wire, a)
}

fn tap(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("mounted {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn key(rt: &mut Runtime<AppState>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    rt.frame(0.0);
}

fn primary() -> Mods {
    Mods { cmd: true, ctrl: !cfg!(target_os = "macos"), logo: cfg!(target_os = "macos"),
           ..Mods::default() }
}

fn stock_format(rt: &mut Runtime<AppState>) {
    key(rt, Key::Command('f'), Mods { shift: true, ..primary() });
}

fn reply(wire: &Rc<RefCell<Wire>>, generation: u64, result: Result<String, NativeSqlFormatError>) {
    wire.borrow_mut().replies.push_back(NativeSqlFormatDelivery { generation, result });
}

#[test]
fn native_query_format_toolbar_uses_exact_selected_text_and_one_undo() {
    let before = "select 1;\r\n    select 'q;z', lower('M') where true and false;\r\nselect 3;";
    let (mut rt, wire, _) = mounted(before, json!({"sqlFormatOptions": {
        "indentStyle": "4spaces", "keywordCase": "lower",
        "breakBeforeBooleanOperators": false, "linesBetweenQueries": 2,
    }}), "Mod-Shift-f");
    tap(&mut rt, "sql-editor-scroll");
    let from = before.find("select 'q").unwrap();
    let to = before.rfind("\r\n").unwrap();
    rt.state.sql.set_main_selection(to, from);
    let selection = rt.state.sql.selection().clone();
    rt.frame(0.0);
    tap(&mut rt, "sql-format");
    let generation = {
        let wire = wire.borrow();
        let [job] = wire.requests.as_slice() else { panic!("exactly one accepted toolbar job") };
        assert_eq!(job.sql, &before[from..to]);
        assert_eq!(job.session_generation, 7);
        assert_eq!(job.target.database, "main");
        assert_eq!(job.options.language, "sqlite");
        assert_eq!(job.options.keyword_case, "lower");
        assert_eq!(job.options.tab_width, 4);
        assert_eq!(job.options.logical_operator_newline, "after");
        assert_eq!(job.options.lines_between_queries, 2);
        job.generation
    };
    assert_eq!(rt.state.sql.doc(), before, "no edit while job is pending");
    assert_eq!(rt.state.sql.selection(), &selection);
    // Actual formatter oracle text, with the engine replaced only in the
    // mounted test. No filesystem, DB or side process implements this test port.
    reply(&wire, generation, Ok("select\n    'q;z',\n    lower('M')\nwhere\n    true and\n    false;".to_owned()));
    rt.frame(1.0 / 25.0);
    let expected = "select 1;\r\n    select\n        'q;z',\n        lower('M')\n    where\n        true and\n        false;\r\nselect 3;";
    assert_eq!(rt.state.sql.doc(), expected);
    assert_eq!(rt.state.sql.selection().main(), sql_editor::Sel {
        anchor: from, head: expected.rfind("\r\n").unwrap(),
    });
    assert_eq!(rt.state.sql.undo_stack.len(), 1);
    key(&mut rt, Key::Command('z'), primary());
    assert_eq!(rt.state.sql.doc(), before);
    assert_eq!(rt.state.sql.selection(), &selection);
    assert!(rt.state.sql.undo_stack.is_empty());
    assert!(rt.state.query_result_tabs.is_empty());
}

#[test]
fn native_query_format_shortcut_and_latest_reply_never_retarget_a_draft() {
    let before = "select 1;\nselect 2;";
    let (mut rt, wire, a) = mounted(before, json!({}), "Mod-Alt-F8");
    tap(&mut rt, "sql-editor-scroll");
    rt.state.sql.set_main_caret(before.rfind("2").unwrap());
    let custom = |rt: &mut Runtime<AppState>| key(rt, Key::Function(8), Mods { alt: true, ..primary() });
    stock_format(&mut rt);
    assert!(wire.borrow().requests.is_empty(), "custom binding replaces stock");
    custom(&mut rt);
    custom(&mut rt);
    let generations = wire.borrow().requests.iter().map(|job| job.generation).collect::<Vec<_>>();
    assert_eq!(generations.len(), 2);
    assert_eq!(wire.borrow().cancelled, [generations[0]]);
    reply(&wire, generations[0], Ok("MUST NOT APPLY".to_owned()));
    let formatted = "SELECT\n  1;\n\nSELECT\n  2;";
    reply(&wire, generations[1], Ok(formatted.to_owned()));
    rt.frame(1.0 / 25.0);
    assert_eq!(rt.state.sql.doc(), formatted);
    assert_eq!(rt.state.sql.main_head(), formatted.rfind("SELECT").unwrap());

    custom(&mut rt);
    let stale = wire.borrow().requests.last().unwrap().generation;
    let b = rt.state.open_query_tab("Formatter B").unwrap();
    rt.frame(0.0);
    assert!(wire.borrow().cancelled.contains(&stale));
    reply(&wire, stale, Ok("MUST NOT HIT B".to_owned()));
    rt.frame(1.0 / 25.0);
    assert_eq!(rt.state.tabs.selected().unwrap().id, b);
    assert_eq!(rt.state.sql.doc(), "");
    assert!(rt.state.select_tab(a));
    rt.frame(0.0);
    assert_eq!(rt.state.sql.doc(), formatted, "no late editing of a parked draft");
    assert!(rt.state.query_result_tabs.is_empty());
}

#[test]
fn native_query_format_refuses_ime_substitution_setting_and_scope_changes() {
    let source = "select  2;";
    let (mut rt, wire, _) = mounted(source, json!({}), "Mod-Shift-f");
    tap(&mut rt, "sql-editor-scroll");
    stock_format(&mut rt);
    let old = wire.borrow().requests.last().unwrap().generation;
    assert!(app_ime(&mut rt.state, &Ime::Preedit("候".into(), None), &mut Tx::default()));
    reply(&wire, old, Ok("STALE".to_owned()));
    rt.frame(1.0 / 25.0);
    assert!(wire.borrow().cancelled.contains(&old));
    assert_eq!(rt.state.sql.composition(), Some("候"));
    rt.state.sql.preedit("");
    assert_eq!(rt.state.sql.doc(), source);
    assert!(rt.state.sql.undo_stack.is_empty());

    stock_format(&mut rt);
    let old = wire.borrow().requests.last().unwrap().generation;
    wire.borrow_mut().context.as_mut().unwrap().options.lines_between_queries = 3;
    reply(&wire, old, Ok("STALE SETTING".to_owned()));
    rt.frame(1.0 / 25.0);
    assert!(wire.borrow().cancelled.contains(&old));
    assert_eq!(rt.state.sql.doc(), source);

    stock_format(&mut rt);
    let old = wire.borrow().requests.last().unwrap().generation;
    // Even the same text/revision/caret on the same UI tab is not the same
    // SqlEditor. A format result must never edit its replacement.
    rt.state.sql = sql_editor::SqlEditor::from_doc(source);
    rt.state.sql.focus();
    reply(&wire, old, Ok("STALE IDENTITY".to_owned()));
    rt.frame(1.0 / 25.0);
    assert!(wire.borrow().cancelled.contains(&old));
    assert_eq!(rt.state.sql.doc(), source);

    tap(&mut rt, "sql-editor-scroll");
    stock_format(&mut rt);
    let old = wire.borrow().requests.last().unwrap().generation;
    rt.state.navigator_value_search_session_generation = Some(8);
    reply(&wire, old, Ok("STALE SESSION".to_owned()));
    rt.frame(1.0 / 25.0);
    assert!(wire.borrow().cancelled.contains(&old));
    assert_eq!(rt.state.sql.doc(), source);
    assert!(rt.state.sql.undo_stack.is_empty());
}

#[test]
fn native_query_format_unsupported_docks_noop_and_output_limit_do_not_edit() {
    let source = "select 1";
    let (mut rt, wire, _) = mounted(source, json!({}), "Mod-Shift-f");
    tap(&mut rt, "sql-find-open");
    stock_format(&mut rt);
    assert!(wire.borrow().requests.is_empty(), "focused search cannot format the editor");
    assert_eq!(rt.state.sql.doc(), source);

    tap(&mut rt, "sql-editor-scroll");
    for response in [Ok(source.to_owned()), Err(NativeSqlFormatError::Rejected),
                     Ok(" ".to_owned()), Ok("x".repeat(NATIVE_SQL_FORMAT_MAX_OUTPUT_BYTES + 1))] {
        stock_format(&mut rt);
        let live = wire.borrow().requests.last().unwrap().generation;
        reply(&wire, live, response);
        rt.frame(1.0 / 25.0);
        assert_eq!(rt.state.sql.doc(), source);
        assert!(rt.state.sql.undo_stack.is_empty());
    }
    let accepted = wire.borrow().requests.len();
    wire.borrow_mut().context = None;
    tap(&mut rt, "sql-format");
    assert_eq!(rt.state.query_notice.as_deref(), Some("Format is not available in this host"));
    assert_eq!(wire.borrow().requests.len(), accepted);
}

#[test]
fn native_query_format_explicit_printable_custom_binding_precedes_its_insertion_only_in_query() {
    let source = "select 2";
    let (mut rt, wire, _) = mounted(source, json!({}), "k");
    tap(&mut rt, "sql-editor-scroll");
    key(&mut rt, Key::Text("k".into()), Mods::default());
    let generation = wire.borrow().requests.last().unwrap().generation;
    assert_eq!(wire.borrow().requests.len(), 1);
    assert_eq!(rt.state.sql.doc(), source);
    reply(&wire, generation, Ok("SELECT\n  2".to_owned()));
    rt.frame(1.0 / 25.0);
    assert_eq!(rt.state.sql.doc(), "SELECT\n  2");

    wire.borrow_mut().context = None;
    key(&mut rt, Key::Text("k".into()), Mods::default());
    assert!(rt.state.sql.doc().starts_with("kSELECT"));
    assert_eq!(wire.borrow().requests.len(), 1);
}
