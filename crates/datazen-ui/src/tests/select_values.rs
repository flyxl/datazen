//! A select whose shown label differs from the value it reports: the React side's
//! `{value: "558:presto_afi_data", label: "presto_afi_data"}` options.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

const LABELS: [&str; 3] = ["Alpha", "Bravo", "Charlie"];
const VALUES: [&str; 3] = ["556:alpha", "558:bravo", "560:charlie"];

#[derive(Default)]
struct ValuesState {
    open: bool,
    chooses: Vec<String>,
}

fn probe(state: &ValuesState, _cx: &mut Cx<ValuesState>) -> El {
    let kit = crate::ui::kit::kit(true);
    let mut layer = Vec::new();
    let trigger = crate::ui::select::Select::new("probe-values", &LABELS)
        .values(&VALUES)
        .label("Target")
        .open(state.open)
        .on_toggle(|s: &mut ValuesState| s.open = !s.open)
        .on_close(|s: &mut ValuesState| s.open = false)
        .on_choose(|s: &mut ValuesState, value, _tx| {
            s.chooses.push(value.to_owned());
            s.open = false;
        })
        .build(kit, &mut layer);
    rune_kit::root(
        rune::col().w_full().h_full().padding(16.0).child(trigger),
        layer,
    )
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn a_press_on_a_row_reports_the_value_not_the_label() {
    let mut rt = Runtime::new(ValuesState::default(), probe, (960.0, 640.0), env());
    rt.frame(0.0);

    click(&mut rt, "probe-values-trigger");
    assert!(rt.find("option1").is_some(), "the listbox is open");

    click(&mut rt, "option1");
    assert_eq!(
        rt.state.chooses,
        vec!["558:bravo".to_string()],
        "the press reports the option's value, not the label it shows"
    );
    assert!(!rt.state.open, "the probe closes the listbox on a choice");
}

#[test]
fn the_keyboard_reports_the_value_of_the_row_it_lands_on() {
    let mut rt = Runtime::new(ValuesState::default(), probe, (960.0, 640.0), env());
    rt.frame(0.0);

    // The kit's popup-button model: a pointer open leaves the highlight invisible, the
    // first Down reveals it on the first row, the second moves to the second row.
    click(&mut rt, "probe-values-trigger");
    assert!(rt.state.open, "the trigger opens the listbox");
    for _ in 0..2 {
        rt.key(KeyEvent {
            key: Key::Down,
            mods: Mods::default(),
        });
        rt.frame(1.0 / 60.0);
    }
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);

    assert_eq!(
        rt.state.chooses,
        vec!["558:bravo".to_string()],
        "Enter reports the value behind the highlighted label"
    );
}
