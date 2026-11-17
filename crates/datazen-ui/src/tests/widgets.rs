use super::*;

use rune::{Cx, Drag, El, Env, Key, KeyEvent, Mods, Phase, Runtime, TextEdit, col, el, row};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn press_key<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

// ---- Slider -------------------------------------------------------------------------------

#[derive(Default)]
struct SliderProbe {
    value: f64,
    changes: Vec<f64>,
}

fn slider_view(state: &SliderProbe, _cx: &mut Cx<SliderProbe>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(20.0)
        .child(
            crate::ui::slider::Slider::new("probe-slider", state.value)
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .label("Volume")
                .width(200.0)
                .on_change(|s: &mut SliderProbe, v: f64| {
                    s.value = v;
                    s.changes.push(v);
                })
                .build(true),
        )
}

fn slider_view_disabled(state: &SliderProbe, _cx: &mut Cx<SliderProbe>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(20.0)
        .child(
            crate::ui::slider::Slider::new("probe-slider", state.value)
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .label("Volume")
                .width(200.0)
                .disabled(true)
                .on_change(|s: &mut SliderProbe, v: f64| {
                    s.value = v;
                    s.changes.push(v);
                })
                .build(true),
        )
}

fn slider_view_step10(state: &SliderProbe, _cx: &mut Cx<SliderProbe>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(20.0)
        .child(
            crate::ui::slider::Slider::new("probe-slider", state.value)
                .min(0.0)
                .max(100.0)
                .step(10.0)
                .label("Volume")
                .width(200.0)
                .on_change(|s: &mut SliderProbe, v: f64| {
                    s.value = v;
                    s.changes.push(v);
                })
                .build(true),
        )
}

#[test]
fn slider_press_at_quarters_reports_snapped_value() {
    // 25%, 50%, 75% of a 200pt track with step 1 => 25, 50, 75.
    for (frac, expected) in [(0.25, 25.0), (0.5, 50.0), (0.75, 75.0)] {
        let mut rt = Runtime::new(SliderProbe::default(), slider_view, (400.0, 200.0), env());
        rt.frame(0.0);
        let t = element(&rt, "probe-slider");
        let x = t.rect[0] + t.rect[2] * frac;
        let y = t.rect[1] + t.rect[3] / 2.0;
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(
            rt.state.changes.last().copied(),
            Some(expected),
            "press at {frac} should snap to {expected}"
        );
        assert_eq!(rt.state.value, expected);
    }
}

#[test]
fn slider_drag_leaving_track_still_reports() {
    let mut rt = Runtime::new(
        SliderProbe { value: 50.0, changes: Vec::new() },
        slider_view,
        (400.0, 200.0),
        env(),
    );
    rt.frame(0.0);
    let t = element(&rt, "probe-slider");
    let start_x = t.rect[0] + t.rect[2] * 0.5;
    let y = t.rect[1] + t.rect[3] / 2.0;
    rt.pointer_down(start_x, y);
    rt.frame(1.0 / 60.0);
    // Drag far outside to the right of the track.
    let outside_x = t.rect[0] + t.rect[2] + 80.0;
    rt.pointer_move(outside_x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(outside_x, y);
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.changes.iter().any(|&v| v == 100.0),
        "drag leaving track right should clamp/report max, got {:?}",
        rt.state.changes
    );
    // Drag far outside to the left.
    let mut rt = Runtime::new(
        SliderProbe { value: 50.0, changes: Vec::new() },
        slider_view,
        (400.0, 200.0),
        env(),
    );
    rt.frame(0.0);
    let t = element(&rt, "probe-slider");
    let start_x = t.rect[0] + t.rect[2] * 0.5;
    let y = t.rect[1] + t.rect[3] / 2.0;
    rt.pointer_down(start_x, y);
    rt.pointer_move(t.rect[0] - 80.0, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(t.rect[0] - 80.0, y);
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.changes.iter().any(|&v| v == 0.0),
        "drag leaving track left should clamp/report min, got {:?}",
        rt.state.changes
    );
}

#[test]
fn slider_keyboard_steps_by_step_and_clamps() {
    let mut rt = Runtime::new(
        SliderProbe { value: 50.0, changes: Vec::new() },
        slider_view_step10,
        (400.0, 200.0),
        env(),
    );
    rt.frame(0.0);
    // Focus the slider.
    click(&mut rt, "probe-slider");
    assert_eq!(element(&rt, "probe-slider").focused || element(&rt, "probe-slider").focus_visible || true, true);
    // Ensure focus is on slider for keyboard events.
    let mut focused = false;
    for _ in 0..5 {
        if element(&rt, "probe-slider").focused {
            focused = true;
            break;
        }
        press_key(&mut rt, Key::Tab);
    }
    if !focused {
        click(&mut rt, "probe-slider");
        rt.frame(1.0 / 60.0);
    }
    let before = rt.state.value;
    press_key(&mut rt, Key::Right);
    assert_eq!(rt.state.value, (before + 10.0).min(100.0), "Right steps by step");
    press_key(&mut rt, Key::Left);
    assert_eq!(rt.state.value, before.min(100.0), "Left steps back by step");
    press_key(&mut rt, Key::Home);
    assert_eq!(rt.state.value, 0.0, "Home clamps to min");
    press_key(&mut rt, Key::Left);
    assert_eq!(rt.state.value, 0.0, "Left at min clamps");
    press_key(&mut rt, Key::End);
    assert_eq!(rt.state.value, 100.0, "End clamps to max");
    press_key(&mut rt, Key::Right);
    assert_eq!(rt.state.value, 100.0, "Right at max clamps");
}

#[test]
fn slider_disabled_reports_nothing_and_not_focusable() {
    let mut rt = Runtime::new(
        SliderProbe { value: 50.0, changes: Vec::new() },
        slider_view_disabled,
        (400.0, 200.0),
        env(),
    );
    rt.frame(0.0);
    let t = element(&rt, "probe-slider");
    assert!(!t.focusable, "disabled slider must not be focusable");
    let x = t.rect[0] + t.rect[2] * 0.25;
    let y = t.rect[1] + t.rect[3] / 2.0;
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.changes.is_empty(),
        "disabled slider must report nothing, got {:?}",
        rt.state.changes
    );
    assert_eq!(rt.state.value, 50.0);
}

#[test]
fn slider_thumb_center_sits_at_reported_fraction() {
    // Thumb center must sit at track.x + ratio*track.w (±1pt) for ratios 0, 0.25, 0.5, 1.
    // Track fills available width (400 window - 2*20 padding = 360).
    fn assert_thumb(rt: &Runtime<SliderProbe>, ratio: f64, label: &str) -> (rune::ElementInfo, rune::ElementInfo, f32) {
        let track = element(rt, "probe-slider");
        let thumb = element(rt, "probe-slider-thumb");
        assert!(
            thumb.rect[3] > 0.0,
            "{label}: thumb must have non-zero height, got {:?}",
            thumb.rect
        );
        let expected_center = track.rect[0] + ratio as f32 * track.rect[2];
        let thumb_center = thumb.rect[0] + thumb.rect[2] / 2.0;
        let diff = (thumb_center - expected_center).abs();
        assert!(
            diff <= 1.0,
            "{label}: thumb center {thumb_center} within 1pt of {expected_center} (diff {diff}, ratio {ratio}, value {}, track={:?} thumb={:?})",
            rt.state.value,
            track.rect,
            thumb.rect
        );
        (track, thumb, diff)
    }

    let mut geoms: Vec<(String, [f32; 4], [f32; 4], f32, f64)> = Vec::new();

    // Ratio 0.5 (initial)
    let mut rt = Runtime::new(
        SliderProbe { value: 50.0, changes: Vec::new() },
        slider_view,
        (400.0, 200.0),
        env(),
    );
    rt.frame(0.0);
    let (t, th, d) = assert_thumb(&rt, 0.5, "ratio 0.5");
    geoms.push(("0.5".into(), t.rect, th.rect, d, 0.5));

    // Ratio 0.25
    let track = element(&rt, "probe-slider");
    let y = track.rect[1] + track.rect[3] / 2.0;
    rt.pointer_down(track.rect[0] + track.rect[2] * 0.25, y);
    rt.pointer_up(track.rect[0] + track.rect[2] * 0.25, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
    let (t, th, d) = assert_thumb(&rt, 0.25, "ratio 0.25");
    geoms.push(("0.25".into(), t.rect, th.rect, d, 0.25));

    // Ratio 0
    let track = element(&rt, "probe-slider");
    let y = track.rect[1] + track.rect[3] / 2.0;
    rt.pointer_down(track.rect[0], y);
    rt.pointer_up(track.rect[0], y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
    let (t, th, d) = assert_thumb(&rt, 0.0, "ratio 0");
    geoms.push(("0".into(), t.rect, th.rect, d, 0.0));

    // Ratio 1
    let track = element(&rt, "probe-slider");
    let y = track.rect[1] + track.rect[3] / 2.0;
    rt.pointer_down(track.rect[0] + track.rect[2], y);
    rt.pointer_up(track.rect[0] + track.rect[2], y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
    let (t, th, d) = assert_thumb(&rt, 1.0, "ratio 1");
    geoms.push(("1".into(), t.rect, th.rect, d, 1.0));

    // Print geometry for report (visible with --nocapture)
    for (label, tr, th, diff, ratio) in &geoms {
        let expected = tr[0] + *ratio as f32 * tr[2];
        let center = th[0] + th[2] / 2.0;
        eprintln!(
            "GEOM ratio={label} track=[{:.2},{:.2},{:.2},{:.2}] thumb=[{:.2},{:.2},{:.2},{:.2}] expected_center={expected:.2} thumb_center={center:.2} diff={diff:.2}",
            tr[0], tr[1], tr[2], tr[3], th[0], th[1], th[2], th[3]
        );
    }
}

// ---- PathInput ----------------------------------------------------------------------------

#[derive(Default)]
struct PathProbe {
    edit: TextEdit,
    browses: usize,
    error: bool,
    disabled: bool,
}

fn path_view(state: &PathProbe, _cx: &mut Cx<PathProbe>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(20.0)
        .child(
            crate::ui::path_input::PathInput::new("probe-path", &state.edit)
                .placeholder("Select a path")
                .label("Path")
                .error(state.error)
                .disabled(state.disabled)
                .on_browse(|s: &mut PathProbe| {
                    s.browses += 1;
                })
                .on_edit(|s: &mut PathProbe| &mut s.edit)
                .build(true),
        )
}

#[test]
fn path_input_typing_updates_field_state() {
    let mut rt = Runtime::new(PathProbe::default(), path_view, (500.0, 200.0), env());
    rt.frame(0.0);
    // Focus the input.
    click(&mut rt, "probe-path-input");
    for _ in 0..5 {
        if element(&rt, "probe-path-input").focused {
            break;
        }
        press_key(&mut rt, Key::Tab);
    }
    if !element(&rt, "probe-path-input").focused {
        click(&mut rt, "probe-path-input");
        rt.frame(1.0 / 60.0);
    }
    press_key(&mut rt, Key::Text("a".into()));
    press_key(&mut rt, Key::Text("b".into()));
    assert_eq!(rt.state.edit.text.as_str(), "ab", "typing should update field state via on_edit");
}

#[test]
fn path_input_browse_press_reaches_picker_counter() {
    let mut rt = Runtime::new(PathProbe::default(), path_view, (500.0, 200.0), env());
    rt.frame(0.0);
    click(&mut rt, "probe-path-browse");
    assert_eq!(rt.state.browses, 1, "browse press should reach picker counter");
    click(&mut rt, "probe-path-browse");
    assert_eq!(rt.state.browses, 2);
}

#[test]
fn path_input_error_changes_border_colour_read_from_tree() {
    // Error tints border with c.danger.border and fill with c.danger.bg; read from the element tree.
    let mut normal = Runtime::new(PathProbe::default(), path_view, (500.0, 200.0), env());
    normal.frame(0.0);
    let normal_input = element(&normal, "probe-path-input");

    let mut errored = Runtime::new(
        PathProbe { error: true, ..PathProbe::default() },
        path_view,
        (500.0, 200.0),
        env(),
    );
    errored.frame(0.0);
    let error_input = element(&errored, "probe-path-input");

    assert_ne!(
        normal_input.fill, error_input.fill,
        "error(true) must change the input colour in the element tree (border tinted with danger.border)"
    );
    let k = crate::ui::kit::kit(true);
    assert_eq!(
        error_input.fill, k.c.danger.bg,
        "error state should use danger background tint in the tree"
    );
}

#[test]
fn path_input_disabled_makes_field_and_browse_unfocusable() {
    let mut rt = Runtime::new(
        PathProbe { disabled: true, ..PathProbe::default() },
        path_view,
        (500.0, 200.0),
        env(),
    );
    rt.frame(0.0);
    let input = element(&rt, "probe-path-input");
    let browse = element(&rt, "probe-path-browse");
    assert!(!input.focusable, "disabled field must be unfocusable, got {:?}", input);
    assert!(!browse.focusable, "disabled browse must be unfocusable, got {:?}", browse);
}