//! T510: `QueryTransactionModals` — exact dialog copy, modal options/actions,
//! busy/empty gating and rect geometry at both window sizes in both themes.
//! Every control dispatches through the injected host port; no fake IPC.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::query::query_transaction_modals::{
    build, can_save_favorite, QueryTransactionHostPort, QueryTransactionModalAction as Action,
    QueryTransactionModalsVm, FavoriteNameDialogVm, ABORTED_KEY, BODY_KEY, CANCEL_KEY,
    CONFIRM_KEY, DETAIL_KEY, FAVORITE_CANCEL_KEY, FAVORITE_KEY, ROLLBACK_KEY, SAVE_KEY, SKIP_KEY,
    SQL_KEY, TITLE_INPUT_KEY, UNCLOSED_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: QueryTransactionModalsVm,
    favorite: Option<FavoriteNameDialogVm>,
    actions: Rc<RefCell<Vec<Action>>>,
    dark: bool,
}

fn vm() -> QueryTransactionModalsVm {
    QueryTransactionModalsVm {
        tx_unclosed_open: true,
        tx_aborted_open: true,
        tx_aborted_detail: Some("transaction aborted".to_owned()),
        tx_busy: false,
    }
}

fn favorite() -> FavoriteNameDialogVm {
    FavoriteNameDialogVm {
        open: true,
        favorite_name: rune::TextEdit::new("My query".to_owned()),
        favorite_dialog_sql: "SELECT 1".to_owned(),
    }
}

fn scene(vm: QueryTransactionModalsVm, favorite: Option<FavoriteNameDialogVm>, dark: bool) -> Scene {
    Scene {
        vm,
        favorite,
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let host = QueryTransactionHostPort {
        on_action: Some(Rc::new(move |_s: &mut Scene, action| actions.borrow_mut().push(action))),
        favorite_name_edit: Some(Rc::new(|s: &mut Scene| {
            &mut s.favorite.as_mut().unwrap().favorite_name
        })),
    };
    build(&s.vm, s.favorite.as_ref(), &host, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn settle(rt: &mut Runtime<Scene>) {
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn label_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.label)
        .unwrap_or_default()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    settle(rt);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

#[test]
fn dialog_copy_is_the_exact_react_text() {
    let rt = runtime(scene(vm(), Some(favorite()), true), DESIGN);
    for key in [UNCLOSED_KEY, ABORTED_KEY, FAVORITE_KEY] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(label_of(&rt, UNCLOSED_KEY), t("query.txUnclosedTitle"));
    assert_eq!(label_of(&rt, ABORTED_KEY), t("query.txAbortedTitle"));
    assert_eq!(label_of(&rt, FAVORITE_KEY), t("common.addToFavorites"));
    let rendered: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    for key in ["query.txUnclosedBody", "query.txAbortedBody"] {
        assert!(rendered.iter().any(|text| text == t(key)), "{key}: exact description copy");
    }
    assert_eq!(text_of(&rt, BODY_KEY), t("query.inTransaction"));
    assert_eq!(text_of(&rt, DETAIL_KEY), "transaction aborted");
    assert_eq!(text_of(&rt, SQL_KEY), "SELECT 1");
    assert_eq!(label_of(&rt, CANCEL_KEY), t("query.txUnclosedCancel"));
    assert_eq!(label_of(&rt, CONFIRM_KEY), t("query.txUnclosedConfirm"));
    assert_eq!(label_of(&rt, SKIP_KEY), t("query.txAbortedSkip"));
    assert_eq!(label_of(&rt, ROLLBACK_KEY), t("query.txAbortedRollback"));
    assert_eq!(label_of(&rt, FAVORITE_CANCEL_KEY), t("common.cancel"));
    assert_eq!(label_of(&rt, SAVE_KEY), t("common.save"));
}

#[test]
fn each_modal_option_routes_through_the_host() {
    // Unclosed: confirm and cancel.
    let mut plan = vm();
    plan.tx_aborted_open = false;
    let mut rt = runtime(scene(plan, None, true), DESIGN);
    click(&mut rt, CONFIRM_KEY);
    assert_eq!(*rt.state.actions.borrow(), [Action::ConfirmUnclosed]);

    let mut plan = vm();
    plan.tx_aborted_open = false;
    let mut rt = runtime(scene(plan, None, true), DESIGN);
    click(&mut rt, CANCEL_KEY);
    assert_eq!(*rt.state.actions.borrow(), [Action::CancelUnclosed]);

    // Aborted: rollback and skip.
    let mut plan = vm();
    plan.tx_unclosed_open = false;
    let mut rt = runtime(scene(plan, None, true), DESIGN);
    click(&mut rt, ROLLBACK_KEY);
    assert_eq!(*rt.state.actions.borrow(), [Action::AbortedRollback]);

    let mut plan = vm();
    plan.tx_unclosed_open = false;
    let mut rt = runtime(scene(plan, None, true), DESIGN);
    click(&mut rt, SKIP_KEY);
    assert_eq!(*rt.state.actions.borrow(), [Action::AbortedSkip]);

    // Favorite: save and cancel.
    let mut rt = runtime(scene(QueryTransactionModalsVm::default(), Some(favorite()), true), DESIGN);
    click(&mut rt, SAVE_KEY);
    assert_eq!(*rt.state.actions.borrow(), [Action::FavoriteSave]);

    let mut rt = runtime(scene(QueryTransactionModalsVm::default(), Some(favorite()), true), DESIGN);
    click(&mut rt, FAVORITE_CANCEL_KEY);
    assert_eq!(*rt.state.actions.borrow(), [Action::FavoriteCancel]);
}

#[test]
fn busy_abort_actions_and_blank_favorite_are_gated() {
    let mut plan = vm();
    plan.tx_unclosed_open = false;
    plan.tx_busy = true;
    let mut rt = runtime(scene(plan, None, false), MIN);
    assert!(has_key(&rt, SKIP_KEY));
    assert!(has_key(&rt, ROLLBACK_KEY));
    click(&mut rt, ROLLBACK_KEY);
    click(&mut rt, SKIP_KEY);
    assert!(rt.state.actions.borrow().is_empty(), "txBusy disables the abort actions");

    let mut plan = vm();
    plan.tx_unclosed_open = false;
    plan.tx_aborted_detail = None;
    let rt = runtime(scene(plan, None, true), MIN);
    assert!(!has_key(&rt, DETAIL_KEY), "a null detail renders no pre block");

    assert!(can_save_favorite("My query"));
    assert!(!can_save_favorite("   "), "blank names cannot be saved");
    let mut blank = favorite();
    blank.favorite_name = rune::TextEdit::new("   ".to_owned());
    let mut rt = runtime(scene(QueryTransactionModalsVm::default(), Some(blank), true), MIN);
    assert!(has_key(&rt, TITLE_INPUT_KEY));
    click(&mut rt, SAVE_KEY);
    assert!(rt.state.actions.borrow().is_empty(), "Save is disabled for a blank title");
}

#[test]
fn closed_dialogs_mount_nothing() {
    let rt = runtime(
        scene(QueryTransactionModalsVm::default(), Some(FavoriteNameDialogVm::default()), true),
        MIN,
    );
    for key in [UNCLOSED_KEY, ABORTED_KEY, FAVORITE_KEY, DETAIL_KEY] {
        assert!(!has_key(&rt, key), "{key} is not mounted while closed");
    }
}

#[test]
fn geometry_stays_inside_both_windows_in_both_themes() {
    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut plan = vm();
            plan.tx_aborted_detail = Some("x".repeat(400));
            let rt = runtime(scene(plan, Some(favorite()), dark), (width, height));
            for element in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, w, h] = element.rect;
                assert!(
                    x >= -0.5 && x + w <= width + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} overflows horizontally",
                    element.key,
                    element.rect
                );
                assert!(
                    y >= -0.5 && y + h <= height + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} overflows vertically",
                    element.key,
                    element.rect
                );
            }
        }
    }
}
