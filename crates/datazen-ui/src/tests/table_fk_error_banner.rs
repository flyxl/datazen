//! T544: `TableFkErrorBanner` — classified/unknown message composition,
//! dismiss action and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use crate::i18n::t;
use crate::views::table_fk_error_banner::{
    build, FkBannerAction, FkBannerActionFn, FkErrorVm, TableFkErrorBannerVm, CLOSE_KEY, ROOT_KEY,
    TEXT_KEY,
};

#[derive(Clone)]
struct Probe {
    vm: TableFkErrorBannerVm,
    dismiss: Rc<RefCell<usize>>,
    dark: bool,
}

fn error(code: &str, raw: &str) -> TableFkErrorBannerVm {
    TableFkErrorBannerVm {
        error: Some(FkErrorVm { code: code.to_owned(), raw: raw.to_owned() }),
    }
}

fn element(rt: &rune::Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &rune::Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text(rt: &rune::Runtime<Probe>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

fn view(p: &Probe, _cx: &mut rune::Cx<Probe>) -> rune::El {
    let mut layer = Vec::new();
    let dismiss = p.dismiss.clone();
    let on_action: Option<FkBannerActionFn<Probe>> =
        Some(Rc::new(move |_s, action| {
            if action == FkBannerAction::Dismiss {
                *dismiss.borrow_mut() += 1;
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

#[test]
fn message_composition_matches_react() {
    assert_eq!(TableFkErrorBannerVm::default().message(), None, "null error shows nothing");
    let unknown = error("unknown", "raw-only");
    assert_eq!(unknown.message().as_deref(), Some("raw-only"));
    let unclassified = error("grid.other", "bare");
    assert_eq!(unclassified.message().as_deref(), Some("bare"));
    let classified = error("grid.fk.unknownConstraint", "raw detail");
    let expected = format!("{}\nraw detail", t("fkNav.error.unknownConstraint"));
    assert_eq!(classified.message().as_deref(), Some(expected.as_str()));
}

#[test]
fn banner_renders_message_and_dismisses() {
    let vm = error("grid.fk.unknownConstraint", "raw detail");
    let rt = runtime(
        Probe { vm, dismiss: Rc::new(RefCell::new(0)), dark: true },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, ROOT_KEY));
    assert!(has_key(&rt, TEXT_KEY));
    assert_eq!(element(&rt, CLOSE_KEY).label.as_deref(), Some(t("common.close")));
    assert!(text(&rt, "table-fk-error-text-text").contains("raw detail"));

    let mut rt = rt;
    let (x, y) = rt.find(CLOSE_KEY).unwrap_or_else(|| panic!("close laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert_eq!(*rt.state.dismiss.borrow(), 1);
}

#[test]
fn null_error_and_geometry() {
    let rt = runtime(
        Probe { vm: TableFkErrorBannerVm::default(), dismiss: Rc::new(RefCell::new(0)), dark: false },
        (1440.0, 816.0),
    );
    assert!(!has_key(&rt, ROOT_KEY));

    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let rt = runtime(
                Probe { vm: error("grid.fk.valueNull", "a long raw detail message"), dismiss: Rc::new(RefCell::new(0)), dark },
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
