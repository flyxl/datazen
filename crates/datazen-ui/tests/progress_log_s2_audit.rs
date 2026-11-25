//! T076-S2 independent audit of `ProgressLog`.
//!
//! The observations here do not call [`copy_text`], [`visible_range`] or
//! [`ROW_HEIGHT`]. Row pitch is the laid-out rect delta. The virtual
//! window is which line keys exist. The copied label is whatever the
//! host flag renders, not what a press handler decided.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::{ProgressLog, ProgressLogState};

const SIZE: (f32, f32) = (420.0, 200.0);

struct Audit {
    lines: Vec<String>,
    copied: bool,
    pinned: bool,
    presses: u32,
    dark: bool,
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn view(scene: &Audit, _cx: &mut Cx<Audit>) -> El {
    let page = ProgressLog::<Audit>::new(&scene.lines)
        .copied(scene.copied)
        .auto_scroll(scene.pinned)
        .on_copy(|state: &mut Audit| state.presses += 1)
        .build(kit(scene.dark));
    rune::col().w_full().h(SIZE.1).child(page)
}

fn mount(scene: Audit) -> Runtime<Audit> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, SIZE, env(dark));
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
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

fn line_keys<S>(rt: &Runtime<S>) -> Vec<String> {
    let mut keys: Vec<String> = rt
        .elements()
        .iter()
        .filter_map(|e| {
            if e.exiting {
                return None;
            }
            let key = e.key.as_deref()?;
            key.starts_with("backup-progress-log-line-")
                .then(|| key.to_owned())
        })
        .collect();
    keys.sort();
    keys
}

#[test]
fn audit_empty_log_spacer_has_no_chrome() {
    let rt = mount(Audit {
        lines: Vec::new(),
        copied: true,
        pinned: true,
        presses: 0,
        dark: true,
    });
    assert!(has_key(&rt, "backup-progress-log-spacer"));
    assert!(!has_key(&rt, "backup-progress-log-copy"));
    assert!(!has_key(&rt, "backup-progress-log-header"));
    assert!(line_keys(&rt).is_empty());
    // A copied flag on an empty log must not invent the button.
    assert!(!any_text(&rt, t("backup.logCopied")));
}

#[test]
fn audit_row_pitch_is_twenty_points() {
    let rt = mount(Audit {
        lines: vec!["甲".into(), "乙".into(), "丙".into()],
        copied: false,
        pinned: true,
        presses: 0,
        dark: false,
    });
    let a = element(&rt, "backup-progress-log-line-0").rect;
    let b = element(&rt, "backup-progress-log-line-1").rect;
    let c = element(&rt, "backup-progress-log-line-2").rect;
    let step = b[1] - a[1];
    assert!((step - 20.0).abs() < 0.5, "step {step}, rects {a:?} {b:?}");
    assert!(
        ((c[1] - b[1]) - step).abs() < 0.5,
        "pitch drifted: {a:?} {b:?} {c:?}"
    );
    assert!((a[3] - 20.0).abs() < 1.0, "row height {}", a[3]);
    assert!(any_text(&rt, t("backup.copyLog")));
    assert!(!any_text(&rt, t("backup.logCopied")));
}

#[test]
fn audit_host_copied_flag_flips_the_label_without_a_press() {
    let rt = mount(Audit {
        lines: vec!["only".into()],
        copied: true,
        pinned: true,
        presses: 0,
        dark: true,
    });
    assert_eq!(rt.state.presses, 0);
    assert!(any_text(&rt, t("backup.logCopied")));
    assert!(!any_text(&rt, t("backup.copyLog")));
}

#[test]
fn audit_copy_press_increments_once_and_does_not_invent_a_second_line() {
    let mut rt = mount(Audit {
        lines: vec!["only".into()],
        copied: false,
        pinned: true,
        presses: 0,
        dark: true,
    });
    click(&mut rt, "backup-progress-log-copy");
    assert_eq!(rt.state.presses, 1);
    assert!(has_key(&rt, "backup-progress-log-line-0"));
    assert!(!has_key(&rt, "backup-progress-log-line-1"));
}

#[test]
fn audit_pinned_long_log_builds_a_window_not_every_row() {
    let lines: Vec<String> = (0..300).map(|i| format!("row-{i:03}")).collect();
    let rt = mount(Audit {
        lines,
        copied: false,
        pinned: true,
        presses: 0,
        dark: true,
    });
    let keys = line_keys(&rt);
    assert!(
        keys.iter().any(|key| key == "backup-progress-log-line-299"),
        "pinned window missing the tail: {keys:?}"
    );
    assert!(
        !keys.iter().any(|key| key == "backup-progress-log-line-0"),
        "pinned window built the head: {keys:?}"
    );
    assert!(
        keys.len() < 60,
        "built {} rows, which is the list rather than a window",
        keys.len()
    );
}

#[test]
fn audit_unpinned_long_log_builds_the_head() {
    let lines: Vec<String> = (0..300).map(|i| format!("row-{i:03}")).collect();
    let rt = mount(Audit {
        lines,
        copied: false,
        pinned: false,
        presses: 0,
        dark: true,
    });
    let keys = line_keys(&rt);
    assert!(keys.iter().any(|key| key == "backup-progress-log-line-0"));
    assert!(!keys.iter().any(|key| key == "backup-progress-log-line-299"));
    assert!(keys.len() < 60, "built {} rows", keys.len());
}

#[test]
fn audit_progress_log_state_default_is_what_the_component_assumes() {
    // The component defaults copied=false and auto_scroll=true. The
    // state type's Default must still be that seed, or a host that
    // passes `ProgressLogState::default()` into the two flags will
    // disagree with a host that omits them.
    let state = ProgressLogState::default();
    assert!(!state.copied);
    assert!(state.auto_scroll);
    assert_eq!(state.copy_request, 0);
}
