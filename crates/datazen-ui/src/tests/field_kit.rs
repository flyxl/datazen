use super::*;
use rune::{Env, Runtime};

#[test]
fn kit_text_input_outer_shell_focuses_on_top_edge_click() {
    // Step-1 probe (kept as the recorded result): kit TextInput routes a press
    // on the outer shell's padding to the inner editor, so the whole box is a
    // hit target. This justified delegating TextField to the kit.
    fn probe(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        let k = crate::ui::kit::kit(true);
        rune_kit::TextInput::new(&state.connection_search)
            .height(28.0)
            .placeholder("x")
            .on_edit(|state: &mut AppState| &mut state.connection_search)
            .build(k)
    }
    let env = Env {
        system_dark: true,
        scale: 1.0,
    };
    let mut rt = Runtime::new(AppState::default(), probe, (400.0, 120.0), env);
    rt.frame(0.0);
    let elements = rt.elements();
    let field = elements
        .iter()
        .find(|e| e.role == "textbox" && (e.rect[3] - 28.0).abs() < 1.0)
        .or_else(|| elements.iter().find(|e| e.role == "textbox"))
        .expect("kit TextInput renders an outer textbox");
    let x = field.rect[0] + 20.0;
    let y = field.rect[1] + 3.0;
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste("hi");
    rt.frame(0.0);
    assert!(
        state_text(&rt.state.connection_search).contains("hi"),
        "click at top-edge padding (y+3) should focus and accept text; got {:?}, focused={}",
        state_text(&rt.state.connection_search),
        rt.state.connection_search.focused
    );
}

#[test]
fn text_field_click_at_top_edge_focuses_and_takes_input() {
    // The delegated TextField must keep the whole 28pt box clickable: a press
    // 3pt below the top edge lands in the outer padding and still focuses.
    fn probe(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        crate::ui::field::TextField::new("probe", &state.connection_search)
            .height(28.0)
            .width(220.0)
            .placeholder("x", rune::Color::TRANSPARENT)
            .on_edit(|state: &mut AppState| &mut state.connection_search)
            .build(true)
    }
    let env = Env {
        system_dark: true,
        scale: 1.0,
    };
    let mut rt = Runtime::new(AppState::default(), probe, (400.0, 120.0), env);
    rt.frame(0.0);
    let field = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("probe"))
        .expect("field rendered");
    rt.pointer_down(field.rect[0] + 20.0, field.rect[1] + 3.0);
    rt.pointer_up(field.rect[0] + 20.0, field.rect[1] + 3.0);
    rt.frame(0.0);
    rt.paste("hi");
    rt.frame(0.0);
    assert_eq!(state_text(&rt.state.connection_search), "hi");
}

#[test]
fn text_field_respects_explicit_width() {
    fn probe(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        rune::col().w_full().items_start().child(
            crate::ui::field::TextField::new("probe-w", &state.connection_search)
                .height(28.0)
                .width(180.0)
                .on_edit(|state: &mut AppState| &mut state.connection_search)
                .build(true),
        )
    }
    let env = Env {
        system_dark: true,
        scale: 1.0,
    };
    let mut rt = Runtime::new(AppState::default(), probe, (400.0, 120.0), env);
    rt.frame(0.0);
    let field = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("probe-w"))
        .expect("field rendered");
    assert!(
        (field.rect[2] - 180.0).abs() < 1.0,
        "width {}, want 180",
        field.rect[2]
    );
}

#[test]
fn secure_text_field_does_not_expose_plaintext() {
    let mut state = AppState::default();
    state.connection_password = rune::TextEdit::new("s3cret");
    fn probe(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        crate::ui::field::TextField::new("probe-secure", &state.connection_password)
            .height(28.0)
            .width(180.0)
            .secure()
            .on_edit(|state: &mut AppState| &mut state.connection_password)
            .build(true)
    }
    let env = Env {
        system_dark: true,
        scale: 1.0,
    };
    let mut rt = Runtime::new(state, probe, (400.0, 120.0), env);
    rt.frame(0.0);
    let leaked = rt.elements().into_iter().any(|e| {
        e.text.as_deref() == Some("s3cret")
            || e.label.as_deref() == Some("s3cret")
            || e.selected.as_deref() == Some("s3cret")
    });
    assert!(!leaked, "secure field must not expose plaintext in element tree");
}

#[test]
fn placeholder_visible_when_empty_and_hidden_when_filled() {
    fn probe(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        crate::ui::field::TextField::new("probe-ph", &state.connection_search)
            .height(28.0)
            .width(220.0)
            .placeholder("Type here", rune::Color::TRANSPARENT)
            .on_edit(|state: &mut AppState| &mut state.connection_search)
            .build(true)
    }
    let env = Env {
        system_dark: true,
        scale: 1.0,
    };
    let mut rt = Runtime::new(AppState::default(), probe, (400.0, 120.0), env);
    rt.frame(0.0);
    // Empty: the editor shows no value text and carries the placeholder as its
    // accessible name (kit renders the placeholder visually in this state).
    let elements = rt.elements();
    let empty_input = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("input"))
        .expect("inner input");
    assert!(
        empty_input.text.as_deref().unwrap_or("").is_empty(),
        "empty field should have no value text, got {:?}",
        empty_input.text
    );
    assert_eq!(
        empty_input.label.as_deref(),
        Some("Type here"),
        "empty field should be named by its placeholder"
    );

    rt.state.connection_search = rune::TextEdit::new("value");
    rt.invalidate();
    rt.frame(0.0);
    let elements = rt.elements();
    let filled_input = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("input"))
        .expect("inner input");
    assert_eq!(
        filled_input.text.as_deref(),
        Some("value"),
        "filled field should show its value"
    );
    let placeholder_as_text = elements
        .iter()
        .any(|e| e.text.as_deref() == Some("Type here"));
    assert!(
        !placeholder_as_text,
        "placeholder text must not appear once the field has a value"
    );
}

#[test]
fn navigator_search_and_home_filter_stay_inside_minimum_window() {
    // 960x640: the navigator search row and the connections-home filter must
    // not hang past the window's right edge.
    let env = Env {
        system_dark: true,
        scale: 1.0,
    };
    let mut rt = Runtime::new(AppState::default(), view, (960.0, 640.0), env);
    rt.frame(0.0);

    let elements = rt.elements();
    let nav = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("navigator-filter"))
        .expect("navigator search present");
    assert!(
        nav.rect[0] + nav.rect[2] <= 960.5,
        "navigator search overflows: x={} w={} right={}",
        nav.rect[0],
        nav.rect[2],
        nav.rect[0] + nav.rect[2]
    );

    let home = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("home-connections-filter"))
        .expect("home filter present");
    assert!(
        home.rect[0] + home.rect[2] <= 960.5,
        "home filter overflows: x={} w={} right={}",
        home.rect[0],
        home.rect[2],
        home.rect[0] + home.rect[2]
    );
}

fn state_text(edit: &rune::TextEdit) -> String {
    edit.display().0.to_string()
}
