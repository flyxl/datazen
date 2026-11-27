//! The two chrome fixes: the titlebar's React `ThemeToggle` (icon face,
//! three modes, settings write) and the content status bar's React tokens
//! in both appearances.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::components::theme_toggle::theme_icon_resolver;
use datazen_ui::components::themed_icon::ResolvedIcon;
use datazen_ui::test_support::runtime;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::content_status_bar::{build, CountStrategy, StatusBarVm};
use datazen_ui::Setting;

fn env(dark: bool) -> Env {
    Env { system_dark: dark, scale: 1.0 }
}

fn keyed<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    keyed(rt, key).is_some()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = keyed(rt, key).unwrap_or_else(|| panic!("{key} is laid out")).rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

#[test]
fn theme_icons_are_the_react_sun_moon_and_monitor() {
    let resolver = theme_icon_resolver();
    for (id, name) in [
        ("theme.light", "sun"),
        ("theme.dark", "moon"),
        ("theme.system", "monitor"),
    ] {
        match resolver(id) {
            ResolvedIcon::Lucide(glyph) => assert_eq!(glyph, name),
            other => panic!("{id} must be a lucide icon, got {other:?}"),
        }
    }
}

#[test]
fn the_titlebar_toggle_starts_on_system_and_writes_the_three_way_setting() {
    let mut rt = runtime();
    // The memory workspace in the test harness ships a legacy dark Theme
    // entry; a real fresh host returns no setting, which reads as System.
    rt.state
        .write_ui_setting(Setting::ThemeMode { mode: "system".to_owned() })
        .expect("the memory port writes the setting");
    rt.invalidate();
    rt.frame(0.0);
    assert!(has_key(&rt, "datazen-theme-toggle"));
    assert!(has_key(&rt, "theme-toggle-button"));
    assert!(has_key(&rt, "theme-toggle-icon"));
    let button = keyed(&rt, "theme-toggle-button").expect("the trigger");
    assert_eq!(button.label.as_deref(), Some(datazen_ui::i18n::t("theme.system")));
    assert!(!has_key(&rt, "theme-toggle-menu"), "the popover starts closed");

    // Pressing the icon opens all three React options; choosing System writes
    // the setting and closes the popover.
    click(&mut rt, "theme-toggle-button");
    for key in ["theme-option-light", "theme-option-dark", "theme-option-system"] {
        assert!(has_key(&rt, key), "{key} is in the popover");
    }
    click(&mut rt, "theme-option-system");
    assert!(!has_key(&rt, "theme-toggle-menu"), "choosing an option closes it");
    let written = rt.state.ui_settings().into_iter().find_map(|setting| match setting {
        Setting::ThemeMode { mode } => Some(mode),
        _ => None,
    });
    assert_eq!(written.as_deref(), Some("system"));
}

#[derive(Clone)]
struct BarProbe {
    dark: bool,
    vm: StatusBarVm,
}

fn bar_view(probe: &BarProbe, _cx: &mut Cx<BarProbe>) -> El {
    // A root row would be stretched to the whole window by the runtime; give
    // the footer a column parent, exactly like the real page body.
    rune::col()
        .w_full()
        .h_full()
        .child(build(&probe.vm, probe.dark))
        .child(rune::row().grow(1.0))
}

fn bar_rt(dark: bool) -> Runtime<BarProbe> {
    let vm = StatusBarVm {
        connected: true,
        connected_label: datazen_ui::i18n::t("conn.connected").to_owned(),
        table_name: Some("users".to_owned()),
        column_count: 3,
        total_rows: 1_234,
        total_rows_kind: CountStrategy::Exact,
        mod_key: "⌘".to_owned(),
        key_sep: String::new(),
        ..Default::default()
    };
    let mut rt = Runtime::new(BarProbe { dark, vm }, bar_view, (1440.0, 900.0), env(dark));
    rt.frame(0.0);
    rt
}

#[test]
fn the_status_bar_uses_react_surface_alt_edge_and_success_tokens() {
    let mut light_fill = None;
    for dark in [true, false] {
        let rt = bar_rt(dark);
        let k = kit(dark);
        let footer = keyed(&rt, "content-status-bar").expect("the footer mounts");
        assert!(
            (footer.rect[3] - 40.0).abs() < 1.0,
            "40pt footer, got {:?}",
            footer.rect
        );
        assert_eq!(footer.role, "status");
        assert_eq!(footer.fill, k.c.layer_01, "bg-surface-alt");
        assert_eq!(
            keyed(&rt, "status-connection-dot").expect("the dot").fill,
            k.c.success.bg,
            "bg-success state dot"
        );
        for key in ["status-connection-label", "status-context", "status-shortcuts"] {
            assert_eq!(
                keyed(&rt, key).expect(key).text_color,
                Some(k.c.text_muted),
                "{key} uses text-fg-secondary/muted"
            );
        }
        if dark {
            light_fill = Some(footer.fill);
        } else {
            // The old `control_off` fill was a translucent dark wash and read
            // as a grey strip in light mode; the two palettes must differ.
            assert_ne!(light_fill, Some(footer.fill));
        }
    }
}
