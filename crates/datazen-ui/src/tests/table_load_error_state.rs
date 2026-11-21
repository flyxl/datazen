//! T544: `TableLoadErrorState` — error text, retry action and rect geometry
//! at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use crate::views::table_load_error_state::{
    build, LoadErrorAction, LoadErrorActionFn, TableLoadErrorVm, RETRY_KEY, ROOT_KEY, TEXT_KEY,
};

#[derive(Clone)]
struct Probe {
    vm: TableLoadErrorVm,
    retries: Rc<RefCell<usize>>,
    dark: bool,
}

fn vm() -> TableLoadErrorVm {
    TableLoadErrorVm {
        error: "failed to load table".to_owned(),
        retry_label: "Retry".to_owned(),
    }
}

fn view(p: &Probe, _cx: &mut rune::Cx<Probe>) -> rune::El {
    let mut layer = Vec::new();
    let retries = p.retries.clone();
    let on_action: Option<LoadErrorActionFn<Probe>> =
        Some(Rc::new(move |_s, action| {
            if action == LoadErrorAction::Retry {
                *retries.borrow_mut() += 1;
            }
        }));
    build(&p.vm, &on_action, p.dark, &mut layer);
    rune::col().w_full().h_full().children(layer)
}

fn runtime(p: Probe, size: (f32, f32)) -> rune::Runtime<Probe> {
    let dark = p.dark;
    let mut rt = rune::Runtime::new(p, view, size, rune::Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &rune::Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn text(rt: &rune::Runtime<Probe>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

#[test]
fn renders_error_text_and_retry_label() {
    let rt = runtime(
        Probe { vm: vm(), retries: Rc::new(RefCell::new(0)), dark: true },
        (960.0, 640.0),
    );
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(ROOT_KEY)));
    assert!(text(&rt, "table-load-error-text-text").contains("failed to load table"));
    assert_eq!(element(&rt, RETRY_KEY).label.as_deref(), Some("Retry"));
}

#[test]
fn retry_button_emits_action() {
    let mut rt = runtime(
        Probe { vm: vm(), retries: Rc::new(RefCell::new(0)), dark: false },
        (960.0, 640.0),
    );
    let (x, y) = rt.find(RETRY_KEY).unwrap_or_else(|| panic!("retry laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert_eq!(*rt.state.retries.borrow(), 1);
}

#[test]
fn geometry_both_sizes_both_palettes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let rt = runtime(
                Probe { vm: vm(), retries: Rc::new(RefCell::new(0)), dark },
                (w, h),
            );
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "{w}x{h} dark={dark}: {:?} horizontal", el.key);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "{w}x{h} dark={dark}: {:?} vertical", el.key);
            }
        }
    }
}
