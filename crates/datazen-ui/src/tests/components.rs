use super::*;

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn focused<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
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

// ---- Select -------------------------------------------------------------------------------

#[derive(Default)]
struct SelectProbe {
    open: bool,
    value: Option<String>,
    chooses: Vec<String>,
    closes: usize,
}

const OPTIONS: [&str; 3] = ["Alpha", "Bravo", "Charlie"];

fn select_probe(state: &SelectProbe, _cx: &mut Cx<SelectProbe>) -> El {
    let kit = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = crate::ui::select::Select::new("probe-select", &OPTIONS)
        .current(state.value.as_deref())
        .placeholder("Pick one")
        .label("Target")
        .open(state.open)
        .on_toggle(|s: &mut SelectProbe| s.open = !s.open)
        .on_close(|s: &mut SelectProbe| {
            s.open = false;
            s.closes += 1;
        })
        .on_choose(|s: &mut SelectProbe, v, _tx| {
            s.value = Some(v.to_owned());
            s.chooses.push(v.to_owned());
            s.open = false;
        })
        .build(kit, &mut layer);
    rune_kit::root(
        rune::col().w_full().h_full().padding(16.0).child(trigger),
        layer,
    )
}

/// Keys and the pointer reach `on_choose` the same way.
///
/// `Down, Down, Enter` inside the open listbox chooses the third option, exactly as pressing
/// the third row does; the value the app stores and the rounds through `on_choose` match. The
/// kit's first Down on the closed trigger is the press that opens it (and lands on the first
/// option), so the closed-state pair is covered as well: `Down, Down, Enter` opens, moves to
/// the second option and chooses it — the same row a press on the second option chooses.
#[test]
fn select_chooses_the_same_option_from_the_keyboard_and_the_pointer() {
    // Open listbox: Down, Down, Enter (the kit's arrows move the highlight) versus a press on
    // the third row. Both start from the same opened state.
    let mut keys = Runtime::new(SelectProbe::default(), select_probe, (960.0, 640.0), env());
    keys.frame(0.0);
    press(&mut keys, Key::Tab);
    press(&mut keys, Key::Text(" ".into()));
    assert!(keys.state.open, "Space opens the listbox");
    assert_eq!(
        focused(&keys).as_deref(),
        Some("option0"),
        "the open listbox holds focus"
    );
    press(&mut keys, Key::Down);
    press(&mut keys, Key::Down);
    assert_eq!(
        focused(&keys).as_deref(),
        Some("option2"),
        "two Downs land on the third option"
    );
    press(&mut keys, Key::Enter);
    assert!(
        !keys.state.open,
        "choosing closes the listbox (the app's handler)"
    );
    assert_eq!(keys.state.value.as_deref(), Some("Charlie"));
    assert_eq!(keys.state.chooses, vec!["Charlie".to_owned()]);
    assert_eq!(
        focused(&keys).as_deref(),
        Some("probe-select-trigger"),
        "focus returns to the trigger"
    );

    let mut pointer = Runtime::new(SelectProbe::default(), select_probe, (960.0, 640.0), env());
    pointer.frame(0.0);
    click(&mut pointer, "probe-select-trigger");
    assert!(
        pointer.state.open,
        "a press on the trigger opens the listbox"
    );
    assert_eq!(focused(&pointer).as_deref(), Some("option0"));
    click(&mut pointer, "option2");
    assert_eq!(pointer.state.value.as_deref(), Some("Charlie"));
    assert_eq!(pointer.state.chooses, keys.state.chooses);
    assert_eq!(pointer.state.value, keys.state.value);

    // Closed trigger: the first Down is the opening press, so the same three keys choose the
    // second option.
    let mut closed = Runtime::new(SelectProbe::default(), select_probe, (960.0, 640.0), env());
    closed.frame(0.0);
    press(&mut closed, Key::Tab);
    press(&mut closed, Key::Down);
    assert!(closed.state.open, "Down opens a closed popup button");
    assert_eq!(focused(&closed).as_deref(), Some("option0"));
    press(&mut closed, Key::Down);
    press(&mut closed, Key::Enter);
    assert_eq!(closed.state.value.as_deref(), Some("Bravo"));

    let mut second = Runtime::new(SelectProbe::default(), select_probe, (960.0, 640.0), env());
    second.frame(0.0);
    click(&mut second, "probe-select-trigger");
    click(&mut second, "option1");
    assert_eq!(second.state.value, closed.state.value);
    assert_eq!(second.state.chooses, closed.state.chooses);
}

// ---- Checkbox / Switch / RadioGroup -------------------------------------------------------

#[derive(Default)]
struct ToggleProbe {
    check: bool,
    switch: bool,
    radio: usize,
    checks: usize,
    switches: usize,
    selects: usize,
}

fn toggle_probe(state: &ToggleProbe, _cx: &mut Cx<ToggleProbe>) -> El {
    use crate::ui::toggle::{Checkbox, RadioGroup, RadioItem, Switch};
    let kit = crate::ui::kit::kit(true);
    rune::col()
        .w_full()
        .h_full()
        .padding(16.0)
        .gap(12.0)
        .child(
            Checkbox::new("probe-check", state.check)
                .label("Reuse connections")
                .on_toggle(|s: &mut ToggleProbe| {
                    s.check = !s.check;
                    s.checks += 1;
                })
                .build(kit),
        )
        .child(
            Switch::new("probe-switch", state.switch)
                .label("Announce presence")
                .on_toggle(|s: &mut ToggleProbe| {
                    s.switch = !s.switch;
                    s.switches += 1;
                })
                .build(kit),
        )
        .child(
            RadioGroup::new("probe-radio")
                .label("TLS mode")
                .item(RadioItem::new("Prefer", state.radio == 0).on_select(
                    |s: &mut ToggleProbe| {
                        s.radio = 0;
                        s.selects += 1;
                    },
                ))
                .item(RadioItem::new("Require", state.radio == 1).on_select(
                    |s: &mut ToggleProbe| {
                        s.radio = 1;
                        s.selects += 1;
                    },
                ))
                .build(kit),
        )
}

#[test]
fn switch_rows_are_whole_row_hit_targets_with_a_focus_ring() {
    let mut rt = Runtime::new(ToggleProbe::default(), toggle_probe, (960.0, 640.0), env());
    rt.frame(0.0);

    for (key, role) in [
        ("probe-check", "checkbox"),
        ("probe-switch", "switch"),
        ("probe-radio-item-0", "radio"),
    ] {
        let row = element(&rt, key);
        assert_eq!(row.role, role, "{key}: role");
        assert!(row.focusable, "{key}: focusable");
        assert!(
            row.rect[3] >= 16.0,
            "{key}: hit height {} is under 16",
            row.rect[3]
        );
    }

    // The label is part of the row, so pressing it toggles: the box itself is not the only
    // target.
    click(&mut rt, "probe-check/content/label");
    assert!(rt.state.check, "a press on the checkbox label toggles it");
    assert_eq!(rt.state.checks, 1);
    click(&mut rt, "probe-switch/label");
    assert!(rt.state.switch, "a press on the switch label toggles it");
    assert_eq!(rt.state.switches, 1);
    click(&mut rt, "probe-radio-item-1/text/label");
    assert_eq!(
        rt.state.radio, 1,
        "a press on a radio row's label selects it"
    );
    assert_eq!(rt.state.selects, 1);

    // Tab reaches each row and gives it the visible keyboard focus the kit paints its ring for.
    let mut ring = Runtime::new(ToggleProbe::default(), toggle_probe, (960.0, 640.0), env());
    ring.frame(0.0);
    press(&mut ring, Key::Tab);
    assert_eq!(focused(&ring).as_deref(), Some("probe-check"));
    assert!(
        element(&ring, "probe-check").focus_visible,
        "keyboard focus is visible"
    );
    press(&mut ring, Key::Tab);
    assert_eq!(focused(&ring).as_deref(), Some("probe-switch"));
    assert!(element(&ring, "probe-switch").focus_visible);
}

// ---- Geometry -----------------------------------------------------------------------------

const SIZES: [(f32, f32); 2] = [(960.0, 640.0), (1440.0, 816.0)];

fn page_probe(state: &SelectProbe, _cx: &mut Cx<SelectProbe>) -> El {
    use crate::ui::toggle::{Checkbox, RadioGroup, RadioItem, Switch};
    let kit = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let select = crate::ui::select::Select::new("probe-select", &OPTIONS)
        .current(Some("Bravo"))
        .label("Target")
        .open(state.open)
        .on_toggle(|s: &mut SelectProbe| s.open = !s.open)
        .on_close(|s: &mut SelectProbe| s.open = false)
        .on_choose(|s: &mut SelectProbe, v, _| s.value = Some(v.to_owned()))
        .build(kit, &mut layer);
    let page = rune::col()
        .w_full()
        .h_full()
        .padding(16.0)
        .gap(12.0)
        .child(select)
        .child(
            Checkbox::new("probe-check", true)
                .label("Reuse connections")
                .on_toggle(|s: &mut SelectProbe| s.open = !s.open)
                .build(kit),
        )
        .child(
            Switch::new("probe-switch", true)
                .label("Announce presence")
                .on_toggle(|s: &mut SelectProbe| s.open = !s.open)
                .build(kit),
        )
        .child(
            RadioGroup::new("probe-radio")
                .label("TLS mode")
                .item(RadioItem::new("Prefer", true))
                .item(RadioItem::new("Require", false))
                .build(kit),
        );
    rune_kit::root(page, layer)
}

#[test]
fn the_switch_family_and_the_open_select_stay_inside_the_window() {
    for (width, height) in SIZES {
        let mut rt = Runtime::new(
            SelectProbe {
                open: true,
                ..SelectProbe::default()
            },
            page_probe,
            (width, height),
            env(),
        );
        rt.frame(0.0);
        assert!(
            rt.find("option2").is_some(),
            "{width}x{height}: the listbox is open"
        );

        let mut outside = Vec::new();
        for e in rt.elements() {
            let (x, w) = (e.rect[0], e.rect[2]);
            if x < -0.5 || x + w > width + 0.5 {
                outside.push(format!(
                    "{} {:?} role={}",
                    e.key.unwrap_or_else(|| e.path.clone()),
                    e.rect,
                    e.role
                ));
            }
        }
        assert!(
            outside.is_empty(),
            "{width}x{height}: {} node(s) cross the window edge: {outside:?}",
            outside.len()
        );
    }
}
