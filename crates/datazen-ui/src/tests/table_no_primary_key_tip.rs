//! T544: `TableNoPrimaryKeyTip` — status message, optional SQL action and
//! rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use crate::views::table_no_primary_key_tip::{
    build, NoPkAction, NoPkTipAction, NoPkTipActionFn, NoPrimaryKeyTipVm, ACTION_KEY, ROOT_KEY,
    TEXT_KEY,
};

#[derive(Clone)]
struct Probe {
    vm: NoPrimaryKeyTipVm,
    edits: Rc<RefCell<usize>>,
    dark: bool,
}

fn vm() -> NoPrimaryKeyTipVm {
    NoPrimaryKeyTipVm {
        message: "This table has no primary key".to_owned(),
        action: None,
    }
}

fn view(p: &Probe, _cx: &mut rune::Cx<Probe>) -> rune::El {
    let mut layer = Vec::new();
    let edits = p.edits.clone();
    let on_action: Option<NoPkTipActionFn<Probe>> =
        Some(Rc::new(move |_s, action| {
            if action == NoPkTipAction::EditInSql {
                *edits.borrow_mut() += 1;
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
fn status_role_and_message() {
    let rt = runtime(
        Probe { vm: vm(), edits: Rc::new(RefCell::new(0)), dark: true },
        (960.0, 640.0),
    );
    assert_eq!(element(&rt, ROOT_KEY).role, "status");
    assert_eq!(text(&rt, TEXT_KEY), "This table has no primary key");
    assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some(ACTION_KEY)), "no action prop -> no button");
}

#[test]
fn optional_sql_action_emits() {
    let mut state = vm();
    state.action = Some(NoPkAction { label: "Edit in SQL".to_owned() });
    let mut rt = runtime(
        Probe { vm: state, edits: Rc::new(RefCell::new(0)), dark: false },
        (1440.0, 816.0),
    );
    assert_eq!(element(&rt, ACTION_KEY).label.as_deref(), Some("Edit in SQL"));
    let (x, y) = rt.find(ACTION_KEY).unwrap_or_else(|| panic!("action laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert_eq!(*rt.state.edits.borrow(), 1);
}

#[test]
fn geometry_both_sizes_both_palettes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut state = vm();
            state.action = Some(NoPkAction { label: "Edit in SQL".to_owned() });
            let rt = runtime(
                Probe { vm: state, edits: Rc::new(RefCell::new(0)), dark },
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
