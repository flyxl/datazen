//! T548: `TableStatusBanners` — the three React banner states, the Retry host
//! intent, and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::table_status_banners::{
    build, TableStatusBannersAction, TableStatusBannersVm,
};

#[derive(Clone)]
struct Scene {
    vm: TableStatusBannersVm,
    actions: Rc<RefCell<Vec<TableStatusBannersAction>>>,
    dark: bool,
}

fn scene(vm: TableStatusBannersVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |_state: &mut Scene, action| {
        actions.borrow_mut().push(action);
    });
    col().w_full().h_full().child(build(&s.vm, Some(callback), s.dark))
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn no_error_and_no_notice_renders_nothing() {
    let rt = runtime(scene(TableStatusBannersVm::default(), true), (1440.0, 816.0));
    assert!(!has_key(&rt, "table-status-error"));
    assert!(!has_key(&rt, "table-status-pending-hint"));
    assert!(!has_key(&rt, "table-insert-degraded"));
}

#[test]
fn error_banner_shows_copyable_message_and_retry() {
    let vm = TableStatusBannersVm {
        error: Some("syntax error near filter clause".to_owned()),
        ..Default::default()
    };
    let mut rt = runtime(scene(vm, true), (1440.0, 816.0));
    for key in ["table-status-error", "table-status-error-copy", "table-status-error-copy-text", "table-status-retry"] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert!(texts(&rt).iter().any(|x| x == "syntax error near filter clause"));
    click(&mut rt, "table-status-retry");
    assert_eq!(rt.state.actions.borrow().as_slice(), &[TableStatusBannersAction::Retry]);
}

#[test]
fn pending_hint_needs_both_an_error_and_staged_work() {
    let vm = TableStatusBannersVm {
        error: Some("boom".to_owned()),
        has_work: true,
        ..Default::default()
    };
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    assert!(has_key(&rt, "table-status-pending-hint"));
    assert!(texts(&rt).iter().any(|x| x == t("tableData.errorPendingHint")));

    let vm = TableStatusBannersVm {
        error: Some("boom".to_owned()),
        has_work: false,
        ..Default::default()
    };
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    assert!(!has_key(&rt, "table-status-pending-hint"), "no staged work: no hint");

    let vm = TableStatusBannersVm {
        has_work: true,
        ..Default::default()
    };
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    assert!(!has_key(&rt, "table-status-pending-hint"), "no error: no hint");
}

#[test]
fn insert_notice_is_its_own_live_region() {
    let vm = TableStatusBannersVm {
        insert_notice: Some("the new row could not be located".to_owned()),
        ..Default::default()
    };
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    let notice = rt.elements().into_iter()
        .find(|e| e.key.as_deref() == Some("table-insert-degraded"))
        .unwrap_or_else(|| panic!("insert notice is laid out"));
    assert_eq!(notice.role, "status");
    assert!(texts(&rt).iter().any(|x| x == "the new row could not be located"));
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let vm = TableStatusBannersVm {
                error: Some("syntax error near filter clause".to_owned()),
                has_work: true,
                insert_notice: Some("the new row could not be located".to_owned()),
            };
            let rt = runtime(scene(vm, dark), size);
            let outside: Vec<String> = rt.elements().iter()
                .filter(|e| {
                    let r = e.rect;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                .collect();
            assert!(outside.is_empty(), "{size:?} dark={dark}: outside {outside:?}");
        }
    }
}

/// Non-capturing view for the "no host port" branch (Runtime needs an fn pointer).
fn no_port_view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(build::<Scene>(&s.vm, None, s.dark))
}

#[test]
fn empty_error_string_mounts_no_banner() {
    let vm = TableStatusBannersVm {
        error: Some(String::new()),
        has_work: true,
        ..Default::default()
    };
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    assert!(!has_key(&rt, "table-status-error"), "an empty error is falsy like React");
    assert!(!has_key(&rt, "table-status-pending-hint"), "no error: no pending hint");
}

#[test]
fn retry_without_a_host_port_is_inert() {
    let vm = TableStatusBannersVm {
        error: Some("boom".to_owned()),
        ..Default::default()
    };
    let mut rt = Runtime::new(
        scene(vm, true),
        no_port_view,
        (960.0, 640.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    click(&mut rt, "table-status-retry");
    assert!(rt.state.actions.borrow().is_empty(), "no injected port: no intents emitted");
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Showing the pending hint without an error made `pending_hint_needs_both_an_error_and_staged_work`
//    fail on `no error: no hint`.
// 2. Wiring Retry to nothing made `error_banner_shows_copyable_message_and_retry`
//    fail on `[Retry]`.

#[test]
fn empty_insert_notice_mounts_no_banner() {
    let vm = TableStatusBannersVm {
        insert_notice: Some(String::new()),
        ..Default::default()
    };
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    assert!(
        !has_key(&rt, "table-insert-degraded"),
        "React's insertNotice && (...) treats an empty string as falsy"
    );
    // Whitespace-only is truthy in React and must still mount.
    let vm = TableStatusBannersVm {
        insert_notice: Some("   ".to_owned()),
        ..Default::default()
    };
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    assert!(has_key(&rt, "table-insert-degraded"), "whitespace is truthy");
}

#[test]
fn long_unbroken_insert_notice_wraps_inside_the_window() {
    let notice = "superlongunbreakabletoken".repeat(24);
    let vm = TableStatusBannersVm {
        insert_notice: Some(notice),
        ..Default::default()
    };
    let size = (960.0, 640.0);
    let rt = runtime(scene(vm, true), size);
    let outside: Vec<String> = rt.elements().iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "a long unbroken notice must wrap: {outside:?}");
}

// Independent lane-C repro probes (original D1/D2), retained as regression guards.
#[test]
fn probe_empty_insert_notice_does_not_render_a_banner() {
    let vm = TableStatusBannersVm {
        insert_notice: Some(String::new()),
        ..Default::default()
    };
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    assert!(
        !has_key(&rt, "table-insert-degraded"),
        "React's insertNotice && (...) treats an empty string as falsy"
    );
}

#[test]
fn probe_long_error_and_notice_stay_inside_both_windows() {
    let long = "verylongunbrokenerrortoken".repeat(24);
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let vm = TableStatusBannersVm {
                error: Some(long.clone()),
                has_work: true,
                insert_notice: Some(long.clone()),
            };
            let rt = runtime(scene(vm, dark), size);
            let outside: Vec<String> = rt.elements().iter()
                .filter(|e| {
                    let r = e.rect;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                .collect();
            assert!(outside.is_empty(), "{size:?} dark={dark}: outside {outside:?}");
        }
    }
}
