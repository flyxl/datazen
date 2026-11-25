//! Independent audit for T400: `ForeignKeysView` port.
//!
//! Fill branches the coder does not drive: no host callbacks wired, the exact
//! six React header titles, and a dense FK list at both window sizes/palettes.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};
use datazen_ui::i18n::t;
use datazen_ui::relation_metadata::{Deferrability, ForeignKeyInfo};
use datazen_ui::views::foreign_keys_view::{build, FkAction, FkIdentity, FkViewState, ROOT_KEY, TABLE_KEY};

#[derive(Default)]
struct Probe { state: FkViewState, dark: bool }

fn identity() -> FkIdentity {
    FkIdentity {
        db_session_id: "s".to_owned(),
        table_name: "orders".to_owned(),
        database: "db".to_owned(),
    }
}

fn fk(name: &str) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.to_owned(),
        columns: vec!["user_id".to_owned(), "tenant_id".to_owned()],
        referenced_table: format!("{name}_ref"),
        referenced_columns: vec!["id".to_owned()],
        on_update: Some("CASCADE".to_owned()),
        on_delete: Some("RESTRICT".to_owned()),
        deferrability: Some(Deferrability::Unknown),
    }
}

fn view(p: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    // Deliberately no on_action / on_open_table — the optional-port branches.
    let no_action: Option<Rc<dyn Fn(&mut Probe, FkAction) + 'static>> = None;
    let no_open: Option<Rc<dyn Fn(&mut Probe, &str, &str) + 'static>> = None;
    build::<Probe>(&identity(), &p.state, no_action, no_open, p.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    Runtime::new(p, view, size, Env { system_dark: dark, scale: 1.0 })
}

#[test]
fn view_composes_without_any_host_callbacks() {
    let mut state = FkViewState::default();
    state.foreign_keys = vec![fk("fk_one")];
    let mut rt = runtime(Probe { state, dark: false }, (1440.0, 816.0));
    rt.frame(0.0);
    for key in [ROOT_KEY, TABLE_KEY, "foreign-keys-row-fk_one", "foreign-keys-ref-table-fk_one"] {
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(key)), "{key} composed");
    }
    // Pressing the referenced-table row is inert with no navigation callback.
    let (x, y) = rt.find("foreign-keys-ref-table-fk_one").expect("referenced table cell");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert_eq!(rt.state.state.foreign_keys.len(), 1, "no callbacks means no state change");
}

#[test]
fn header_titles_are_the_exact_six_react_labels() {
    let mut state = FkViewState::default();
    state.foreign_keys = vec![fk("fk_one")];
    let rt = runtime(Probe { state, dark: true }, (1440.0, 816.0));
    let mut rt = rt;
    rt.frame(0.0);
    let expected = [
        t("fk.constraintName"),
        t("fk.localColumn"),
        t("fk.refTable"),
        t("fk.refColumn"),
        "ON UPDATE",
        "ON DELETE",
    ];
    for (index, title) in expected.iter().enumerate() {
        let el = rt.elements().into_iter()
            .find(|e| e.key.as_deref() == Some(&format!("foreign-keys-head-{index}")))
            .expect("header cell is laid out");
        assert_eq!(el.text.as_deref(), Some(*title), "header {index} matches React");
    }
}

#[test]
fn a_dense_fk_list_stays_within_both_windows() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut state = FkViewState::default();
            state.foreign_keys = (0..8).map(|i| fk(&format!("fk_{i}"))).collect();
            let mut rt = runtime(Probe { state, dark }, (width, height));
            rt.frame(0.0);
            assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(ROOT_KEY)));
            for element in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, _, w, _] = element.rect;
                assert!(
                    x >= -0.5 && x + w <= width + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} overflows horizontally",
                    element.key,
                    element.rect
                );
            }
        }
    }
}

#[test]
fn error_state_does_not_leak_stale_rows() {
    let mut state = FkViewState::default();
    state.foreign_keys = vec![fk("stale")];
    state.apply_load(Err("disk gone".to_owned()));
    let mut rt = runtime(Probe { state, dark: true }, (960.0, 640.0));
    rt.frame(0.0);
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("foreign-keys-error-text")));
    assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some("foreign-keys-row-stale")));
}
