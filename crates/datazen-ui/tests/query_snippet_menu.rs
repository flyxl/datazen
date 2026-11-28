use datazen_ui::test_support::*;
use rune::{Key, KeyEvent, Mods};

const TRIGGER: &str = "query-library-snippets-toggle";
const POPUP: &str = "query-snippets-popup";
const EDITOR: &str = "sql-editor-scroll";

fn click(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.settle_all();
    rt.frame(0.0);
}
fn key(rt: &mut Runtime<AppState>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    rt.settle_all();
    rt.frame(0.0);
}
fn focused(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements().iter().any(|e| !e.exiting && e.focused && e.key.as_deref() == Some(key))
}

#[test]
fn real_query_toolbar_snippets_use_current_draft_and_literal_store_without_running() {
    let mut rt = rune::App::new(rune::AppSpec {
        on_key: Some(datazen_ui::app_key),
        poll: Some(datazen_ui::app_poll),
        ..rune::AppSpec::new("Snippets gate", (1440.0, 900.0), AppState::default, view)
    }).on_ime(datazen_ui::app_ime)
        .headless_runtime((1440.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.state.workspace.connect("local").unwrap();
    let owner = rt.state.open_query_tab("Current draft").unwrap();
    let saved = "SELECT '${1:literal} 中文';";
    rt.state.workspace.save_snippet("query.run", saved).unwrap();
    let previous = "old β";
    rt.state.sql = sql_editor::SqlEditor::from_doc(previous);
    rt.state.sql.set_main_selection(0, previous.len());
    rt.frame(0.0);
    click(&mut rt, "query-library-history-toggle");
    let canvas = rt.presented(EDITOR).unwrap().0;

    click(&mut rt, TRIGGER);
    assert!(rt.find("query-history-panel").is_some(), "the real drawer is untouched");
    assert_eq!(rt.presented(EDITOR).unwrap().0, canvas, "the menu is out of flow");
    assert!(focused(&rt, "snippet-select-all"), "the real first item receives focus");
    let popup = rt.presented(POPUP).unwrap().0;
    assert!(popup[2] > 200.0 && popup[3] >= 7.0 * 32.0);
    let literal = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("snippet-saved-0")).unwrap();
    assert_eq!(literal.label.as_deref(), Some("query.run"), "literal user name, not i18n key");

    key(&mut rt, Key::Enter, Mods { cmd: true, ..Mods::default() });
    key(&mut rt, Key::Text("Q".to_owned()), Mods::default());
    rt.ime(rune::Ime::Commit("未提交".to_owned()));
    rt.paste("never pasted at this menu");
    rt.frame(0.0);
    assert_eq!(rt.state.sql.doc(), previous);
    assert!(rt.find(POPUP).is_some());
    assert!(rt.state.query_result_tabs.is_empty());
    key(&mut rt, Key::Escape, Mods::default());
    assert!(rt.find(POPUP).is_none());
    assert!(focused(&rt, TRIGGER));

    key(&mut rt, Key::Text(" ".into()), Mods::default());
    assert!(rt.find(POPUP).is_some(), "focused real toolbar trigger can reopen by keyboard");
    key(&mut rt, Key::Down, Mods::default());
    assert!(focused(&rt, "snippet-select-columns"));
    key(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.sql.doc(), "SELECT columns\nFROM table_name;");
    assert_eq!(rt.state.sql.selected_text(), "table_name");
    assert!(focused(&rt, EDITOR));
    key(&mut rt, Key::Command('z'), Mods { cmd: true, ..Mods::default() });
    assert_eq!(rt.state.sql.doc(), previous);
    assert_eq!(rt.state.sql.selected_text(), previous, "one undo restores the original selection");

    click(&mut rt, TRIGGER);
    click(&mut rt, "snippet-saved-0");
    assert_eq!(rt.state.sql.doc(), saved);
    assert!(focused(&rt, EDITOR));
    key(&mut rt, Key::Command('z'), Mods { cmd: true, ..Mods::default() });
    assert_eq!(rt.state.sql.doc(), previous);
    assert_eq!(rt.state.tabs.selected().map(|tab| tab.id), Some(owner));
    assert_eq!(rt.state.host_snippets()[0].sql, saved);
    assert!(rt.state.query_result_tabs.is_empty());
}
