use rune::{Cx, El, Env, Runtime, Tx};

use crate::ui::copyable::CopyableError;

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press_sync<S: 'static>(rt: &mut Runtime<S>) {
    click(rt, "sync");
}

#[derive(Default)]
struct Probe {
    copied_calls: usize,
    press_clock: Option<f64>,
    copied_until: Option<f64>,
    now: f64,
}

fn copyable_view_with_callback(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let copied = state.copied_until.map_or(false, |u| state.now < u);
    let w = CopyableError::new("c", "exact message")
        .copy_button(true)
        .copied(copied)
        .on_copy_tx(|s: &mut Probe, tx: &mut Tx| {
            s.copied_calls += 1;
            s.press_clock = Some(tx.clock());
            s.now = tx.clock();
            s.copied_until = Some(tx.clock() + 1.5);
        })
        .build::<Probe>(k);
    let sync = rune::row()
        .key("sync")
        .role("button")
        .label("sync")
        .w(1.0)
        .h(1.0)
        .on_press(|_s: &mut Probe| {});
    rune::col().w(480.0).h(120.0).child(sync).child(w)
}

fn copyable_view_no_callback(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let w = CopyableError::new("c", "exact message")
        .copy_button(true)
        .copied(false)
        .build::<Probe>(k);
    rune::col().w(480.0).h(120.0).child(w)
}

fn label_text(rt: &Runtime<Probe>) -> String {
    element(rt, "c-copy").label.unwrap_or_default()
}

#[test]
fn copied_true_and_false_render_expected_labels() {
    fn view_true(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
        let k = crate::ui::kit::kit(true);
        rune::col().w(480.0).h(120.0).child(
            CopyableError::new("c", "msg")
                .copy_button(true)
                .copied(true)
                .build::<Probe>(k),
        )
    }
    fn view_false(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
        let k = crate::ui::kit::kit(true);
        rune::col().w(480.0).h(120.0).child(
            CopyableError::new("c", "msg")
                .copy_button(true)
                .copied(false)
                .build::<Probe>(k),
        )
    }
    let mut rt = Runtime::new(Probe::default(), view_true, (480.0, 240.0), env());
    rt.frame(0.0);
    let l = label_text(&rt);
    assert!(
        l.contains("copied") || l.contains("已复制"),
        "copied(true) should show copied label: {l}"
    );

    let mut rt = Runtime::new(Probe::default(), view_false, (480.0, 240.0), env());
    rt.frame(0.0);
    let l = label_text(&rt);
    assert!(
        l.contains("copy") || l.contains("复制"),
        "copied(false) should show copy label: {l}"
    );
    assert!(
        !(l.contains("copied") || l.contains("已复制")),
        "copied(false) must not show copied label: {l}"
    );
}

#[test]
fn press_schedules_wake_deadline_1500ms_after_press_clock() {
    let mut rt = Runtime::new(Probe::default(), copyable_view_with_callback, (480.0, 240.0), env());
    rt.frame(0.0);
    click(&mut rt, "c-copy");
    let press_clock = rt.state.press_clock.expect("callback observed tx.clock()");
    let deadline = rt
        .wake_deadline()
        .expect("copy press must schedule a pending wake for feedback revert");
    let delta = deadline - press_clock;
    assert!(
        (delta - 1.5).abs() <= 0.01,
        "wake must be scheduled ~1500ms after press clock: press={press_clock} deadline={deadline} delta={delta}"
    );
}

#[test]
fn copied_until_deadline_is_1500ms_and_boundary_reverts_at_expiry() {
    let mut rt = Runtime::new(Probe::default(), copyable_view_with_callback, (480.0, 240.0), env());
    rt.frame(0.0);
    click(&mut rt, "c-copy");
    rt.frame(0.0);

    let press_clock = rt.state.press_clock.expect("press clock");
    let until = rt.state.copied_until.expect("copied_until set");
    assert!(
        (until - press_clock - 1.5).abs() <= 1e-9,
        "copied_until must be press_clock + 1.5s: press={press_clock} until={until}"
    );
    let l = label_text(&rt);
    assert!(l.contains("copied") || l.contains("已复制"), "immediately copied: {l}");

    rt.state.now = until - 0.001;
    press_sync(&mut rt);
    let l = label_text(&rt);
    assert!(
        l.contains("copied") || l.contains("已复制"),
        "just before expiry must still be copied: {l}"
    );

    rt.state.now = until;
    press_sync(&mut rt);
    let l = label_text(&rt);
    assert!(l.contains("copy") || l.contains("复制"), "at expiry boundary must revert to copy: {l}");
    assert!(
        !(l.contains("copied") || l.contains("已复制")),
        "at expiry must not show copied: {l}"
    );
}

#[test]
fn second_press_retimes_deadline_not_accumulates() {
    let mut rt = Runtime::new(Probe::default(), copyable_view_with_callback, (480.0, 240.0), env());
    rt.frame(0.0);
    click(&mut rt, "c-copy");
    let first_until = rt.state.copied_until.expect("first until");
    let first_clock = rt.state.press_clock.expect("first clock");

    rt.frame(0.8);
    click(&mut rt, "c-copy");
    let second_clock = rt.state.press_clock.expect("second clock");
    let second_until = rt.state.copied_until.expect("second until");

    assert!(second_clock > first_clock, "second press should be later in time");
    assert!(
        (second_until - second_clock - 1.5).abs() <= 1e-9,
        "second press must re-time to clock+1.5: second_clock={second_clock} second_until={second_until}"
    );
    assert!(second_until > first_until, "re-timed deadline must extend beyond first");
    assert!(
        (second_until - first_until - (second_clock - first_clock)).abs() <= 1e-9,
        "deadline must move by elapsed between presses (re-time)"
    );

    rt.state.now = second_until - 0.5;
    press_sync(&mut rt);
    let l = label_text(&rt);
    assert!(l.contains("copied") || l.contains("已复制"), "before re-timed expiry still copied: {l}");
    rt.state.now = second_until + 0.01;
    press_sync(&mut rt);
    let l = label_text(&rt);
    assert!(l.contains("copy") || l.contains("复制"), "after re-timed expiry reverted: {l}");
}

#[test]
fn press_without_on_copy_callback_still_copies_and_schedules_wake() {
    let clipboard = rune::clipboard::memory();
    let mut rt = Runtime::new(Probe::default(), copyable_view_no_callback, (480.0, 240.0), env());
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);

    let before = rt.clock.max(0.0);
    click(&mut rt, "c-copy");
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("exact message"),
        "press without callback must still copy exact message"
    );
    let deadline = rt
        .wake_deadline()
        .expect("press without callback must still schedule feedback wake");
    let delta = deadline - before;
    assert!(
        delta > 1.4 && delta < 1.7,
        "wake without callback must be scheduled ~1.5s out: before={before} deadline={deadline} delta={delta}"
    );
}

#[test]
#[ignore = "pending wake deadline is observable via Runtime::wake_deadline() in this runtime and is pinned above; keep only if runtime API changes"]
fn wake_deadline_observability_placeholder() {
    // Runtime::wake_deadline() is exposed and asserted in:
    // - press_schedules_wake_deadline_1500ms_after_press_clock
    // - press_without_on_copy_callback_still_copies_and_schedules_wake
}