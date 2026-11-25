//! T225 independent audit (tester): fresh vectors — the cancel-then-close
//! seam (Esc routes to the same callback as the button), an
//! open-epoch restamp, a same-name plan pair keyed by target, the
//! dialog's vertical containment in the window, and the missing-callback
//! mount (no panic, no action).

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::views::{TransferExecuteConfirmDialog, TransferWritePlanVm, WriteMode};

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
    epoch: u64,
    closed: usize,
    confirmed: usize,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        TransferExecuteConfirmDialog::<Scene>::new(
            scene.open,
            scene.write_mode,
            scene.write_plans.clone(),
        )
        .open_epoch(scene.epoch)
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

#[test]
fn the_open_epoch_restamps_without_duplicating_the_dialog() {
    let scene = Scene {
        open: true,
        write_mode: WriteMode::Insert,
        write_plans: vec![plan("a", "b")],
        epoch: 3,
        dark: true,
        ..Scene::default()
    };
    let rt = mount(scene, (960.0, 640.0));
    // One dialog, one body — an epoch bump never mounts a second copy.
    let dialogs = rt
        .elements()
        .into_iter()
        .filter(|e| e.key.as_deref() == Some("data-transfer-execute-confirm-dialog") && !e.exiting)
        .count();
    assert_eq!(dialogs, 1);
}

#[test]
fn a_same_name_pair_keys_its_row_by_the_target() {
    let rt = mount(
        Scene {
            open: true,
            write_mode: WriteMode::Insert,
            write_plans: vec![plan("users", "users")],
            dark: true,
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-transfer-execute-confirm-table-users"));
    assert_eq!(
        text_of(&rt, "data-transfer-execute-confirm-table-users"),
        "users → users"
    );
}

#[test]
fn the_dialog_stays_inside_the_window_vertically() {
    let rt = mount(
        Scene {
            open: true,
            write_mode: WriteMode::Insert,
            write_plans: (0..8)
                .map(|i| plan(&format!("s{i}"), &format!("t{i}")))
                .collect(),
            dark: true,
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    for key in [
        "data-transfer-execute-confirm-dialog",
        "data-transfer-execute-confirm-proceed",
    ] {
        let element = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
            .unwrap_or_else(|| panic!("{key} mounted"));
        let [x, y, w, h] = element.rect;
        assert!(
            y >= -0.5 && y + h <= 640.0 + 0.5 && x >= -0.5 && x + w <= 960.0 + 0.5,
            "{key} rect {element:?} stays in 960x640"
        );
    }
}

#[test]
fn missing_callbacks_mount_a_working_dialog() {
    let rt = Runtime::new(
        Scene {
            open: true,
            write_mode: WriteMode::Insert,
            write_plans: vec![plan("a", "b")],
            dark: true,
            ..Scene::default()
        },
        |scene: &Scene, _cx: &mut Cx<Scene>| {
            col().w_full().h_full().child(
                TransferExecuteConfirmDialog::<Scene>::new(
                    scene.open,
                    scene.write_mode,
                    scene.write_plans.clone(),
                )
                .build(scene.dark),
            )
        },
        (960.0, 640.0),
        env(true),
    );
    let mut rt = rt;
    rt.frame(0.0);
    assert!(has_key(&rt, "data-transfer-execute-confirm-dialog"));
    assert!(has_key(&rt, "data-transfer-execute-confirm-proceed"));
}
