//! T0-11 audit: independent probes for the temporal editor contract.
//!
//! The coder's `tests/temporal_input.rs` covers the happy path of each kind.
//! This file re-derives the contract from the React reference
//! (`packages/frontend/src/lib/cellEditorKind.ts::coerceTemporal`,
//! `CellEditorHost.tsx`) and pins the parts the coder did not:
//!
//! - the real-calendar rule (leap years, century rule) and its literal
//!   `cellEditor.invalidDate` sentence;
//! - the 24h boundary and zero-padded `HH:MM:SS` of time cells;
//! - the separator the user typed in datetime cells and their literal
//!   `cellEditor.invalidDateTime` sentence;
//! - the literal `cellEditor.notNull` sentence and "no commit, no leave";
//! - `.on_key` on both `TextField` build paths (kit and direct) and that a
//!   plain field still traverses on `Tab`;
//! - the error/hint line and the explicit width inside both windows.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit, Tx};

use crate::ui::field::TextField;
use crate::ui::temporal_input::{TemporalKind, TemporalValueInput};

/// Exact zh-CN sentences the React editor shows; they are asserted as
/// rendered text, not through `t()` again.
const INVALID_DATE: &str = "请输入有效日期 (YYYY-MM-DD)";
const INVALID_TIME: &str = "请输入有效时间 (HH:MM:SS)";
const INVALID_DATETIME: &str = "请输入有效的日期和时间 (YYYY-MM-DDTHH:MM:SS)";
const NOT_NULL: &str = "此列不允许 NULL";

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

struct Probe {
    kind: TemporalKind,
    nullable: bool,
    value: TextEdit,
    next: TextEdit,
    commits: Vec<String>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            kind: TemporalKind::Date,
            nullable: true,
            value: TextEdit::default(),
            next: TextEdit::default(),
            commits: Vec::new(),
        }
    }
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let field = TemporalValueInput::new("when", &state.value, state.kind)
        .nullable(state.nullable)
        .hint(Some("YYYY-MM-DD".to_owned()))
        .on_actions(
            |state: &mut Probe| &mut state.value,
            |state: &mut Probe, value: &str| state.commits.push(value.to_owned()),
            |_state: &mut Probe| {},
        )
        .tab_to("next-field");
    let next = TextField::new("next-field", &state.next)
        .on_edit(|state: &mut Probe| &mut state.next)
        .build(true);
    col()
        .key("page")
        .w_full()
        .h_full()
        .padding(24.0)
        .gap(12.0)
        .child(field.build(true))
        .child(next)
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn runtime(state: Probe) -> Runtime<Probe> {
    let mut rt = Runtime::new(state, view, DESIGN, env(true));
    rt.frame(0.0);
    rt
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
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

fn text_of<S>(rt: &Runtime<S>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
}

fn focused<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

fn commits_on_enter(kind: TemporalKind, typed: &str) -> Vec<String> {
    let mut rt = runtime(Probe {
        kind,
        value: TextEdit::new(typed),
        ..Probe::default()
    });
    click(&mut rt, "when-input");
    press(&mut rt, Key::Enter);
    rt.state.commits
}

#[test]
fn the_real_calendar_rule_matches_the_react_coercion() {
    for (typed, expected) in [
        ("2024-02-29", "2024-02-29"),
        ("2000-02-29", "2000-02-29"),
        ("2026-03-01", "2026-03-01"),
    ] {
        assert_eq!(
            commits_on_enter(TemporalKind::Date, typed),
            [expected],
            "{typed} is a real calendar day"
        );
    }

    for typed in [
        "2023-02-29",
        "1900-02-29",
        "2026-02-30",
        "2026-13-01",
        "2026-00-10",
    ] {
        let mut rt = runtime(Probe {
            value: TextEdit::new(typed),
            ..Probe::default()
        });
        click(&mut rt, "when-input");
        press(&mut rt, Key::Enter);
        assert!(rt.state.commits.is_empty(), "{typed} must not commit");
        assert_eq!(
            text_of(&rt, "when-error").as_deref(),
            Some(INVALID_DATE),
            "{typed} renders the React invalidDate sentence"
        );
        assert!(
            rt.state.value.focused,
            "{typed} keeps the caret in the cell"
        );
    }
}

#[test]
fn time_hours_are_bounded_and_canonicalised_to_zero_padded_hh_mm_ss() {
    for (typed, expected) in [
        ("9:05", "09:05:00"),
        ("0:00", "00:00:00"),
        ("23:59", "23:59:00"),
        ("23:59:59", "23:59:59"),
    ] {
        assert_eq!(
            commits_on_enter(TemporalKind::Time, typed),
            [expected],
            "{typed} canonicalises"
        );
    }

    for typed in ["24:00", "23:60", "23:59:60", "1:5", ":30", "12:"] {
        let mut rt = runtime(Probe {
            kind: TemporalKind::Time,
            value: TextEdit::new(typed),
            ..Probe::default()
        });
        click(&mut rt, "when-input");
        press(&mut rt, Key::Enter);
        assert!(rt.state.commits.is_empty(), "{typed} must not commit");
        assert_eq!(
            text_of(&rt, "when-error").as_deref(),
            Some(INVALID_TIME),
            "{typed} renders the React invalidTime sentence"
        );
        assert!(
            rt.state.value.focused,
            "{typed} keeps the caret in the cell"
        );
    }
}

#[test]
fn datetime_keeps_the_typed_separator_and_checks_both_halves() {
    for typed in [
        "2026-03-01 7:02:03",
        "2026-03-01T7:02:03",
        "2026-03-01T07:02",
    ] {
        assert_eq!(
            commits_on_enter(TemporalKind::DateTime, typed),
            [typed],
            "{typed} commits verbatim (separator preserved)"
        );
    }

    for typed in [
        "2026-03-01T24:00",
        "2023-02-29T10:00",
        "2026-03-01T10:60",
        "2026-03-01",
    ] {
        let mut rt = runtime(Probe {
            kind: TemporalKind::DateTime,
            value: TextEdit::new(typed),
            ..Probe::default()
        });
        click(&mut rt, "when-input");
        press(&mut rt, Key::Enter);
        assert!(rt.state.commits.is_empty(), "{typed} must not commit");
        assert_eq!(
            text_of(&rt, "when-error").as_deref(),
            Some(INVALID_DATETIME),
            "{typed} renders the React invalidDateTime sentence"
        );
        assert!(
            rt.state.value.focused,
            "{typed} keeps the caret in the cell"
        );
    }
}

#[test]
fn not_null_refuses_the_empty_value_on_enter_and_tab_without_leaving() {
    let mut rt = runtime(Probe {
        nullable: false,
        ..Probe::default()
    });
    // `validation_error` runs during build, so the refusal is already visible
    // before any key; React clears it only after a failed attempt.
    assert_eq!(
        text_of(&rt, "when-error").as_deref(),
        Some(NOT_NULL),
        "the refusal line is the React notNull sentence"
    );

    click(&mut rt, "when-input");
    press(&mut rt, Key::Enter);
    assert!(rt.state.commits.is_empty(), "Enter never commits NULL here");
    assert!(rt.state.value.focused, "Enter keeps the caret in the cell");

    press(&mut rt, Key::Tab);
    assert!(rt.state.commits.is_empty(), "Tab never commits NULL here");
    assert!(rt.state.value.focused, "Tab does not leave an invalid cell");
    assert!(
        !rt.state.next.focused,
        "focus did not escape to the next field"
    );
    assert_eq!(text_of(&rt, "when-error").as_deref(), Some(NOT_NULL));
}

#[test]
fn nullable_empty_commits_the_empty_string_on_enter_and_tab() {
    let mut enter = runtime(Probe::default());
    click(&mut enter, "when-input");
    press(&mut enter, Key::Enter);
    assert_eq!(enter.state.commits, [""], "Enter commits empty as NULL");
    assert!(!enter.state.value.focused, "Enter blurs");
    assert!(
        enter.find("when-error").is_none(),
        "no error for a legal NULL"
    );

    let mut tab = runtime(Probe::default());
    click(&mut tab, "when-input");
    press(&mut tab, Key::Tab);
    assert_eq!(tab.state.commits, [""], "Tab commits empty as NULL");
    assert!(tab.state.next.focused, "Tab moves to the configured target");
}

#[test]
fn on_key_is_wired_on_the_kit_build_path() {
    let mut rt = runtime(Probe {
        value: TextEdit::new("2026-03-01"),
        ..Probe::default()
    });
    click(&mut rt, "when-input");
    press(&mut rt, Key::Tab);
    assert_eq!(rt.state.commits, ["2026-03-01"], "the kit path commits");
    assert!(rt.state.next.focused, "the kit path moves focus");

    let mut bad = runtime(Probe {
        value: TextEdit::new("2026-02-30"),
        ..Probe::default()
    });
    click(&mut bad, "when-input");
    press(&mut bad, Key::Tab);
    assert!(bad.state.commits.is_empty(), "invalid Tab does not commit");
    assert!(bad.state.value.focused, "invalid Tab stays on the field");
    assert!(!bad.state.next.focused, "invalid Tab does not move focus");
}

#[test]
#[ignore = "DEFECT 464bbe786: TextField::on_key on the direct (ink) build path is wired with El::on_key, which Rune never calls for a focused text field (its keys go to on_input_key); un-ignore after the wiring change"]
fn on_key_is_wired_on_the_direct_build_path_too() {
    struct Direct {
        value: TextEdit,
        next: TextEdit,
        key_ran: bool,
    }

    fn direct_view(state: &Direct, _cx: &mut Cx<Direct>) -> El {
        // `.ink` is what selects `TextField::build_direct`. The handler also
        // records that it ran: Rune's own Tab traversal reaches `next-field`
        // anyway, so the flag — not the focus alone — proves the wiring.
        let keyed = |state: &mut Direct, event: &KeyEvent, tx: &mut Tx| {
            if event.key == Key::Tab {
                state.key_ran = true;
                tx.focus("next-field", true);
                true
            } else {
                false
            }
        };
        let first = TextField::new("direct", &state.value)
            .ink(rune::Color::WHITE)
            .on_edit(|state: &mut Direct| &mut state.value)
            .on_key(keyed)
            .build(true);
        let next = TextField::new("next-field", &state.next)
            .on_edit(|state: &mut Direct| &mut state.next)
            .build(true);
        col()
            .key("page")
            .w_full()
            .h_full()
            .padding(24.0)
            .gap(12.0)
            .child(first)
            .child(next)
    }

    let mut rt = Runtime::new(
        Direct {
            value: TextEdit::default(),
            next: TextEdit::default(),
            key_ran: false,
        },
        direct_view,
        DESIGN,
        env(true),
    );
    rt.frame(0.0);
    click(&mut rt, "direct");
    assert!(
        rt.state.value.focused,
        "the direct field takes the caret on click"
    );
    press(&mut rt, Key::Tab);
    assert!(
        rt.state.key_ran,
        "the direct path runs the caller's on_key handler"
    );
    assert!(
        rt.state.next.focused,
        "the direct path consumes Tab and focuses the target"
    );
    assert!(
        !rt.state.value.focused,
        "the direct path leaves the first field"
    );
}

#[test]
fn a_plain_field_still_traverses_on_tab() {
    struct Pair {
        first: TextEdit,
        second: TextEdit,
    }

    fn pair_view(state: &Pair, _cx: &mut Cx<Pair>) -> El {
        let first = TextField::new("first", &state.first)
            .on_edit(|state: &mut Pair| &mut state.first)
            .build(true);
        let second = TextField::new("second", &state.second)
            .on_edit(|state: &mut Pair| &mut state.second)
            .build(true);
        col()
            .key("page")
            .w_full()
            .h_full()
            .padding(24.0)
            .gap(12.0)
            .child(first)
            .child(second)
    }

    let mut rt = Runtime::new(
        Pair {
            first: TextEdit::default(),
            second: TextEdit::default(),
        },
        pair_view,
        DESIGN,
        env(true),
    );
    rt.frame(0.0);
    click(&mut rt, "first");
    press(&mut rt, Key::Tab);
    assert!(
        rt.state.second.focused,
        "a field without on_key keeps Rune's default Tab traversal"
    );
}

#[test]
fn the_error_and_hint_lines_stay_inside_both_windows_and_themes() {
    for dark in [true, false] {
        for (width, height) in [MIN, DESIGN] {
            let mut rt = Runtime::new(
                Probe {
                    value: TextEdit::new("2026-02-30"),
                    ..Probe::default()
                },
                view,
                (width, height),
                env(dark),
            );
            rt.frame(0.0);
            let context = format!("temporal {width}x{height} dark={dark}");
            assert!(
                rt.find("when-error").is_some(),
                "{context}: the error line renders"
            );
            assert!(
                rt.find("when-hint").is_none(),
                "{context}: the hint gives way to the error"
            );
            for element in rt.elements() {
                if element.exiting {
                    continue;
                }
                let [x, y, w, h] = element.rect;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= width + 0.5 && y + h <= height + 0.5,
                    "{context}: key={:?} role={} rect={:?} crosses the window",
                    element.key,
                    element.role,
                    element.rect
                );
            }
        }
    }

    let valid = runtime(Probe {
        value: TextEdit::new("2026-02-28"),
        ..Probe::default()
    });
    assert!(valid.find("when-error").is_none(), "valid: no error line");
    assert_eq!(
        text_of(&valid, "when-hint").as_deref(),
        Some("YYYY-MM-DD"),
        "valid: the format hint renders"
    );
    assert_eq!(
        focused(&valid),
        None,
        "nothing takes focus before the editor does"
    );
}

#[test]
fn an_explicit_width_reaches_the_field_and_the_root() {
    struct Sized {
        value: TextEdit,
    }

    fn sized_view(state: &Sized, _cx: &mut Cx<Sized>) -> El {
        let field = TemporalValueInput::new("when", &state.value, TemporalKind::Date)
            .width(320.0)
            .height(36.0)
            .hint(Some("YYYY-MM-DD".to_owned()))
            .on_actions(
                |state: &mut Sized| &mut state.value,
                |_state: &mut Sized, _value: &str| {},
                |_state: &mut Sized| {},
            )
            .build(true);
        col()
            .key("page")
            .w_full()
            .h_full()
            .padding(24.0)
            .items_start()
            .child(field)
    }

    let mut rt = Runtime::new(
        Sized {
            value: TextEdit::new("2026-02-28"),
        },
        sized_view,
        DESIGN,
        env(true),
    );
    rt.frame(0.0);
    let field = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("when-input"))
        .expect("the field is laid out");
    assert!(
        (field.rect[2] - 320.0).abs() < 0.5,
        "explicit width: {:?}",
        field.rect
    );
    assert!(
        (field.rect[3] - 36.0).abs() < 0.5,
        "explicit height: {:?}",
        field.rect
    );
    let root = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("when"))
        .expect("the root is laid out");
    assert!(
        (root.rect[2] - 320.0).abs() < 0.5,
        "the error/hint column follows the width: {:?}",
        root.rect
    );
}
