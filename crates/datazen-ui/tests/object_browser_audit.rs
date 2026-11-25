//! Independent audit for T480: `ObjectBrowser` port.
//!
//! Fill branch coverage the coder does not drive: DDL-fetch and execute answer
//! messages, the running flag gating, and menu teardown.

use rune::{PointerPress, TextEdit};
use datazen_ui::i18n::t;
use datazen_ui::views::object_browser::{
    BrowserObject, ObjectBrowserAction, ObjectBrowserVm,
};

fn sample() -> BrowserObject {
    BrowserObject {
        kind: datazen_ui::views::object_browser::BrowserKind::Function,
        schema: Some("public".to_owned()),
        name: "lookup".to_owned(),
        ..Default::default()
    }
}

fn press() -> PointerPress {
    // A zero-size anchor is sufficient: these helpers only store/clear it.
    PointerPress { x: 0.0, y: 0.0 }
}

#[test]
fn ddl_and_execute_answers_follow_the_react_messages() {
    let mut vm = ObjectBrowserVm::default();
    vm.apply_ddl(Ok("CREATE FUNCTION lookup()".to_owned()));
    assert_eq!(vm.ddl.text, "CREATE FUNCTION lookup()");

    vm.apply_ddl(Err("no ddl".to_owned()));
    assert_eq!(vm.ddl.text, "-- no ddl", "a failed DDL fetch is shown as a SQL comment");

    vm.apply_execute(Ok(()));
    assert!(!vm.running);
    assert_eq!(vm.run_message, t("objects.executeOk"));
    vm.apply_execute(Err("exec failed".to_owned()));
    assert_eq!(vm.run_message, "exec failed", "a concrete failure is preserved verbatim");
}

#[test]
fn running_gates_execute_independently_of_ddl() {
    let mut vm = ObjectBrowserVm::default();
    vm.ddl = TextEdit::new("SELECT 1".to_owned());
    assert!(!vm.execute_disabled(), "non-empty DDL is executable");

    vm.reduce(&ObjectBrowserAction::Execute {
        sql: vm.ddl.text.clone(),
        schema: None,
    });
    assert!(vm.running, "Execute marks the run in flight");
    assert!(vm.run_message.is_empty(), "Execute clears the prior message");
    assert!(vm.execute_disabled(), "a running query cannot be re-submitted");

    vm.apply_execute(Ok(()));
    assert!(!vm.execute_disabled(), "the answer re-enables execute");
}

#[test]
fn close_menu_clears_both_anchors_and_the_menu_object() {
    let mut vm = ObjectBrowserVm::default();
    vm.reduce(&ObjectBrowserAction::ListContextMenu {
        object: sample(),
        press: press(),
    });
    assert!(vm.list_menu.is_some() && vm.menu_object.is_some());
    vm.reduce(&ObjectBrowserAction::EditorContextMenu { press: press() });
    assert!(vm.editor_menu.is_some());

    vm.reduce(&ObjectBrowserAction::CloseMenu);
    assert!(vm.list_menu.is_none() && vm.menu_object.is_none() && vm.editor_menu.is_none());
}

#[test]
fn selection_clears_the_prior_run_message() {
    let mut vm = ObjectBrowserVm::default();
    vm.run_message = "stale".to_owned();
    vm.select_object(&sample());
    assert!(vm.run_message.is_empty());
    assert_eq!(vm.selection_label(), "function · lookup");
}
