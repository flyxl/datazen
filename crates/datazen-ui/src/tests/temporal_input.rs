//! T0-11: the temporal value editor's typing, validation and commit contract.
//!
//! The probe mirrors a data-grid cell host: `on_actions` records commits and
//! restores the cell on cancel, and a second field named `next-field` proves
//! that `Tab` moves focus after committing.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::temporal_input::{TemporalKind, TemporalValueInput};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

struct Probe {
    kind: TemporalKind,
    nullable: bool,
    width: Option<f32>,
    height: f32,
    value: TextEdit,
    next: TextEdit,
    commits: Vec<String>,
    cancels: u32,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            kind: TemporalKind::Date,
            nullable: true,
            width: None,
            height: 28.0,
            value: TextEdit::default(),
            next: TextEdit::default(),
            commits: Vec::new(),
            cancels: 0,
        }
    }
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut field = TemporalValueInput::new("when", &state.value, state.kind)
        .nullable(state.nullable)
        .height(state.height)
        .hint(Some("YYYY-MM-DD".to_owned()))
        .placeholder(format!("temporal-{}", state.kind as u8))
        .on_actions(
            |state: &mut Probe| &mut state.value,
            |state: &mut Probe, value: &str| state.commits.push(value.to_owned()),
            |state: &mut Probe| {
                state.cancels += 1;
                state.value.text = "2026-01-01".to_owned();
            },
        )
        .tab_to("next-field");
    if let Some(width) = state.width {
        field = field.width(width);
    }
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

fn text_of<S>(rt: &Runtime<S>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
}

#[test]
fn a_valid_date_commits_on_enter_and_blurs() {
    let mut rt = runtime(Probe {
        value: TextEdit::new("2026-09-20"),
        ..Probe::default()
    });
    click(&mut rt, "when-input");
    press(&mut rt, Key::Enter);

    assert_eq!(
        rt.state.commits,
        ["2026-09-20"],
        "Enter commits the typed date"
    );
    assert!(!rt.state.value.focused, "a committed value blurs the cell");
    assert!(
        rt.find("when-error").is_none(),
        "a valid value shows no error"
    );
}

#[test]
fn an_invalid_date_shows_the_keyed_error_and_does_not_commit() {
    let mut rt = runtime(Probe {
        value: TextEdit::new("2026-02-30"),
        ..Probe::default()
    });
    click(&mut rt, "when-input");
    press(&mut rt, Key::Enter);

    assert!(
        rt.state.commits.is_empty(),
        "an invalid date is never committed"
    );
    assert_eq!(
        text_of(&rt, "when-error").as_deref(),
        Some(t("cellEditor.invalidDate")),
        "the inline error uses the React locale key"
    );
    assert!(
        rt.state.value.focused,
        "an invalid value keeps the caret in the cell"
    );
}

#[test]
fn a_non_nullable_empty_value_is_refused_with_not_null() {
    let mut rt = runtime(Probe {
        nullable: false,
        ..Probe::default()
    });
    click(&mut rt, "when-input");
    press(&mut rt, Key::Enter);

    assert!(
        rt.state.commits.is_empty(),
        "NOT NULL cannot commit an empty value"
    );
    assert_eq!(
        text_of(&rt, "when-error").as_deref(),
        Some(t("cellEditor.notNull")),
        "the refusal uses the React cellEditor.notNull message"
    );
}

#[test]
fn a_nullable_empty_value_commits_as_null() {
    let mut rt = runtime(Probe::default());
    click(&mut rt, "when-input");
    press(&mut rt, Key::Enter);

    assert_eq!(
        rt.state.commits,
        [""],
        "an empty nullable cell commits NULL"
    );
    assert!(!rt.state.value.focused);
    assert!(rt.find("when-error").is_none());
}

#[test]
fn escape_cancels_and_restores_the_original_value() {
    let mut rt = runtime(Probe {
        value: TextEdit::new("2026-01-01"),
        ..Probe::default()
    });
    click(&mut rt, "when-input");
    rt.paste("2026-13-40");
    rt.frame(1.0 / 60.0);
    assert_eq!(
        text_of(&rt, "when-error").as_deref(),
        Some(t("cellEditor.invalidDate"))
    );
    press(&mut rt, Key::Escape);

    assert_eq!(
        rt.state.cancels, 1,
        "Escape reaches the host cancel callback"
    );
    assert_eq!(
        rt.state.value.text, "2026-01-01",
        "cancel restores the original text"
    );
    assert!(!rt.state.value.focused, "cancel blurs the cell");
    assert!(
        rt.find("when-error").is_none(),
        "the restored value is valid again"
    );
}

#[test]
fn tab_commits_and_moves_focus_to_the_tab_target() {
    let mut rt = runtime(Probe {
        value: TextEdit::new("2026-03-01"),
        ..Probe::default()
    });
    click(&mut rt, "when-input");
    press(&mut rt, Key::Tab);

    assert_eq!(rt.state.commits, ["2026-03-01"], "Tab commits like Enter");
    assert!(
        rt.state.next.focused,
        "Tab moves focus to the configured target"
    );
    assert_eq!(
        focused(&rt).as_deref(),
        Some("input"),
        "focus lands inside the next text box"
    );
}

#[test]
fn tab_with_an_invalid_value_stays_put_and_consumes_the_key() {
    let mut rt = runtime(Probe {
        value: TextEdit::new("2026-02-30"),
        ..Probe::default()
    });
    click(&mut rt, "when-input");
    press(&mut rt, Key::Tab);

    assert!(
        rt.state.commits.is_empty(),
        "Tab does not commit an invalid value"
    );
    assert!(
        rt.state.value.focused,
        "the caret stays in the invalid cell"
    );
    assert!(
        !rt.state.next.focused,
        "focus does not escape to the next field"
    );
    assert_eq!(
        text_of(&rt, "when-error").as_deref(),
        Some(t("cellEditor.invalidDate"))
    );
}

#[test]
fn time_values_canonicalise_and_datetimes_keep_the_typed_text() {
    let mut time = runtime(Probe {
        kind: TemporalKind::Time,
        value: TextEdit::new("9:05"),
        ..Probe::default()
    });
    click(&mut time, "when-input");
    press(&mut time, Key::Enter);
    assert_eq!(
        time.state.commits,
        ["09:05:00"],
        "time commits canonical HH:MM:SS"
    );

    let mut datetime = runtime(Probe {
        kind: TemporalKind::DateTime,
        value: TextEdit::new("2026-03-01 7:02:03"),
        ..Probe::default()
    });
    click(&mut datetime, "when-input");
    press(&mut datetime, Key::Enter);
    assert_eq!(
        datetime.state.commits,
        ["2026-03-01 7:02:03"],
        "datetime commits the typed separator and digits verbatim"
    );
}

#[test]
fn the_field_stays_inside_the_window_at_both_sizes_and_themes() {
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
            let theme = if dark { "dark" } else { "light" };
            let context = format!("temporal {width}x{height} {theme}");
            let elements = rt.elements();
            assert!(!elements.is_empty(), "{context}: the page rendered nothing");
            for element in elements {
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
}

#[test]
fn explicit_width_and_height_reach_the_text_field() {
    let mut rt = runtime(Probe {
        width: Some(320.0),
        height: 36.0,
        ..Probe::default()
    });
    rt.frame(0.0);
    let field = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("when-input"))
        .expect("the text field is laid out");
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
}
