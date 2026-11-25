//! T400: `ForeignKeysView` port — real branch tests.
//!
//! Pinned values mirror the React source's four states and the i18n keys; the
//! geometry probe reads runtime rects at both window sizes and palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, ScrollSpec};

use datazen_ui::i18n::t;
use datazen_ui::relation_metadata::{Deferrability, ForeignKeyInfo};
use datazen_ui::views::foreign_keys_view::{
    build, FkAction, FkIdentity, FkViewState, EMPTY_KEY, ERROR_KEY, HEADER_KEY, LOADING_KEY, ROOT_KEY,
    TABLE_KEY,
};

#[derive(Default)]
struct Probe {
    state: FkViewState,
    dark: bool,
    calls: Rc<RefCell<Vec<String>>>,
}

fn identity() -> FkIdentity {
    FkIdentity {
        db_session_id: "sess-1".to_owned(),
        table_name: "orders".to_owned(),
        database: "db_a".to_owned(),
    }
}

fn fk(name: &str) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.to_owned(),
        columns: vec!["user_id".to_owned(), "tenant_id".to_owned()],
        referenced_table: "users".to_owned(),
        referenced_columns: vec!["id".to_owned()],
        on_update: Some("CASCADE".to_owned()),
        on_delete: Some("RESTRICT".to_owned()),
        deferrability: Some(Deferrability::Unknown),
    }
}

fn view(probe: &Probe, _cx: &mut Cx<Probe>) -> El {
    let calls = probe.calls.clone();
    let on_action = Rc::new(move |_s: &mut Probe, action: FkAction| {
        calls.borrow_mut().push(format!("action:{action:?}"));
    });
    let calls = probe.calls.clone();
    let on_open = Rc::new(move |_s: &mut Probe, table: &str, database: &str| {
        calls.borrow_mut().push(format!("open:{table}:{database}"));
    });
    let mut layer = Vec::new();
    build(&identity(), &probe.state, Some(on_action), Some(on_open), probe.dark, &mut layer);
    col()
        .w_full()
        .h_full()
        .scroll(ScrollSpec { stick_bottom: false })
        .children(layer)
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    Runtime::new(probe, view, size, Env { system_dark: dark, scale: 1.0 })
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|element| element.key.as_deref() == Some(key))
}

fn has_text(rt: &Runtime<Probe>, needle: &str) -> bool {
    rt.elements().iter().filter_map(|e| e.text.clone()).any(|text| text.contains(needle))
}

fn settle(rt: &mut Runtime<Probe>) {
    rt.invalidate();
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn begin_load_and_apply_load_pin_the_hook_transitions() {
    let mut state = FkViewState {
        foreign_keys: vec![fk("old")],
        loading: false,
        error: "old error".to_owned(),
    };
    state.begin_load();
    assert!(state.loading);
    assert!(state.error.is_empty());
    assert!(state.foreign_keys.is_empty());

    state.apply_load(Ok(vec![fk("fk_orders_user")]));
    assert!(!state.loading);
    assert_eq!(state.foreign_keys.len(), 1);
    assert_eq!(state.foreign_keys[0].name, "fk_orders_user");

    state.apply_load(Err(String::new()));
    assert!(!state.loading);
    assert_eq!(state.error, t("fk.loadFailed"), "empty error uses the translated fallback");
    assert!(state.foreign_keys.is_empty());

    state.apply_load(Err("disk gone".to_owned()));
    assert_eq!(state.error, "disk gone", "a concrete message is preserved verbatim");
}

#[test]
fn loading_error_and_empty_states_are_exclusive() {
    let mut probe = Probe::default();
    probe.state.loading = true;
    let mut rt = runtime(probe, (960.0, 640.0));
    rt.frame(0.0);
    assert!(has_key(&rt, LOADING_KEY));
    assert!(has_key(&rt, "foreign-keys-loading-text"));
    assert!(has_text(&rt, &t("fk.loading")));
    assert!(!has_key(&rt, ROOT_KEY));

    rt.state.state.loading = false;
    rt.state.state.error = "broken".to_owned();
    settle(&mut rt);
    assert!(has_key(&rt, ERROR_KEY));
    assert!(has_text(&rt, "broken"));
    assert!(!has_key(&rt, LOADING_KEY));

    rt.state.state.error.clear();
    settle(&mut rt);
    assert!(has_key(&rt, EMPTY_KEY));
    assert!(has_text(&rt, &t("fk.noForeignKeys")));
    assert!(!has_key(&rt, ROOT_KEY));
}

#[test]
fn error_state_uses_copyable_error_with_an_alert_role() {
    let mut probe = Probe::default();
    probe.state.error = "disk gone".to_owned();
    let mut rt = runtime(probe, (960.0, 640.0));
    rt.frame(0.0);
    let error = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(ERROR_KEY))
        .expect("error branch mounted");
    assert_eq!(
        error.role, "alert",
        "React's CopyableError message carries role=alert; left: {:?} right: \"alert\"",
        error.role
    );
    assert!(
        rt.elements().iter().any(|element| {
            element.key.as_deref() == Some("foreign-keys-error-text")
                && element.text.as_deref() == Some("disk gone")
        }),
        "the exact error message is preserved on the selectable text node"
    );
}

#[test]
fn header_and_fk_rows_are_composed_with_local_and_reference_chips() {
    let mut probe = Probe::default();
    probe.state.foreign_keys = vec![fk("fk_orders_user"), fk("fk_orders_tenant")];
    let mut rt = runtime(probe, (1440.0, 816.0));
    rt.frame(0.0);

    for key in [
        ROOT_KEY,
        HEADER_KEY,
        "foreign-keys-title",
        "foreign-keys-count",
        TABLE_KEY,
        "foreign-keys-head",
        "foreign-keys-head-0",
        "foreign-keys-head-5",
        "foreign-keys-row-fk_orders_user",
        "foreign-keys-row-fk_orders_tenant",
        "foreign-keys-name-fk_orders_user",
        "foreign-keys-local-fk_orders_user-0",
        "foreign-keys-local-fk_orders_user-1",
        "foreign-keys-ref-table-fk_orders_user",
        "foreign-keys-ref-cols-fk_orders_user-0",
        "foreign-keys-on-update-fk_orders_user",
        "foreign-keys-on-delete-fk_orders_user",
    ] {
        assert!(has_key(&rt, key), "composed key missing: {key}");
    }
    assert!(has_text(&rt, "orders"), "table name in header");
    assert!(has_text(&rt, "2 个外键"), "the i18n count is rendered");
    assert!(has_text(&rt, &t("fk.constraintName")));
    assert!(has_text(&rt, &t("fk.localColumn")));
    assert!(has_text(&rt, &t("fk.refTable")));
    assert!(has_text(&rt, &t("fk.refColumn")));
    assert!(has_text(&rt, "ON UPDATE"));
    assert!(has_text(&rt, "user_id"));
    assert!(has_text(&rt, "tenant_id"));
    assert!(has_text(&rt, "CASCADE"));
    assert!(has_text(&rt, "RESTRICT"));
}

#[test]
fn navigation_callback_carries_referenced_table_and_database() {
    let mut probe = Probe::default();
    probe.state.foreign_keys = vec![fk("fk_orders_user")];
    let mut rt = runtime(probe, (1440.0, 816.0));
    click(&mut rt, "foreign-keys-ref-table-fk_orders_user");
    assert_eq!(
        rt.state.calls.borrow().as_slice(),
        ["open:users:db_a"],
        "the referenced table and the panel database are delivered verbatim"
    );
}

#[test]
fn diagram_fits_both_windows_in_both_themes() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = Probe::default();
            probe.dark = dark;
            probe.state.foreign_keys = vec![fk("fk_orders_user"), fk("fk_orders_tenant")];
            let mut rt = runtime(probe, (width, height));
            rt.frame(0.0);
            assert!(has_key(&rt, ROOT_KEY), "view mounted at {width}x{height} dark={dark}");
            for element in rt.elements() {
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
