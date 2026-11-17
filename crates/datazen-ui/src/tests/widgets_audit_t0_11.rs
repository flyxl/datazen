use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer, write_png};
use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, col};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 2.0, // 2x as requested
    }
}

#[derive(Default)]
struct SliderProbe {
    value: f64,
    changes: Vec<f64>,
}

fn slider_view_fixed(state: &SliderProbe, _cx: &mut Cx<SliderProbe>) -> El {
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

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn render_pixels(rt: &mut Runtime<SliderProbe>, size: (u32, u32)) -> Vec<u8> {
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let target = Offscreen::new(&gpu.device, size.0, size.1);
    renderer.prepare(&gpu.device, &gpu.queue, &mut rt.text.atlas, &rt.instances, size);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.draw(&mut encoder, &target.view, [0.0; 4]);
    target.copy(&mut encoder);
    gpu.queue.submit([encoder.finish()]);
    target.read(&gpu.device)
}

fn px(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn luminance(p: [u8; 4]) -> f32 {
    let a = p[3] as f32 / 255.0;
    // premul-aware-ish brightness proxy
    (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0 * a + p[3] as f32 * 0.01
}

fn analyze_knob_center_x(rgba: &[u8], w: u32, h: u32, track: [f32; 4], expected_pt: f32) -> Option<f32> {
    // Target a window around expected knob center to avoid conflating accent fill/track.
    let scale = 2.0f32;
    let y_center_pt = track[1] + track[3] / 2.0;
    let y0 = ((y_center_pt - 14.0) * scale).max(0.0) as u32;
    let y1 = ((y_center_pt + 14.0) * scale).min(h as f32 - 1.0) as u32;
    let x0 = ((expected_pt - 14.0) * scale).max(0.0) as u32;
    let x1 = ((expected_pt + 14.0) * scale).min(w as f32 - 1.0) as u32;

    let mut max_lum = 0.0f32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = px(rgba, w, x, y);
            let l = luminance(p);
            if l > max_lum { max_lum = l; }
        }
    }
    // Also do a wider scan to report where bright mass actually is (diagnostic).
    let wy0 = ((y_center_pt - 20.0) * scale).max(0.0) as u32;
    let wy1 = ((y_center_pt + 20.0) * scale).min(h as f32 - 1.0) as u32;
    let wx0 = ((track[0] - 20.0) * scale).max(0.0) as u32;
    let wx1 = ((track[0] + track[2] + 20.0) * scale).min(w as f32 - 1.0) as u32;
    let mut wmax = 0.0f32;
    for y in wy0..=wy1 { for x in wx0..=wx1 { let l=luminance(px(rgba,w,x,y)); if l>wmax {wmax=l;} } }
    let wthresh = wmax * 0.55;
    let (mut wsx, mut wsw, mut wminx, mut wmaxx) = (0.0f64, 0.0f64, u32::MAX, 0u32);
    for y in wy0..=wy1 { for x in wx0..=wx1 {
        let l=luminance(px(rgba,w,x,y));
        if l>=wthresh { let ww=(l/wmax) as f64; wsx+=x as f64*ww; wsw+=ww; if x<wminx{wminx=x;} if x>wmaxx{wmaxx=x;} }
    }}
    let wide_center = if wsw>0.0 { Some((wsx/wsw) as f32 / scale) } else { None };
    eprintln!("ANALYZE expected={expected_pt:.2} local_box=([{x0},{x1}],[{y0},{y1}]) local_max={max_lum:.2} wide_bbox_x=[{wminx},{wmaxx}] wide_center={wide_center:?}");

    if max_lum <= 10.0 { return None; }
    let thresh = max_lum * 0.45;
    let mut sum_x = 0.0f64;
    let mut sum_w = 0.0f64;
    let mut count = 0u32;
    let mut min_bx = u32::MAX;
    let mut max_bx = 0u32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = px(rgba, w, x, y);
            let l = luminance(p);
            if l >= thresh {
                let ww = (l / max_lum) as f64;
                sum_x += x as f64 * ww;
                sum_w += ww;
                count += 1;
                if x < min_bx { min_bx = x; }
                if x > max_bx { max_bx = x; }
            }
        }
    }
    eprintln!("ANALYZE local count={count} local_bbox_x=[{min_bx},{max_bx}]");
    if sum_w <= 0.0 || count < 10 { return None; }
    Some((sum_x / sum_w) as f32 / scale)
}

#[test]
fn slider_visual_knob_pixel_center_matches_ratio_2x() {
    let tmp = "/private/var/folders/2y/5w1_2qg97r7b9bfv20b4nybh0000gn/T/opencode";
    let size_pt = (400.0f32, 200.0f32);
    let size_px = ((size_pt.0 * 2.0) as u32, (size_pt.1 * 2.0) as u32);
    let mut results = Vec::new();
    for (label, value, ratio) in [("0", 0.0f64, 0.0f64), ("0.25", 25.0, 0.25), ("0.5", 50.0, 0.5), ("1", 100.0, 1.0)] {
        let mut rt = Runtime::new(
            SliderProbe { value, changes: Vec::new() },
            slider_view_fixed,
            (size_pt.0, size_pt.1),
            env(),
        );
        rt.frame(0.0);
        let track = element(&rt, "probe-slider");
        let thumb = element(&rt, "probe-slider-thumb");
        let expected = track.rect[0] + ratio as f32 * track.rect[2];
        let thumb_center = thumb.rect[0] + thumb.rect[2] / 2.0;
        let probe_diff = (thumb_center - expected).abs();

        let pixels = render_pixels(&mut rt, size_px);
        let path = format!("{tmp}/slider-audit-t0-11-r{label}.png");
        let _ = write_png(std::path::Path::new(&path), size_px.0, size_px.1, &pixels);
        let visual_center = analyze_knob_center_x(&pixels, size_px.0, size_px.1, track.rect, expected);
        let visual_diff = visual_center.map(|c| (c - expected).abs());
        eprintln!(
            "PIXEL ratio={label} track={:?} thumb_rect={:?} expected={expected:.3} thumb_probe_center={thumb_center:.3} probe_diff={probe_diff:.3} visual_center={:?} visual_diff={:?} png={path}",
            track.rect, thumb.rect, visual_center, visual_diff
        );
        results.push((label, expected, thumb_center, probe_diff, visual_center, visual_diff));
    }
    for (label, expected, thumb_center, probe_diff, visual_center, visual_diff) in &results {
        assert!(
            *probe_diff <= 1.0,
            "probe geometry off at {label}: expected {expected} got {thumb_center} diff {probe_diff}"
        );
        let vc = visual_center.unwrap_or_else(|| panic!("could not detect visual knob center at {label}"));
        let vd = visual_diff.unwrap();
        assert!(
            vd <= 1.5,
            "VISUAL knob center off at {label}: expected {expected} visual {vc} diff {vd} (>1.5pt)"
        );
    }
}

#[derive(Default)]
struct SProbe {
    value: f64,
    changes: Vec<f64>,
}

fn env_geom() -> Env {
    Env { system_dark: true, scale: 1.0 }
}

fn press_key_s(rt: &mut Runtime<SProbe>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
}

fn element_s(rt: &Runtime<SProbe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn focus_slider_s(rt: &mut Runtime<SProbe>) {
    // Tab until focused without clicking track (click would change value).
    for _ in 0..8 {
        if element_s(rt, "s").focused { return; }
        press_key_s(rt, Key::Tab);
    }
    panic!("s not focused via Tab");
}

fn view_step10_non_aligned(state: &SProbe, _cx: &mut Cx<SProbe>) -> El {
    // same config as 53@step10 test
    col().w_full().h_full().padding(20.0).child(
        crate::ui::slider::Slider::new("s", state.value)
            .min(0.0).max(100.0).step(10.0).width(200.0)
            .on_change(|s: &mut SProbe, v: f64| { s.value = v; s.changes.push(v); })
            .build(true),
    )
}

fn view_min_step10(state: &SProbe, _cx: &mut Cx<SProbe>) -> El {
    col().w_full().h_full().padding(20.0).child(
        crate::ui::slider::Slider::new("s", state.value)
            .min(0.0).max(100.0).step(10.0).width(200.0)
            .on_change(|s: &mut SProbe, v: f64| { s.value = v; s.changes.push(v); })
            .build(true),
    )
}

fn view_step0_defense(state: &SProbe, _cx: &mut Cx<SProbe>) -> El {
    col().w_full().h_full().padding(20.0).child(
        crate::ui::slider::Slider::new("s", state.value)
            .min(0.0).max(100.0).step(0.0).width(200.0)
            .on_change(|s: &mut SProbe, v: f64| { s.value = v; s.changes.push(v); })
            .build(true),
    )
}

fn view_step1(state: &SProbe, _cx: &mut Cx<SProbe>) -> El {
    col().w_full().h_full().padding(20.0).child(
        crate::ui::slider::Slider::new("s", state.value)
            .min(0.0).max(100.0).step(1.0).width(200.0)
            .on_change(|s: &mut SProbe, v: f64| { s.value = v; s.changes.push(v); })
            .build(true),
    )
}

fn view_disabled(state: &SProbe, _cx: &mut Cx<SProbe>) -> El {
    col().w_full().h_full().padding(20.0).child(
        crate::ui::slider::Slider::new("s", state.value)
            .min(0.0).max(100.0).step(1.0).width(200.0).disabled(true)
            .on_change(|s: &mut SProbe, v: f64| { s.value = v; s.changes.push(v); })
            .build(true),
    )
}

#[test]
fn keyboard_up_down_match_right_left_and_non_aligned_value_steps_by_step() {
    // Start at non-step-aligned 53 with step 10: keyboard should add/subtract step from current (no resnap).
    let mut rt = Runtime::new(SProbe { value: 53.0, changes: Vec::new() }, view_step10_non_aligned, (400.0, 200.0), env_geom());
    rt.frame(0.0);
    focus_slider_s(&mut rt);
    press_key_s(&mut rt, Key::Right);
    assert_eq!(rt.state.value, 63.0, "Right from 53 by step 10 -> 63");
    press_key_s(&mut rt, Key::Up);
    assert_eq!(rt.state.value, 73.0, "Up same as Right -> 73");
    press_key_s(&mut rt, Key::Left);
    assert_eq!(rt.state.value, 63.0, "Left -> 63");
    press_key_s(&mut rt, Key::Down);
    assert_eq!(rt.state.value, 53.0, "Down same as Left -> 53");
}

#[test]
fn keyboard_boundaries_and_step_defense() {
    // at min, Left/Down stay at min; at max, Right/Up stay at max
    let mut rt = Runtime::new(SProbe { value: 0.0, changes: Vec::new() }, view_min_step10, (400.0, 200.0), env_geom());
    rt.frame(0.0);
    focus_slider_s(&mut rt);
    press_key_s(&mut rt, Key::Left);
    assert_eq!(rt.state.value, 0.0, "Left at min clamps to min");
    press_key_s(&mut rt, Key::Down);
    assert_eq!(rt.state.value, 0.0, "Down at min clamps to min");
    press_key_s(&mut rt, Key::End);
    assert_eq!(rt.state.value, 100.0);
    press_key_s(&mut rt, Key::Right);
    assert_eq!(rt.state.value, 100.0, "Right at max clamps to max");
    press_key_s(&mut rt, Key::Up);
    assert_eq!(rt.state.value, 100.0, "Up at max clamps to max");

    // step <= 0 should be defended (treated as 1.0 by builder)
    let mut rt = Runtime::new(SProbe { value: 50.0, changes: Vec::new() }, view_step0_defense, (400.0, 200.0), env_geom());
    rt.frame(0.0);
    focus_slider_s(&mut rt);
    press_key_s(&mut rt, Key::Right);
    assert_eq!(rt.state.value, 51.0, "step 0 defended to 1.0 on keyboard step");
}

#[test]
fn drag_outside_track_reports_clamped_extrema_independent() {
    // Independent recheck of drag leaving track
    let mut rt = Runtime::new(SProbe { value: 50.0, changes: Vec::new() }, view_step1, (400.0, 200.0), env_geom());
    rt.frame(0.0);
    let t = element_s(&rt, "s");
    let y = t.rect[1] + t.rect[3] / 2.0;
    rt.pointer_down(t.rect[0] + t.rect[2] * 0.5, y);
    rt.pointer_move(t.rect[0] + t.rect[2] + 120.0, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(t.rect[0] + t.rect[2] + 120.0, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.changes.contains(&100.0), "drag right outside clamps to max: {:?}", rt.state.changes);

    let mut rt = Runtime::new(SProbe { value: 50.0, changes: Vec::new() }, view_step1, (400.0, 200.0), env_geom());
    rt.frame(0.0);
    let t = element_s(&rt, "s");
    let y = t.rect[1] + t.rect[3] / 2.0;
    rt.pointer_down(t.rect[0] + t.rect[2] * 0.5, y);
    rt.pointer_move(t.rect[0] - 120.0, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(t.rect[0] - 120.0, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.changes.contains(&0.0), "drag left outside clamps to min: {:?}", rt.state.changes);
}

#[test]
fn disabled_slider_independent_no_focus_no_report() {
    let mut rt = Runtime::new(SProbe { value: 40.0, changes: Vec::new() }, view_disabled, (400.0, 200.0), env_geom());
    rt.frame(0.0);
    let t = element_s(&rt, "s");
    assert!(!t.focusable, "disabled must not be focusable");
    rt.pointer_down(t.rect[0] + t.rect[2]*0.1, t.rect[1] + t.rect[3]/2.0);
    rt.pointer_up(t.rect[0] + t.rect[2]*0.1, t.rect[1] + t.rect[3]/2.0);
    rt.frame(1.0/60.0);
    assert!(rt.state.changes.is_empty() && rt.state.value == 40.0, "no change reported");
}
