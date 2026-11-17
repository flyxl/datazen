//! Audit probes for the `TextField` → `rune-kit::TextInput` delegation
//! (`e80bfb243`): the whole box must stay a hit target (top/bottom/right
//! padding), the line must stay centred in 28/36-point boxes, an unbound field
//! must still trip the `on_edit` guard, and caller chrome must survive the
//! kit's own shell.
use super::*;

fn probe_env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn element(rt: &Runtime<AppState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} rendered"))
}

fn field_rect(rt: &Runtime<AppState>, key: &str) -> [f32; 4] {
    element(rt, key).rect
}

fn text_of(edit: &rune::TextEdit) -> String {
    edit.display().0.to_string()
}

/// A press in the box's own padding (not the inner editor's 15pt line) must
/// route focus to the editor; `x` is the horizontal offset inside the box.
fn press_and_type(rt: &mut Runtime<AppState>, key: &str, x_off: f32, y_off: f32, expected: &str) {
    rt.frame(0.0);
    let rect = field_rect(rt, key);
    let x = rect[0] + x_off;
    let y = rect[1] + y_off;
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste(expected);
    rt.frame(0.0);
    assert_eq!(
        text_of(&rt.state.connection_search),
        expected,
        "a press at ({x_off}, {y_off}) inside a {key} field must focus it and take input"
    );
}

#[test]
fn top_padding_press_focuses_and_takes_input() {
    fn probe(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        crate::ui::field::TextField::new("audit-top", &state.connection_search)
            .height(28.0)
            .width(220.0)
            .on_edit(|state: &mut AppState| &mut state.connection_search)
            .build(true)
    }
    let mut rt = Runtime::new(AppState::default(), probe, (400.0, 160.0), probe_env());
    press_and_type(&mut rt, "audit-top", 20.0, 3.0, "t");
}

#[test]
fn bottom_padding_press_focuses_and_takes_input() {
    fn probe(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        crate::ui::field::TextField::new("audit-bottom", &state.connection_search)
            .height(28.0)
            .width(220.0)
            .on_edit(|state: &mut AppState| &mut state.connection_search)
            .build(true)
    }
    let mut rt = Runtime::new(AppState::default(), probe, (400.0, 160.0), probe_env());
    press_and_type(&mut rt, "audit-bottom", 20.0, 25.0, "b");
}

#[test]
fn right_padding_press_focuses_and_takes_input() {
    fn probe(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        crate::ui::field::TextField::new("audit-right", &state.connection_search)
            .height(28.0)
            .width(220.0)
            .on_edit(|state: &mut AppState| &mut state.connection_search)
            .build(true)
    }
    let mut rt = Runtime::new(AppState::default(), probe, (400.0, 160.0), probe_env());
    // The inner editor ends 8pt before the box's right edge; x-3 lands in the
    // outer row's right padding.
    press_and_type(&mut rt, "audit-right", 217.0, 14.0, "r");
}

#[test]
fn box_padding_press_focuses_a_36pt_field() {
    fn top(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        crate::ui::field::TextField::new("audit-top36", &state.connection_search)
            .height(36.0)
            .width(220.0)
            .on_edit(|state: &mut AppState| &mut state.connection_search)
            .build(true)
    }
    fn bottom(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        crate::ui::field::TextField::new("audit-bottom36", &state.connection_search)
            .height(36.0)
            .width(220.0)
            .on_edit(|state: &mut AppState| &mut state.connection_search)
            .build(true)
    }
    let mut rt = Runtime::new(AppState::default(), top, (400.0, 160.0), probe_env());
    press_and_type(&mut rt, "audit-top36", 20.0, 3.0, "T");
    let mut rt = Runtime::new(AppState::default(), bottom, (400.0, 160.0), probe_env());
    press_and_type(&mut rt, "audit-bottom36", 20.0, 33.0, "B");
}

#[test]
fn line_stays_centred_in_28_and_36_point_boxes() {
    fn two_fields(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        rune::col()
            .w_full()
            .items_start()
            .gap(12.0)
            .child(
                crate::ui::field::TextField::new("audit-c28", &state.connection_search)
                    .height(28.0)
                    .width(220.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_search)
                    .build(true),
            )
            .child(
                crate::ui::field::TextField::new("audit-c36", &state.connection_search)
                    .height(36.0)
                    .width(220.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_search)
                    .build(true),
            )
    }
    let mut rt = Runtime::new(AppState::default(), two_fields, (400.0, 200.0), probe_env());
    rt.frame(0.0);
    for (key, height) in [("audit-c28", 28.0), ("audit-c36", 36.0)] {
        let box_rect = field_rect(&rt, key);
        assert!(
            (box_rect[3] - height).abs() <= 0.5,
            "{key} fills its requested {height}pt height; got {}",
            box_rect[3]
        );
        let inputs: Vec<_> = rt
            .elements()
            .into_iter()
            .filter(|e| e.key.as_deref() == Some("input"))
            .collect();
        // The inner editor of the field whose box encloses it.
        let input = inputs
            .iter()
            .find(|e| e.rect[1] >= box_rect[1] && e.rect[1] + e.rect[3] <= box_rect[1] + box_rect[3])
            .unwrap_or_else(|| panic!("{key} has an inner editor"));
        let box_centre = box_rect[1] + box_rect[3] / 2.0;
        let line_centre = input.rect[1] + input.rect[3] / 2.0;
        assert!(
            (line_centre - box_centre).abs() <= 0.5,
            "{key}: line centre {line_centre} vs box centre {box_centre} (rects {box_rect:?} / {:?})",
            input.rect
        );
    }
}

#[test]
#[should_panic(expected = "a text field without on_edit cannot take focus")]
fn a_field_without_on_edit_trips_the_guard() {
    let state = AppState::default();
    let _ = crate::ui::field::TextField::new("audit-unbound", &state.connection_search)
        .height(28.0)
        .build(true);
}

#[test]
fn grow_field_takes_the_row_remainder() {
    fn row(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        rune::row()
            .w_full()
            .child(
                crate::ui::field::TextField::new("audit-grow", &state.connection_search)
                    .height(28.0)
                    .grow()
                    .on_edit(|state: &mut AppState| &mut state.connection_search)
                    .build(true),
            )
            .child(rune::el().w(100.0))
    }
    let mut rt = Runtime::new(AppState::default(), row, (400.0, 120.0), probe_env());
    rt.frame(0.0);
    let rect = field_rect(&rt, "audit-grow");
    assert!(
        (rect[2] - 300.0).abs() <= 1.0,
        "grow field takes the 300pt left by the fixed child; got width {}",
        rect[2]
    );
}

#[test]
fn default_field_fills_its_parent() {
    fn col(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        rune::col().w_full().items_start().child(
            crate::ui::field::TextField::new("audit-full", &state.connection_search)
                .height(28.0)
                .on_edit(|state: &mut AppState| &mut state.connection_search)
                .build(true),
        )
    }
    let mut rt = Runtime::new(AppState::default(), col, (400.0, 120.0), probe_env());
    rt.frame(0.0);
    assert!(
        (field_rect(&rt, "audit-full")[2] - 400.0).abs() <= 1.0,
        "a field without width fills its 400pt parent"
    );
}

#[test]
fn caller_chrome_survives_the_kit_shell() {
    const FILL: rune::Color = rune::Color::WHITE;
    fn field(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        rune::col().w_full().items_start().child(
        crate::ui::field::TextField::new("audit-chrome", &state.connection_search)
            .height(36.0)
            .width(200.0)
            .radius(8.0)
            .fill(FILL)
            .border(1.0, rune::Color::BLACK)
            .on_edit(|state: &mut AppState| &mut state.connection_search)
            .build(true))
    }
    let mut rt = Runtime::new(AppState::default(), field, (400.0, 120.0), probe_env());
    rt.frame(0.0);
    let chrome = element(&rt, "audit-chrome");
    assert_eq!(chrome.fill, FILL, "caller fill must win over the kit shell");
    assert!(
        (chrome.radius - 8.0).abs() <= 0.5,
        "caller radius must win; got {}",
        chrome.radius
    );
    assert!((chrome.rect[3] - 36.0).abs() <= 0.5, "height kept");
}

#[test]
fn plain_field_has_no_kit_shell() {
    // A bare field (no fill/border) is embedded: no shell fill is painted.
    fn field(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        crate::ui::field::TextField::new("audit-plain", &state.connection_search)
            .height(28.0)
            .width(220.0)
            .on_edit(|state: &mut AppState| &mut state.connection_search)
            .build(true)
    }
    let mut rt = Runtime::new(AppState::default(), field, (400.0, 120.0), probe_env());
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "audit-plain").fill,
        rune::Color::TRANSPARENT,
        "embedded field must not paint the kit's shell background"
    );
}

#[test]
#[ignore = "DEFECT e80bfb243: TextField::ink is stored but never applied; un-ignore after the fix"]
fn ink_parameter_reaches_the_editor_text_colour() {
    const INK: rune::Color = rune::Color::WHITE;
    fn field(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        rune::col().w_full().items_start().child(
            crate::ui::field::TextField::new("audit-ink", &state.connection_search)
                .height(28.0)
                .width(220.0)
                .ink(INK)
                .on_edit(|state: &mut AppState| &mut state.connection_search)
                .build(true),
        )
    }
    let mut rt = Runtime::new(AppState::default(), field, (400.0, 120.0), probe_env());
    rt.frame(0.0);
    let input = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("input"))
        .expect("inner editor");
    assert_eq!(
        input.text_color,
        Some(INK),
        "TextField::ink must colour the editor text"
    );
}

#[test]
#[ignore = "DEFECT e80bfb243: AppState::dark_theme() defaults to dark while app.rs falls back to system_dark(false); un-ignore after the fix"]
fn light_system_without_a_saved_theme_keeps_fields_on_the_light_palette() {
    let mut rt = Runtime::new(
        AppState::default(),
        view,
        (960.0, 640.0),
        Env { system_dark: false, scale: 1.0 },
    );
    rt.state.active = Section::Settings;
    rt.state.set_settings_section("dataBrowsing");
    rt.frame(0.0);
    let elements = rt.elements();
    let field = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("settings-page-size-input"))
        .expect("page-size field");
    let input = elements
        .iter()
        .find(|e| {
            e.key.as_deref() == Some("input")
                && e.rect[1] >= field.rect[1]
                && e.rect[1] + e.rect[3] <= field.rect[1] + field.rect[3]
        })
        .expect("inner editor");
    assert_eq!(
        input.text_color,
        Some(rune::tok::theme(false).text),
        "a light system without a saved theme must paint the light palette; dark_theme() said {}",
        rt.state.dark_theme()
    );
}
