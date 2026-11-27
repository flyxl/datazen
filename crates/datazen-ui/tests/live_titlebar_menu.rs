//! The actual root shell, not a detached menu component.

#![cfg(any(target_os = "linux", target_os = "windows"))]

use datazen_ui::test_support::{runtime, AppState, Runtime};
use datazen_ui::{Setting, Section};
use rune::{Key, KeyEvent, Mods};

fn settle(rt: &mut Runtime<AppState>) {
    for _ in 0..48 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<AppState>, key: &str) {
    settle(rt);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is painted"));
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} after hover"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn has(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

#[test]
fn titlebar_file_items_open_real_dialog_and_preserve_settings_return_route() {
    let mut rt = runtime();
    settle(&mut rt);
    for menu in ["file", "view", "tools", "help"] {
        assert!(has(&rt, &format!("menu-bar-{menu}-trigger")));
    }
    click(&mut rt, "menu-bar-file-trigger");
    assert!(has(&rt, "menu-bar-file-menu"));
    assert!(!has(&rt, "menu-bar-file-item3"), "no inert File entries");
    click(&mut rt, "menu-bar-file-item0");
    assert!(rt.state.connection_dialog_open);
    assert!(!has(&rt, "menu-bar-file-menu"));
    rt.state.close_connection_dialog();
    rt.invalidate();

    click(&mut rt, "menu-bar-tools-trigger");
    click(&mut rt, "menu-bar-tools-item0");
    assert_eq!(rt.state.active, Section::Workflows);
    click(&mut rt, "menu-bar-file-trigger");
    click(&mut rt, "menu-bar-file-item2");
    assert_eq!(rt.state.active, Section::Settings);
    assert!(has(&rt, "settings-section-general"));
    click(&mut rt, "settings-back");
    assert_eq!(rt.state.active, Section::Workflows);
}

#[test]
fn titlebar_view_persists_theme_and_help_requests_only_the_real_url() {
    let mut rt = runtime();
    rt.state
        .write_ui_setting(Setting::ThemeMode { mode: "system".into() })
        .expect("seed the Settings port");
    settle(&mut rt);
    click(&mut rt, "menu-bar-view-trigger");
    click(&mut rt, "menu-bar-view-item0");
    assert!(rt.state.ui_settings().iter().any(|setting| {
        matches!(setting, Setting::ThemeMode { mode } if mode == "light")
    }));
    assert!(!has(&rt, "menu-bar-view-menu"));
    assert!(has(&rt, "theme-toggle-button"));

    click(&mut rt, "menu-bar-help-trigger");
    assert!(has(&rt, "menu-bar-help-menu"));
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    settle(&mut rt);
    assert!(!has(&rt, "menu-bar-help-menu"));
    assert!(rt.take_url_requests().is_empty());

    click(&mut rt, "menu-bar-help-trigger");
    click(&mut rt, "menu-bar-help-item0");
    assert_eq!(
        rt.take_url_requests(),
        vec!["https://github.com/flyxl/datazen/issues/new".to_owned()]
    );
    assert!(!has(&rt, "menu-bar-help-menu"));
}

#[test]
fn file_shortcuts_use_the_same_actions_without_resetting_an_open_draft() {
    let mut rt = runtime();
    rt.on_key = Some(datazen_ui::app_key);
    settle(&mut rt);
    let ctrl = Mods {
        cmd: true,
        ctrl: true,
        ..Mods::default()
    };
    rt.key(KeyEvent {
        key: Key::Command(','),
        mods: ctrl,
    });
    settle(&mut rt);
    assert_eq!(rt.state.active, Section::Settings);
    click(&mut rt, "settings-back");
    assert_eq!(rt.state.active, Section::Connections);

    rt.key(KeyEvent {
        key: Key::Command('n'),
        mods: ctrl,
    });
    settle(&mut rt);
    assert!(rt.state.connection_dialog_open);
    let opened = rt.state.connection_dialog_epoch;
    rt.key(KeyEvent {
        key: Key::Command('n'),
        mods: ctrl,
    });
    settle(&mut rt);
    assert_eq!(rt.state.connection_dialog_epoch, opened);
}
