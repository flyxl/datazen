//! T475 independent audit (tester, not coder).
//!
//! The shipped test asserts `.label` twice and calls the second one "tooltip",
//! so the actual `help` string is never observed. These probes check the help
//! tooltip, the full-round radius, exact status parsing, and a non-vacuous
//! `visible`-rect geometry channel for all three drawn states.

use rune::{col, row, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::navigator::tree_status_dot::{render_status_dot, ConnectionStatus};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const ID: &str = "conn-1";

#[derive(Clone)]
struct Scene {
    status: ConnectionStatus,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let active = [(ID.to_owned(), s.status)];
    let mut line = row().key("tree-row").w(240.0).h(32.0).center_y();
    if let Some(dot) = render_status_dot(ID, &active, s.dark) {
        line = line.child(dot);
    }
    col().w_full().h_full().child(line)
}

fn runtime(status: ConnectionStatus, dark: bool, size: (f32, f32)) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        Scene { status, dark },
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn dot<S>(rt: &Runtime<S>) -> rune::ElementInfo {
    let key = format!("tree-status-dot-{ID}");
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(&key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

#[test]
fn the_help_tooltip_carries_the_exact_locale_string() {
    for (status, key) in [
        (ConnectionStatus::Connecting, "conn.connecting"),
        (ConnectionStatus::Connected, "conn.connected"),
        (ConnectionStatus::Error, "conn.failed"),
    ] {
        for dark in [true, false] {
            let rt = runtime(status, dark, DESIGN);
            let e = dot(&rt);
            assert_eq!(e.help.as_deref(), Some(t(key)), "{status:?} dark={dark}: help tooltip");
            assert_eq!(e.label.as_deref(), Some(t(key)), "{status:?} dark={dark}: aria label");
        }
    }
}

#[test]
fn a_full_round_pip_has_radius_four() {
    for status in [ConnectionStatus::Connecting, ConnectionStatus::Connected, ConnectionStatus::Error] {
        let e = dot(&runtime(status, true, DESIGN));
        assert!((e.radius - 4.0).abs() <= 0.01, "{status:?}: rounded-full on an 8x8 pip has radius 4, got {}", e.radius);
    }
}

#[test]
fn status_parsing_is_exact_and_case_sensitive() {
    assert_eq!(ConnectionStatus::from_str(Some("CONNECTED")), ConnectionStatus::Idle);
    assert_eq!(ConnectionStatus::from_str(Some("connecting ")), ConnectionStatus::Idle);
    assert_eq!(ConnectionStatus::from_str(Some("")), ConnectionStatus::Idle);
    assert_eq!(ConnectionStatus::from_str(Some("error")), ConnectionStatus::Error);
}

#[test]
fn all_drawn_pips_stay_inside_via_visible_rects() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            for status in [ConnectionStatus::Connecting, ConnectionStatus::Connected, ConnectionStatus::Error] {
                let rt = runtime(status, dark, size);
                let [x, y, w, h] = dot(&rt).visible;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                    "{status:?} {size:?} dark={dark}: visible rect crosses window"
                );
            }
        }
    }
}
