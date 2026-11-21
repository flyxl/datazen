//! Independent audit for T465: `NavigatorToolbar` port.
//!
//! Fill branch coverage the coder does not drive: exact structural heights,
//! the structural/observation keys, and partial host-port wiring.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::views::navigator::navigator_toolbar::{
    build, NavigatorToolbarHostPort, NavigatorToolbarVm, QueryEditFn, SEARCH_INPUT_KEY,
    SEARCH_ROW_HEIGHT, TITLE_ROW_HEIGHT,
};

#[derive(Default)]
struct P { query: rune::TextEdit, actions: Rc<RefCell<Vec<&'static str>>> }

fn view(p: &P, _cx: &mut Cx<P>) -> El {
    let mut layer = Vec::new();
    let edit: Option<QueryEditFn<P>> = Some(Rc::new(|s: &mut P| &mut s.query));
    // Wire only refresh and search-values: the "partial" branch.
    let a = p.actions.clone();
    let b = p.actions.clone();
    let host = NavigatorToolbarHostPort {
        on_refresh: Some(Rc::new(move |_s: &mut P| a.borrow_mut().push("refresh"))),
        on_search_values: Some(Rc::new(move |_s: &mut P| b.borrow_mut().push("search-values"))),
        ..Default::default()
    };
    let vm = NavigatorToolbarVm { search_query: rune::TextEdit::new(p.query.text.clone()) };
    build(&vm, &host, edit, true, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime() -> Runtime<P> {
    Runtime::new(P::default(), view, (1440.0, 816.0),
        Env { system_dark: true, scale: 1.0 })
}

#[test]
fn structural_heights_match_the_react_literals() {
    // React's title row is `h-12 min-h-[48px]`; the search row is py-1 + h-6.
    assert_eq!(TITLE_ROW_HEIGHT, 48.0);
    assert_eq!(SEARCH_ROW_HEIGHT, 32.0);
}

#[test]
fn structural_rows_and_icons_are_composed() {
    let mut rt = runtime();
    rt.frame(0.0);
    for key in [
        "navigator-title-row",
        "navigator-title-row-content",
        "navigator-title-row-border",
        "navigator-toolbar-buttons",
        "navigator-title",
        "navigator-search-row",
        "navigator-search-input-wrap",
        "navigator-search-icon",
        SEARCH_INPUT_KEY,
    ] {
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(key)), "{key} composed");
    }
    let height = rt.elements().into_iter()
        .find(|e| e.key.as_deref() == Some("navigator-title-row"))
        .expect("title row laid out").rect[3];
    assert!((height - 48.0).abs() < 0.5, "title row is 48pt tall, got {height}");
}

#[test]
fn partial_host_wiring_mounts_only_the_wired_optional_buttons() {
    let mut rt = runtime();
    rt.frame(0.0);
    // Required buttons always mount.
    for key in ["new-connection-button", "navigator-new-group", "navigator-collapse-all"] {
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(key)), "{key} required");
    }
    // Only refresh + search-values were wired.
    for key in ["navigator-refresh", "navigator-search-values"] {
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(key)), "{key} wired");
    }
    for key in ["navigator-export", "navigator-import", "navigator-collapse-sidebar"] {
        assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some(key)), "{key} unwired");
    }
    for key in ["navigator-refresh", "navigator-search-values"] {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
    }
    assert_eq!(rt.state.actions.borrow().as_slice(), ["refresh", "search-values"]);
}
