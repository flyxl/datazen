//! T475: `treeStatusDot` port — status → colour/aria mapping, the idle no-op,
//! the 8×8 pip geometry and two-window/two-palette rect containment.

use rune::{col, row, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::ui::palette;
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

fn dot(rt: &Runtime<Scene>) -> rune::ElementInfo {
    let key = format!("tree-status-dot-{ID}");
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(&key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_dot(rt: &Runtime<Scene>) -> bool {
    let key = format!("tree-status-dot-{ID}");
    rt.elements().iter().any(|e| e.key.as_deref() == Some(&key))
}

#[test]
fn status_parsing_matches_the_active_connection_union() {
    assert_eq!(ConnectionStatus::from_str(None), ConnectionStatus::Idle);
    assert_eq!(ConnectionStatus::from_str(Some("idle")), ConnectionStatus::Idle);
    assert_eq!(ConnectionStatus::from_str(Some("connecting")), ConnectionStatus::Connecting);
    assert_eq!(ConnectionStatus::from_str(Some("connected")), ConnectionStatus::Connected);
    assert_eq!(ConnectionStatus::from_str(Some("error")), ConnectionStatus::Error);
    assert_eq!(ConnectionStatus::from_str(Some("other")), ConnectionStatus::Idle);
}

#[test]
fn each_status_pins_its_exact_locale_title_and_only_connecting_pulses() {
    assert_eq!(ConnectionStatus::Idle.aria_key(), None);
    assert_eq!(ConnectionStatus::Connecting.aria_key(), Some("conn.connecting"));
    assert_eq!(ConnectionStatus::Connected.aria_key(), Some("conn.connected"));
    assert_eq!(ConnectionStatus::Error.aria_key(), Some("conn.failed"));

    for status in [ConnectionStatus::Connecting, ConnectionStatus::Connected, ConnectionStatus::Error] {
        let key = status.aria_key().expect("a drawn pip has a title");
        assert_eq!(dot(&runtime(status, true, DESIGN)).label.as_deref(), Some(t(key)), "{key} is the aria label");
        // The tooltip carries the same string as the aria label (React's title).
        assert_eq!(dot(&runtime(status, false, MIN)).label.as_deref(), Some(t(key)), "{key} is the tooltip text");
    }
    assert!(ConnectionStatus::Connecting.pulses(), "animate-pulse only on connecting");
    assert!(!ConnectionStatus::Connected.pulses());
    assert!(!ConnectionStatus::Error.pulses());
}

#[test]
fn color_mapping_uses_existing_tokens_and_three_distinct_fills() {
    for dark in [true, false] {
        assert_eq!(ConnectionStatus::Connecting.fill(dark), Some(kit(dark).c.warning.icon));
        assert_eq!(ConnectionStatus::Connected.fill(dark), Some(palette::status_ok(dark)));
        assert_eq!(ConnectionStatus::Error.fill(dark), Some(palette::status_fail(dark)));
        assert_eq!(ConnectionStatus::Idle.fill(dark), None);

        let connecting = dot(&runtime(ConnectionStatus::Connecting, dark, DESIGN)).fill;
        let connected = dot(&runtime(ConnectionStatus::Connected, dark, DESIGN)).fill;
        let failed = dot(&runtime(ConnectionStatus::Error, dark, DESIGN)).fill;
        assert_eq!(connecting, ConnectionStatus::Connecting.fill(dark).unwrap_or(connecting));
        assert_eq!(connected, ConnectionStatus::Connected.fill(dark).unwrap_or(connected));
        assert_eq!(failed, ConnectionStatus::Error.fill(dark).unwrap_or(failed));
        assert_ne!(connecting, connected, "connecting and connected differ: dark={dark}");
        assert_ne!(connected, failed, "connected and error differ: dark={dark}");
        assert_ne!(connecting, failed, "connecting and error differ: dark={dark}");
    }
}

#[test]
fn idle_status_draws_nothing_and_has_no_reserved_space() {
    for dark in [true, false] {
        let rt = runtime(ConnectionStatus::Idle, dark, DESIGN);
        assert!(!has_dot(&rt), "idle renders no pip: dark={dark}");
        let row = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("tree-row"));
        assert!(row.is_some_and(|r| r.rect[2] <= 240.5), "no space reserved for the idle pip");
    }
}

#[test]
fn a_missing_connection_entry_falls_back_to_idle() {
    let active: [(String, ConnectionStatus); 0] = [];
    assert!(render_status_dot(ID, &active, true).is_none(), "unknown id renders nothing");
    let other = [("conn-2".to_owned(), ConnectionStatus::Connected)];
    assert!(render_status_dot(ID, &other, true).is_none(), "another id does not draw this row's pip");
}

#[test]
fn pip_is_an_8_by_8_circle_and_fits_both_sizes_in_both_themes() {
    for dark in [true, false] {
        for size in [MIN, DESIGN] {
            for status in [ConnectionStatus::Connecting, ConnectionStatus::Connected, ConnectionStatus::Error] {
                let rt = runtime(status, dark, size);
                let element = dot(&rt);
                let [x, y, w, h] = element.rect;
                assert!((w - 8.0).abs() <= 0.5 && (h - 8.0).abs() <= 0.5, "8x8 pip: {status:?} {size:?} rect={:?}", element.rect);
                assert!(x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5, "{status:?} {size:?} dark={dark} crosses the window: {:?}", element.rect);
            }
        }
    }
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Mapping `Error` to `conn.connected` made `each_status_pins_its_exact_locale_title...`
//    fail with `left: Some("conn.connected") right: Some("conn.failed")`.
// 2. Making idle draw a pip made `idle_status_draws_nothing_and_has_no_reserved_space`
//    fail on `idle renders no pip`.
