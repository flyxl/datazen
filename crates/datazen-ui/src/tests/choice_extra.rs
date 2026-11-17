//! Coverage added by the independent review of T0-06 (Select) and the
//! checkbox/switch/radio base (T0-09; the track's brief calls it T0-08).
//!
//! These tests only read behaviour the wrappers already expose. They are kept out of
//! `components.rs` so a later port can keep the original test file untouched.

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
    click_at(rt, x, y);
}

fn click_at<S>(rt: &mut Runtime<S>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    let suffix = format!("/{key}");
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) || e.path == key || e.path.ends_with(&suffix))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

/// Every text run currently on screen, in document order.
fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

// ---- Select -------------------------------------------------------------------------------

const OPTIONS: [&str; 3] = ["Alpha", "Bravo", "Charlie"];
const WIDE: f32 = 960.0;
const TALL: f32 = 640.0;

/// One page builder for every select scenario, so each test only sets the state it is about.
#[derive(Default)]
struct SelectState {
    open: bool,
    disabled: bool,
    width: Option<f32>,
    value: Option<String>,
    placeholder: Option<String>,
    toggles: usize,
    closes: usize,
    chooses: Vec<String>,
}

fn select_probe(state: &SelectState, _cx: &mut Cx<SelectState>) -> El {
    let kit = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let mut select = crate::ui::select::Select::new("probe-select", &OPTIONS)
        .current(state.value.as_deref())
        .open(state.open)
        .disabled(state.disabled)
        .label("Target")
        .on_toggle(|s: &mut SelectState| {
            s.open = !s.open;
            s.toggles += 1;
        })
        .on_close(|s: &mut SelectState| {
            s.open = false;
            s.closes += 1;
        })
        .on_choose(|s: &mut SelectState, v, _tx| {
            s.value = Some(v.to_owned());
            s.chooses.push(v.to_owned());
            s.open = false;
        });
    if let Some(placeholder) = state.placeholder.as_deref() {
        select = select.placeholder(placeholder);
    }
    if let Some(width) = state.width {
        select = select.width(width);
    }
    let trigger = select.build(kit, &mut layer);
    rune_kit::root(
        rune::col().w_full().h_full().padding(16.0).child(trigger),
        layer,
    )
}

#[test]
fn a_disabled_select_cannot_be_opened_or_chosen() {
    // Closed and disabled: neither a key nor a press on the trigger reaches the app.
    let mut rt = Runtime::new(
        SelectState {
            disabled: true,
            ..SelectState::default()
        },
        select_probe,
        (WIDE, TALL),
        env(),
    );
    rt.frame(0.0);
    assert!(
        !element(&rt, "probe-select-trigger").focusable,
        "a disabled trigger is not a tab stop"
    );
    press(&mut rt, Key::Tab);
    assert_eq!(focused(&rt), None, "Tab skips the disabled trigger");
    press(&mut rt, Key::Down);
    press(&mut rt, Key::Text(" ".into()));
    click(&mut rt, "probe-select-trigger");
    assert!(!rt.state.open, "a disabled select never opens");
    assert_eq!(rt.state.toggles, 0, "its press handler is never bound");
    assert!(rt.state.chooses.is_empty());
    assert!(rt.find("option0").is_none(), "no listbox is built");
    assert!(
        rt.find("probe-select-scrim").is_none(),
        "no dismissal layer is mounted"
    );

    // Open state plus disabled: the kit returns the field alone, so the flag wins over `open`.
    let mut stateful = Runtime::new(
        SelectState {
            disabled: true,
            open: true,
            ..SelectState::default()
        },
        select_probe,
        (WIDE, TALL),
        env(),
    );
    stateful.frame(0.0);
    assert!(stateful.find("probe-select-trigger").is_some());
    assert!(
        stateful.find("option0").is_none(),
        "disabled suppresses the listbox even while the app says open"
    );
    press(&mut stateful, Key::Enter);
    assert!(
        stateful.state.chooses.is_empty(),
        "Enter cannot choose without a listbox"
    );
}

#[test]
fn a_select_with_no_value_shows_the_placeholder_and_a_choice_replaces_it() {
    let mut rt = Runtime::new(
        SelectState {
            placeholder: Some("Pick one".to_owned()),
            ..SelectState::default()
        },
        select_probe,
        (WIDE, TALL),
        env(),
    );
    rt.frame(0.0);
    let trigger = element(&rt, "probe-select-trigger");
    assert_eq!(
        trigger.label.as_deref(),
        Some("Target"),
        "the label names the field"
    );
    assert_eq!(trigger.expanded, Some(false));
    assert!(
        texts(&rt).iter().any(|t| t == "Pick one"),
        "the empty field shows the placeholder: {:?}",
        texts(&rt)
    );

    click(&mut rt, "probe-select-trigger");
    assert_eq!(rt.state.toggles, 1);
    click(&mut rt, "option1");
    assert_eq!(rt.state.value.as_deref(), Some("Bravo"));
    assert!(
        texts(&rt).iter().any(|t| t == "Bravo"),
        "the chosen option replaces the placeholder: {:?}",
        texts(&rt)
    );
    assert!(
        !texts(&rt).iter().any(|t| t == "Pick one"),
        "the placeholder is gone once a value is chosen"
    );
}

#[test]
fn the_select_width_is_the_one_asked_for_and_full_width_by_default() {
    let mut fixed = Runtime::new(
        SelectState {
            width: Some(220.0),
            value: Some("Alpha".to_owned()),
            ..SelectState::default()
        },
        select_probe,
        (WIDE, TALL),
        env(),
    );
    fixed.frame(0.0);
    let rect = element(&fixed, "probe-select-trigger").rect;
    assert!(
        (rect[2] - 220.0).abs() < 0.5,
        "width(220) produced {rect:?}"
    );

    let mut full = Runtime::new(
        SelectState {
            value: Some("Alpha".to_owned()),
            ..SelectState::default()
        },
        select_probe,
        (WIDE, TALL),
        env(),
    );
    full.frame(0.0);
    let rect = element(&full, "probe-select-trigger").rect;
    assert!(
        (rect[2] - (WIDE - 32.0)).abs() < 0.5,
        "the default fills the 16 point padded parent: {rect:?}"
    );
}

#[test]
fn closing_the_select_through_escape_or_an_outside_press_reaches_on_close() {
    let mut rt = Runtime::new(
        SelectState {
            open: true,
            value: Some("Alpha".to_owned()),
            ..SelectState::default()
        },
        select_probe,
        (WIDE, TALL),
        env(),
    );
    rt.frame(0.0);
    assert!(rt.find("option0").is_some(), "the listbox is open");

    press(&mut rt, Key::Escape);
    assert!(!rt.state.open, "Escape closes the listbox");
    assert_eq!(rt.state.closes, 1, "Escape runs the app's on_close once");
    assert_eq!(
        focused(&rt).as_deref(),
        Some("probe-select-trigger"),
        "focus returns to the trigger"
    );

    // Reopen and press the scrim, away from both the field and the listbox.
    click(&mut rt, "probe-select-trigger");
    assert!(rt.state.open);
    assert!(rt.find("probe-select-scrim").is_some());
    click_at(&mut rt, 4.0, TALL - 12.0);
    assert!(!rt.state.open, "a press outside closes the listbox");
    assert_eq!(rt.state.closes, 2, "the outside press runs on_close once");
    assert!(
        rt.state.chooses.is_empty(),
        "closing is not a choice: on_choose stays untouched"
    );
}

// ---- Checkbox / Switch / RadioGroup -------------------------------------------------------

#[derive(Default)]
struct ToggleState {
    check: bool,
    check_disabled: bool,
    check_hits: usize,
    switch: bool,
    switch_disabled: bool,
    switch_hits: usize,
    radio: usize,
    radio_disabled: bool,
    radio_hits: Vec<usize>,
}

fn toggles_page(state: &ToggleState, _cx: &mut Cx<ToggleState>) -> El {
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
                .disabled(state.check_disabled)
                .on_toggle(|s: &mut ToggleState| {
                    s.check = !s.check;
                    s.check_hits += 1;
                })
                .build(kit),
        )
        .child(
            Switch::new("probe-switch", state.switch)
                .label("Announce presence")
                .disabled(state.switch_disabled)
                .on_toggle(|s: &mut ToggleState| {
                    s.switch = !s.switch;
                    s.switch_hits += 1;
                })
                .build(kit),
        )
        .child(
            RadioGroup::new("probe-radio")
                .label("TLS mode")
                .disabled(state.radio_disabled)
                .item(RadioItem::new("Prefer", state.radio == 0).on_select(
                    |s: &mut ToggleState| {
                        s.radio = 0;
                        s.radio_hits.push(0);
                    },
                ))
                .item(RadioItem::new("Require", state.radio == 1).on_select(
                    |s: &mut ToggleState| {
                        s.radio = 1;
                        s.radio_hits.push(1);
                    },
                ))
                .item(RadioItem::new("Verify", state.radio == 2).on_select(
                    |s: &mut ToggleState| {
                        s.radio = 2;
                        s.radio_hits.push(2);
                    },
                ))
                .build(kit),
        )
}

#[test]
fn the_switch_family_rows_are_24_16_and_20_points_tall_and_clickable_across_the_gap() {
    let mut rt = Runtime::new(ToggleState::default(), toggles_page, (WIDE, TALL), env());
    rt.frame(0.0);
    for (key, height) in [
        ("probe-check", 24.0),
        ("probe-switch", 16.0),
        ("probe-radio-item-0", 20.0),
    ] {
        let row = element(&rt, key);
        assert!(
            (row.rect[3] - height).abs() < 0.5,
            "{key}: row height {} != {height}",
            row.rect[3]
        );
        assert!(row.focusable, "{key}: focusable");
    }

    // The gap between the control and the label is part of the row, not a dead strip.
    let gap_click = |rt: &mut Runtime<ToggleState>, root: &str, control: &str, label: &str| {
        let row = element(rt, root).rect;
        let control = element(rt, control).rect;
        let label = element(rt, label).rect;
        // Checkbox and radio lead with the control; the switch trails its label.
        let (left, right) = if control[0] <= label[0] {
            (control, label)
        } else {
            (label, control)
        };
        assert!(
            left[0] + left[2] <= right[0],
            "{root}: the control and the label are separate"
        );
        let x = (left[0] + left[2] + right[0]) * 0.5;
        let y = row[1] + row[3] * 0.5;
        click_at(rt, x, y);
    };
    gap_click(
        &mut rt,
        "probe-check",
        "probe-check/control",
        "probe-check/content/label",
    );
    assert!(rt.state.check, "a press in the checkbox gap toggles");
    assert_eq!(rt.state.check_hits, 1);
    gap_click(
        &mut rt,
        "probe-switch",
        "probe-switch/control",
        "probe-switch/label",
    );
    assert!(rt.state.switch, "a press in the switch gap toggles");
    assert_eq!(rt.state.switch_hits, 1);

    // The far inside edge of a row still toggles: the root, not the label, owns the press.
    let row = element(&rt, "probe-check").rect;
    click_at(&mut rt, row[0] + row[2] - 2.0, row[1] + row[3] * 0.5);
    assert!(
        !rt.state.check,
        "the checkbox row's trailing edge toggles too"
    );
    assert_eq!(rt.state.check_hits, 2);
}

#[test]
fn a_disabled_switch_ignores_keys_and_presses() {
    let mut rt = Runtime::new(
        ToggleState {
            switch_disabled: true,
            check_disabled: true,
            ..ToggleState::default()
        },
        toggles_page,
        (WIDE, TALL),
        env(),
    );
    rt.frame(0.0);
    assert!(
        !element(&rt, "probe-switch").focusable,
        "a disabled switch is not a tab stop"
    );
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused(&rt).as_deref(),
        Some("probe-radio-item-0"),
        "Tab skips the disabled checkbox and switch"
    );
    click(&mut rt, "probe-switch/label");
    click(&mut rt, "probe-switch/control");
    click(&mut rt, "probe-check/content/label");
    assert!(!rt.state.switch, "a disabled switch never flips");
    assert!(!rt.state.check, "a disabled checkbox never flips");
    assert_eq!(rt.state.switch_hits + rt.state.check_hits, 0);
}

#[test]
fn only_the_pressed_radio_row_reports_a_selection() {
    let mut rt = Runtime::new(ToggleState::default(), toggles_page, (WIDE, TALL), env());
    rt.frame(0.0);
    assert!(element(&rt, "probe-radio-item-0").item_selected);

    click(&mut rt, "probe-radio-item-1/text/label");
    assert_eq!(rt.state.radio, 1);
    assert_eq!(rt.state.radio_hits, vec![1], "one row, one handler");
    assert!(element(&rt, "probe-radio-item-1").item_selected);
    assert!(!element(&rt, "probe-radio-item-0").item_selected);

    click(&mut rt, "probe-radio-item-2/stack/control");
    assert_eq!(rt.state.radio, 2);
    assert_eq!(
        rt.state.radio_hits,
        vec![1, 2],
        "still one handler per press"
    );
}

#[test]
fn a_disabled_radio_group_disables_every_row() {
    let mut rt = Runtime::new(
        ToggleState {
            radio_disabled: true,
            ..ToggleState::default()
        },
        toggles_page,
        (WIDE, TALL),
        env(),
    );
    rt.frame(0.0);
    for i in 0..3 {
        let row = element(&rt, &format!("probe-radio-item-{i}"));
        assert!(
            !row.focusable,
            "radio row {i}: disabled rows are not tab stops"
        );
    }
    click(&mut rt, "probe-radio-item-1/text/label");
    click(&mut rt, "probe-radio-item-2/stack/control");
    assert_eq!(rt.state.radio, 0, "the checked row cannot change");
    assert!(rt.state.radio_hits.is_empty());
}

// ---- Geometry at the minimum window -------------------------------------------------------

fn page_probe(state: &SelectState, _cx: &mut Cx<SelectState>) -> El {
    use crate::ui::toggle::{Checkbox, RadioGroup, RadioItem, Switch};
    let kit = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let select = crate::ui::select::Select::new("probe-select", &OPTIONS)
        .current(Some("Bravo"))
        .label("Target")
        .open(state.open)
        .on_toggle(|s: &mut SelectState| s.open = !s.open)
        .on_close(|s: &mut SelectState| s.open = false)
        .on_choose(|s: &mut SelectState, v, _| {
            s.value = Some(v.to_owned());
            s.open = false;
        })
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
                .on_toggle(|s: &mut SelectState| s.open = !s.open)
                .build(kit),
        )
        .child(
            Switch::new("probe-switch", true)
                .label("Announce presence")
                .on_toggle(|s: &mut SelectState| s.open = !s.open)
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
fn the_choice_page_and_the_open_listbox_stay_inside_the_window() {
    for (width, height) in [(WIDE, TALL), (1440.0, 816.0)] {
        let mut rt = Runtime::new(
            SelectState {
                open: true,
                ..SelectState::default()
            },
            page_probe,
            (width, height),
            env(),
        );
        rt.frame(0.0);
        assert!(rt.find("option2").is_some(), "the listbox is open");

        // Anchored to the field's trailing edge: width - 16 padding - 160 min width, and
        // 16 + 28 field + 2 gap down. Three 28 point rows and 4 + 4 padding make 160x96.
        let list = element(&rt, "probe-select-content").rect;
        assert!(
            (list[0] - (width - 176.0)).abs() < 1.0 && (list[1] - 46.0).abs() < 1.0,
            "{width}x{height}: the listbox is anchored under the trigger: {list:?}"
        );
        assert!(
            (list[2] - 160.0).abs() < 1.0 && (list[3] - 96.0).abs() < 1.0,
            "{width}x{height}: three rows and the padding make 160x96: {list:?}"
        );

        let mut outside = Vec::new();
        for e in rt.elements() {
            let (x, y, w, h) = (e.rect[0], e.rect[1], e.rect[2], e.rect[3]);
            let over_x = x < -0.5 || x + w > width + 0.5;
            let over_y = y < -0.5 || y + h > height + 0.5;
            if over_x || over_y {
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

// ---- End-to-end journey -------------------------------------------------------------------

#[test]
fn a_whole_choice_journey_mixes_keyboard_and_pointer_over_one_state() {
    let mut rt = Runtime::new(SelectState::default(), select_probe, (WIDE, TALL), env());
    rt.frame(0.0);

    // Open with the pointer, move with the keyboard, commit with Enter. The first arrow
    // after a pointer open reveals the (invisible) highlight and the second one moves it,
    // which is the kit's macOS popup-button model.
    click(&mut rt, "probe-select-trigger");
    assert!(rt.state.open);
    press(&mut rt, Key::Down);
    press(&mut rt, Key::Down);
    press(&mut rt, Key::Enter);
    assert!(
        !rt.state.open,
        "Enter closes the listbox through the app handler"
    );
    assert_eq!(rt.state.value.as_deref(), Some("Bravo"));
    assert_eq!(rt.state.chooses, vec!["Bravo".to_owned()]);

    // Reopen and pick a different row with the pointer.
    click(&mut rt, "probe-select-trigger");
    assert!(rt.state.open, "the field opens again");
    click(&mut rt, "option2");
    assert!(!rt.state.open);
    assert_eq!(
        rt.state.chooses,
        vec!["Bravo".to_owned(), "Charlie".to_owned()],
        "both choices arrive in order"
    );
    assert_eq!(rt.state.value.as_deref(), Some("Charlie"));
    assert!(
        texts(&rt).iter().any(|t| t == "Charlie"),
        "the trigger shows the last choice: {:?}",
        texts(&rt)
    );
    assert_eq!(
        focused(&rt).as_deref(),
        Some("probe-select-trigger"),
        "focus returns to the trigger after each choice"
    );
    eprintln!(
        "journey: chooses={:?} current={:?}",
        rt.state.chooses, rt.state.value
    );
}
