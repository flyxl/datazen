//! Independent audit for T544: FK error banner, load-error state and no-PK tip.
//!
//! Cover message-composition edges the coder does not drive and pin the React
//! contracts for the copyable error controls and the empty-message guard.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::table_fk_error_banner::{
    build as fk_build, FkBannerAction, FkBannerActionFn, FkErrorVm, TableFkErrorBannerVm,
    ROOT_KEY as FK_ROOT_KEY,
};
use datazen_ui::views::table_load_error_state::{
    build as load_build, LoadErrorAction, LoadErrorActionFn, TableLoadErrorVm,
    ROOT_KEY as LOAD_ROOT_KEY,
};
use datazen_ui::views::table_no_primary_key_tip::{
    build as tip_build, NoPkAction, NoPkTipAction, NoPkTipActionFn, NoPrimaryKeyTipVm,
    ACTION_KEY as TIP_ACTION_KEY, ROOT_KEY as TIP_ROOT_KEY, TEXT_KEY as TIP_TEXT_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct FkScene {
    vm: TableFkErrorBannerVm,
    dismiss: Rc<RefCell<usize>>,
    dark: bool,
}

#[derive(Clone)]
struct LoadScene {
    vm: TableLoadErrorVm,
    retries: Rc<RefCell<usize>>,
    dark: bool,
}

#[derive(Clone)]
struct TipScene {
    vm: NoPrimaryKeyTipVm,
    edits: Rc<RefCell<usize>>,
    dark: bool,
}

fn fk_view(s: &FkScene, _cx: &mut Cx<FkScene>) -> El {
    let mut layer = Vec::new();
    let dismiss = s.dismiss.clone();
    let on_action: Option<FkBannerActionFn<FkScene>> =
        Some(Rc::new(move |_s, action| if action == FkBannerAction::Dismiss { *dismiss.borrow_mut() += 1 }));
    fk_build(&s.vm, &on_action, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn load_view(s: &LoadScene, _cx: &mut Cx<LoadScene>) -> El {
    let mut layer = Vec::new();
    let retries = s.retries.clone();
    let on_action: Option<LoadErrorActionFn<LoadScene>> =
        Some(Rc::new(move |_s, action| if action == LoadErrorAction::Retry { *retries.borrow_mut() += 1 }));
    load_build(&s.vm, &on_action, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn tip_view(s: &TipScene, _cx: &mut Cx<TipScene>) -> El {
    let mut layer = Vec::new();
    let edits = s.edits.clone();
    let on_action: Option<NoPkTipActionFn<TipScene>> =
        Some(Rc::new(move |_s, action| if action == NoPkTipAction::EditInSql { *edits.borrow_mut() += 1 }));
    tip_build(&s.vm, &on_action, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn fk_rt(vm: TableFkErrorBannerVm, dark: bool) -> Runtime<FkScene> {
    let mut rt = Runtime::new(FkScene { vm, dismiss: Rc::new(RefCell::new(0)), dark }, fk_view, MIN, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn load_rt(vm: TableLoadErrorVm, dark: bool) -> Runtime<LoadScene> {
    let mut rt = Runtime::new(LoadScene { vm, retries: Rc::new(RefCell::new(0)), dark }, load_view, MIN, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn tip_rt(vm: NoPrimaryKeyTipVm, dark: bool) -> Runtime<TipScene> {
    let mut rt = Runtime::new(TipScene { vm, edits: Rc::new(RefCell::new(0)), dark }, tip_view, MIN, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key(els: &[rune::ElementInfo], key: &str) -> bool {
    els.iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn fk(code: &str, raw: &str) -> TableFkErrorBannerVm {
    TableFkErrorBannerVm { error: Some(FkErrorVm { code: code.to_owned(), raw: raw.to_owned() }) }
}

#[test]
fn fk_message_composition_edges_follow_react() {
    assert_eq!(fk("unknown", "raw").message().as_deref(), Some("raw"));
    assert_eq!(fk("grid.other", "raw").message().as_deref(), Some("raw"), "a non-grid.fk code has no reason");
    assert_eq!(fk("grid.fk.", "raw").message().as_deref(), Some("raw"), "an empty reason has no title");
    let expected = format!("{}\nraw", t("fkNav.error.unknownConstraint"));
    assert_eq!(fk("grid.fk.unknownConstraint", "raw").message().as_deref(), Some(expected.as_str()));
}

#[test]
fn load_error_shape_and_tip_action_shape() {
    let load = load_rt(TableLoadErrorVm { error: "e".to_owned(), retry_label: "Retry".to_owned() }, true);
    assert!(has_key(&load.elements(), LOAD_ROOT_KEY));
    let tip = tip_rt(NoPrimaryKeyTipVm { message: "no pk".to_owned(), action: None }, false);
    assert!(has_key(&tip.elements(), TIP_ROOT_KEY));
    assert!(!has_key(&tip.elements(), TIP_ACTION_KEY));
    let tip = tip_rt(
        NoPrimaryKeyTipVm { message: "no pk".to_owned(), action: Some(NoPkAction { label: "Edit in SQL".to_owned() }) },
        true,
    );
    assert!(has_key(&tip.elements(), TIP_ACTION_KEY));
}

#[test]
fn geometry_holds_for_all_three_banners() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut rt = Runtime::new(
                FkScene { vm: fk("grid.fk.valueNull", "detail"), dismiss: Rc::new(RefCell::new(0)), dark },
                fk_view, (w, h), Env { system_dark: dark, scale: 1.0 },
            );
            rt.frame(0.0);
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "audit: fk {:?} horizontal overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "audit: fk {:?} vertical overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
            }
        }
    }
}

// ----- React parity assertions -----

#[test]
fn react_fk_banner_skips_an_empty_message() {
    let rt = fk_rt(fk("unknown", ""), true);
    assert!(
        !has_key(&rt.elements(), FK_ROOT_KEY),
        "DEFECT: React returns null when the composed message is empty (TableFkErrorBanner.tsx:36)"
    );
}

#[test]
fn react_fk_banner_offers_a_copy_control() {
    let rt = fk_rt(fk("grid.fk.unknownConstraint", "raw"), true);
    assert!(
        has_key(&rt.elements(), "table-fk-error-text-copy"),
        "DEFECT: React renders CopyableError with copyButton (TableFkErrorBanner.tsx:41)"
    );
}

#[test]
fn react_load_error_offers_a_copy_control() {
    let rt = load_rt(TableLoadErrorVm { error: "load failed".to_owned(), retry_label: "Retry".to_owned() }, true);
    assert!(
        has_key(&rt.elements(), "table-load-error-text-copy"),
        "DEFECT: React renders CopyableError with copyButton (TableLoadErrorState.tsx:17)"
    );
}

#[test]
fn react_no_pk_tip_shows_the_message_tooltip() {
    let rt = tip_rt(NoPrimaryKeyTipVm { message: "no pk".to_owned(), action: None }, true);
    let el = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(TIP_TEXT_KEY)).unwrap();
    assert_eq!(
        el.help.as_deref(),
        Some("no pk"),
        "DEFECT: React sets title={{message}} on the tip text (TableNoPrimaryKeyTip.tsx:44)"
    );
}
