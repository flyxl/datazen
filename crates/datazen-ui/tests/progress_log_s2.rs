//! T076-S2: `ProgressLog` component tests — empty spacer, copy label,
//! copy press, row geometry, and the virtual window
//! (`views/backup/progress_log.rs`).

use rune::{Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::{copy_text, ProgressLog, ProgressLogState, ROW_HEIGHT};

const VIEWPORT: (f32, f32) = (480.0, 320.0);

struct Scene {
    lines: Vec<String>,
    log: ProgressLogState,
    copies: Vec<String>,
    dark: bool,
}

impl Scene {
    fn new(lines: &[&str]) -> Self {
        Self {
            lines: lines.iter().map(|line| (*line).to_owned()).collect(),
            log: ProgressLogState::new(),
            copies: Vec::new(),
            dark: true,
        }
    }
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let page = ProgressLog::<Scene>::new(&scene.lines)
        .copied(scene.log.copied)
        .auto_scroll(scene.log.auto_scroll)
        .on_copy(|state: &mut Scene| {
            if let Some(text) = copy_text(&state.lines) {
                let _request = state.log.copy_pressed();
                state.copies.push(text);
            }
        })
        .build(kit(scene.dark));
    // A fixed box so the virtual list has a real viewport. The log
    // grows inside it; without a bound the column would size to its
    // content and the windowing assertion would be meaningless.
    rune::col().w_full().h(VIEWPORT.1).child(page)
}

fn mount(scene: Scene) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, VIEWPORT, env(dark));
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key)
        .text
        .unwrap_or_else(|| panic!("{key} has text"))
}

fn any_text<S>(rt: &Runtime<S>, text: &str) -> bool {
    rt.elements().iter().any(|e| e.text.as_deref() == Some(text))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let rect = element(rt, key).rect;
    rt.pointer_down(rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5);
    rt.pointer_up(rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

#[test]
fn empty_log_is_the_spacer_and_has_no_copy_button() {
    let rt = mount(Scene::new(&[]));
    assert!(has_key(&rt, "backup-progress-log-spacer"));
    assert!(!has_key(&rt, "backup-progress-log"));
    assert!(!has_key(&rt, "backup-progress-log-copy"));
    assert!(!has_key(&rt, "backup-progress-log-line-0"));
}

#[test]
fn two_lines_render_at_row_height_and_the_copy_label_is_idle() {
    let rt = mount(Scene::new(&["第一行", "第二行"]));
    assert!(!has_key(&rt, "backup-progress-log-spacer"));
    assert!(has_key(&rt, "backup-progress-log"));
    assert!(has_key(&rt, "backup-progress-log-copy"));
    assert!(any_text(&rt, t("backup.progressLog")));
    assert!(any_text(&rt, t("backup.copyLog")));
    assert!(!any_text(&rt, t("backup.logCopied")));
    assert_eq!(text_of(&rt, "backup-progress-log-line-0"), "第一行");
    assert_eq!(text_of(&rt, "backup-progress-log-line-1"), "第二行");

    let first = element(&rt, "backup-progress-log-line-0").rect;
    let second = element(&rt, "backup-progress-log-line-1").rect;
    assert!(
        (first[3] - ROW_HEIGHT as f32).abs() < 1.0,
        "row height {first:?}, expected {ROW_HEIGHT}"
    );
    let dy = second[1] - first[1];
    assert!(
        (dy - ROW_HEIGHT as f32).abs() < 1.0,
        "row step {dy}, rects {first:?} {second:?}"
    );
}

#[test]
fn copy_press_records_the_joined_text_and_flips_the_label() {
    let mut rt = mount(Scene::new(&["第一行", "第二行"]));
    click(&mut rt, "backup-progress-log-copy");
    assert_eq!(rt.state.copies, ["第一行\n第二行"]);
    assert!(rt.state.log.copied);
    assert_eq!(rt.state.log.copy_request, 1);
    assert!(any_text(&rt, t("backup.logCopied")));
    assert!(!any_text(&rt, t("backup.copyLog")));
}

#[test]
fn a_single_empty_line_does_not_copy() {
    // React `if (!text) return`: one empty line joins to "" and copies nothing.
    let mut rt = mount(Scene::new(&[""]));
    click(&mut rt, "backup-progress-log-copy");
    assert!(rt.state.copies.is_empty());
    assert!(!rt.state.log.copied);
    assert!(any_text(&rt, t("backup.copyLog")));
}

#[test]
fn a_long_log_mounts_the_window_and_not_the_whole_list() {
    let lines: Vec<String> = (0..400).map(|i| format!("line-{i}")).collect();
    let scene = Scene {
        lines: lines.clone(),
        log: ProgressLogState::new(),
        copies: Vec::new(),
        dark: true,
    };
    // Default auto-scroll is pinned, so the viewport opens at the bottom
    // (`stick_bottom`): the tail is built, the head is not.
    let rt = mount(scene);
    assert!(
        has_key(&rt, "backup-progress-log-line-399"),
        "pinned log should mount the tail"
    );
    assert_eq!(text_of(&rt, "backup-progress-log-line-399"), "line-399");
    assert!(
        !has_key(&rt, "backup-progress-log-line-0"),
        "the head row must not be built while pinned to the bottom"
    );
    let mounted = rt
        .elements()
        .iter()
        .filter(|e| {
            e.key
                .as_deref()
                .is_some_and(|key| key.starts_with("backup-progress-log-line-"))
        })
        .count();
    assert!(
        mounted < 80,
        "mounted {mounted} rows in a {}pt viewport; expected a window, not the log",
        VIEWPORT.1
    );
    assert!(mounted > 2, "mounted {mounted} rows; the window collapsed");

    // Unpinned opens at the top: the head is built, the tail is not.
    let mut unpinned = ProgressLogState::new();
    unpinned.auto_scroll = false;
    let rt = mount(Scene {
        lines,
        log: unpinned,
        copies: Vec::new(),
        dark: true,
    });
    assert!(has_key(&rt, "backup-progress-log-line-0"));
    assert_eq!(text_of(&rt, "backup-progress-log-line-0"), "line-0");
    assert!(
        !has_key(&rt, "backup-progress-log-line-399"),
        "the tail row must not be built while pinned to the top"
    );
}

#[test]
fn unpinned_auto_scroll_still_renders_the_rows() {
    let mut scene = Scene::new(&["仅一行"]);
    scene.log.auto_scroll = false;
    let rt = mount(scene);
    assert_eq!(text_of(&rt, "backup-progress-log-line-0"), "仅一行");
    assert!(has_key(&rt, "backup-progress-log"));
}

#[test]
fn light_theme_keeps_the_same_keys() {
    let mut scene = Scene::new(&["light"]);
    scene.dark = false;
    let rt = mount(scene);
    assert_eq!(text_of(&rt, "backup-progress-log-line-0"), "light");
    assert!(any_text(&rt, t("backup.copyLog")));
}
