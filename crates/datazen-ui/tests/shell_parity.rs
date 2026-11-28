//! T053: React `packages/frontend/src/shell.ts` parity.
//!
//! `shell.ts` is an X-class file: a doc comment plus four pure
//! `export { .. } from '..'` statements, with no constants and no logic of its
//! own. It exists only so the desktop shell (TitleBar / WindowControls) has a
//! small shared surface: the i18n hook, the OS-dimension platform hook, the UI
//! store and the window-surface colour helper. Once the React shell is gone
//! there is nothing to port — Rune's shell is `app::view` + `views::chrome`,
//! composed directly from the framework.
//!
//! These tests are the receipt for that decision: they show the four concerns
//! the barrel re-exported are all present in the Rune root view, sourced from
//! `t()`, `Cx`/`Env`, `AppState` and the DataZen palette instead of a re-export. A
//! future "port shell.ts" that duplicated a separate shell component would be
//! wrong; this file documents why.

use datazen_ui::test_support::*;

/// Render the real root view once and return its runtime.
fn root_shell() -> Runtime<AppState> {
    let mut rt = runtime();
    rt.frame(0.0);
    rt
}

/// A laid-out element by key, from the tree (not from any component's report).
fn element_by_key(rt: &Runtime<AppState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("shell key {key} is laid out"))
}

#[test]
fn shell_ts_is_x_class_and_the_rune_root_carries_its_four_concerns() {
    let mut rt = root_shell();

    // Chrome: the nodes the desktop shell used to reach through the barrel.
    for key in [
        "datazen-root",
        "datazen-titlebar",
        "datazen-titlebar-label",
        "datazen-theme-toggle",
        "datazen-body",
        "datazen-page",
    ] {
        assert!(rt.find(key).is_some(), "shell chrome node missing: {key}");
    }

    // i18n hook concern: rail labels are resolved t() strings, not key echoes.
    for section in Section::ALL {
        let label = section.label();
        if section == Section::Tools {
            // Tools has no React sidebar entry; it keeps a literal.
            assert_eq!(label, "Tools");
            continue;
        }
        assert!(!label.starts_with("nav."), "rail label is localized: {label}");
        assert_ne!(label, section.key(), "rail label is not a key echo");
    }
    let databases = element_by_key(&rt, &format!("rail-{}", Section::Connections.key()));
    assert_eq!(databases.label.as_deref(), Some(Section::Connections.label()));

    // Platform hook concern: the self-drawn titlebar is the 40pt OS-chrome band
    // (Rune supplies the window-control inset through `Cx`, no React hook).
    let titlebar = element_by_key(&rt, "datazen-titlebar");
    assert!(
        (titlebar.rect[3] - 40.0).abs() < 0.5,
        "titlebar is the 40pt chrome band, got {}",
        titlebar.rect[3]
    );

    // UI store concern: one shared AppState drives both the rail selection and
    // the titlebar. Pressing a rail item moves state and the titlebar follows.
    let (x, y) = rt
        .find(&format!("rail-{}", Section::Workspace.key()))
        .expect("workspace rail");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert_eq!(rt.state.active, Section::Workspace);
    let label = element_by_key(&rt, "datazen-titlebar-label");
    assert_eq!(label.text.as_deref(), Some(Section::Workspace.label()));
}

#[test]
fn live_shell_and_settings_use_original_react_surface_roles_in_both_appearances() {
    for (system_dark, page, title, rail, selected) in [
        (false, "#f6f8fb", "#ffffff", "#f1f5f9", "#d9edf9"),
        (true, "#0b0e14", "#0a0d14", "#0e121b", "#172c3b"),
    ] {
        let env = Env {
            system_dark,
            scale: 1.0,
        };
        let mut rt = Runtime::new(AppState::default(), view, (1440.0, 900.0), env);
        rt.frame(0.0);
        let check = |rt: &Runtime<AppState>, key: &str, expected: &str| {
            assert_eq!(
                element_by_key(rt, key).fill,
                rune::color::hex(expected).expect("known CSS color"),
                "{key} (system_dark={system_dark})",
            );
        };
        check(&rt, "datazen-root", page);
        check(&rt, "datazen-titlebar", title);
        let (x, y) = rt.find("rail-settings").expect("Settings entry");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        check(&rt, "settings-nav", rail);
        check(&rt, "settings-nav-general", selected);
    }
}
