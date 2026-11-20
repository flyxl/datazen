//! T225: `windows/data-transfer/TransferExecuteConfirmDialog.tsx` port —
//! the write-mode intro, the plan list, the footer buttons, the close /
//! confirm callbacks, and both-window geometry.

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::{TransferExecuteConfirmDialog, TransferWritePlanVm, WriteMode};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn plan(source: &str, target: &str) -> TransferWritePlanVm {
    TransferWritePlanVm {
        source_table: source.to_owned(),
        target_table: target.to_owned(),
        write_mode: WriteMode::Insert,
        mapped_columns: Vec::new(),
        estimated_rows: None,
        preamble: Vec::new(),
        source_filter_preview: None,
        recordset_preview: None,
    }
}

#[derive(Default)]
struct Scene {
    open: bool,
    write_mode: WriteMode,
    write_plans: Vec<TransferWritePlanVm>,
    closed: usize,
    confirmed: usize,
    dark: bool,
}

impl Scene {
    fn with(open: bool, write_mode: WriteMode, plans: Vec<TransferWritePlanVm>) -> Self {
        Self {
            open,
            write_mode,
            write_plans: plans,
            dark: true,
            ..Self::default()
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        TransferExecuteConfirmDialog::<Scene>::new(
            scene.open,
            scene.write_mode,
            scene.write_plans.clone(),
        )
        .on_close(|s| s.closed += 1)
        .on_confirm(|s| s.confirmed += 1)
        .build(scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn the_dialog_mounts_its_title_body_and_footer() {
    let rt = mount(
        Scene::with(true, WriteMode::TruncateInsert, vec![plan("users", "users_new")]),
        (960.0, 640.0),
    );
    for key in [
        "data-transfer-execute-confirm-dialog",
        "data-transfer-execute-confirm",
        "data-transfer-execute-confirm-intro",
        "data-transfer-execute-confirm-table-users_new",
        "data-transfer-execute-confirm-cancel",
        "data-transfer-execute-confirm-proceed",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
}

#[test]
fn a_closed_dialog_mounts_nothing() {
    let rt = mount(Scene::with(false, WriteMode::Insert, vec![plan("a", "b")]), (960.0, 640.0));
    assert!(!has_key(&rt, "data-transfer-execute-confirm-dialog"));
    assert!(!has_key(&rt, "data-transfer-execute-confirm"));
}

#[test]
fn the_intro_follows_the_write_mode() {
    let rt = mount(Scene::with(true, WriteMode::TruncateInsert, Vec::new()), (960.0, 640.0));
    assert_eq!(
        text_of(&rt, "data-transfer-execute-confirm-intro"),
        t("transfer.executeConfirm.introTruncate")
    );
    let rt = mount(Scene::with(true, WriteMode::Insert, Vec::new()), (960.0, 640.0));
    assert_eq!(
        text_of(&rt, "data-transfer-execute-confirm-intro"),
        t("transfer.executeConfirm.introTruncate"),
        "insert is not dropCreateInsert: the truncate intro is the else branch"
    );
    let rt = mount(Scene::with(true, WriteMode::DropCreateInsert, Vec::new()), (960.0, 640.0));
    assert_eq!(
        text_of(&rt, "data-transfer-execute-confirm-intro"),
        t("transfer.executeConfirm.introDrop")
    );
}

#[test]
fn the_plan_rows_list_the_pairs_in_order() {
    let rt = mount(
        Scene::with(true, WriteMode::DropCreateInsert, vec![plan("src_a", "dst_a"), plan("src_b", "dst_b")]),
        (960.0, 640.0),
    );
    assert_eq!(
        text_of(&rt, "data-transfer-execute-confirm-table-dst_a"),
        "src_a → dst_a"
    );
    assert_eq!(
        text_of(&rt, "data-transfer-execute-confirm-table-dst_b"),
        "src_b → dst_b"
    );
    let a = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-transfer-execute-confirm-table-dst_a") && !e.exiting)
        .expect("a mounted");
    let b = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-transfer-execute-confirm-table-dst_b") && !e.exiting)
        .expect("b mounted");
    assert!(a.rect[1] < b.rect[1], "the rows stack in plan order");
}

#[test]
fn the_footer_buttons_fire_their_own_callbacks() {
    let mut rt = mount(Scene::with(true, WriteMode::Insert, vec![plan("a", "b")]), (960.0, 640.0));
    click(&mut rt, "data-transfer-execute-confirm-cancel");
    assert_eq!(rt.state.closed, 1);
    assert_eq!(rt.state.confirmed, 0);
    click(&mut rt, "data-transfer-execute-confirm-proceed");
    assert_eq!(rt.state.confirmed, 1);
    assert_eq!(rt.state.closed, 1);
}

#[test]
fn an_empty_plan_list_mounts_the_intro_alone() {
    let rt = mount(Scene::with(true, WriteMode::Insert, Vec::new()), (960.0, 640.0));
    assert!(has_key(&rt, "data-transfer-execute-confirm-intro"));
    assert!(!has_key(&rt, "data-transfer-execute-confirm-table-any"));
}

#[test]
fn the_dialog_fits_both_window_widths() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = mount(Scene::with(true, WriteMode::Insert, vec![plan("a", "b")]), (w, h));
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else { continue };
            if !key.starts_with("data-transfer-execute-confirm") {
                continue;
            }
            let [x, _y, ew, _eh] = e.rect;
            if x < -0.5 || x + ew > w + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{w}x{h}: {outside:?}");
    }
}

#[test]
fn the_light_theme_mounts_the_same_dialog() {
    let mut scene = Scene::with(true, WriteMode::Insert, vec![plan("a", "b")]);
    scene.dark = false;
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, "data-transfer-execute-confirm-dialog"));
    assert!(has_key(&rt, "data-transfer-execute-confirm-proceed"));
}
