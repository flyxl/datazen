//! T435 independent audit: `ShortcutFooter` vs React keymap.
//!
//! Probes branches the coder's suite does not: preset/action value mapping,
//! display formatting for F-keys and '+' separators, every preset's rendered
//! kbd values, empty-custom fallback in the built tree, and a geometry stress
//! with long custom bindings at both windows/palettes.

use std::collections::BTreeMap;

use rune::{col, Cx, El, Env, Runtime};

use crate::views::home::shortcut_footer::{
    format_shortcut_for_display, get_action_shortcut, shortcut_footer, KeymapAction, KeymapPreset,
    ShortcutFooterVm, FOOTER_KEY, REACT_KEY,
};

#[derive(Clone)]
struct Probe {
    vm: ShortcutFooterVm,
    dark: bool,
    is_mac: bool,
}

fn probe(preset: KeymapPreset, dark: bool, is_mac: bool) -> Probe {
    Probe {
        vm: ShortcutFooterVm {
            preset,
            custom: BTreeMap::new(),
        },
        dark,
        is_mac,
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let footer = shortcut_footer(&s.vm, s.dark, s.is_mac);
    col().w_full().h_full().child(footer.unwrap_or_else(|| rune::el().a11y_hidden()))
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

/// React's preset/action string unions round-trip through the Rune mirrors.
#[test]
fn preset_and_action_values_match_react_unions() {
    assert_eq!(KeymapPreset::from_value("default"), KeymapPreset::Default);
    assert_eq!(KeymapPreset::from_value("dbeaver"), KeymapPreset::Dbeaver);
    assert_eq!(KeymapPreset::from_value("navicat"), KeymapPreset::Navicat);
    assert_eq!(KeymapPreset::from_value("bogus"), KeymapPreset::Default);
    for (action, value) in [
        (KeymapAction::Execute, "execute"),
        (KeymapAction::ExecuteAll, "executeAll"),
        (KeymapAction::NewQuery, "newQuery"),
        (KeymapAction::CloseTab, "closeTab"),
        (KeymapAction::SaveQuery, "saveQuery"),
        (KeymapAction::FormatSql, "formatSql"),
    ] {
        assert_eq!(action.as_str(), value);
    }
}

/// React's formatShortcutForDisplay handles F-keys, '+' separators and plain
/// tokens in addition to the modifiers the coder's suite already pins.
#[test]
fn display_formatting_covers_fkeys_and_plus_separators() {
    assert_eq!(format_shortcut_for_display("Mod-F3", true), "Cmd+F3");
    assert_eq!(format_shortcut_for_display("ctrl+f12", false), "Ctrl+F12");
    assert_eq!(format_shortcut_for_display("Alt+Shift-n", true), "Option+Shift+N");
    assert_eq!(format_shortcut_for_display("space", true), "SPACE");
}

/// Every React preset yields three visible kbd values matching keymap.ts.
#[test]
fn every_preset_renders_its_react_kbd_values() {
    let cases = [
        (KeymapPreset::Default, ["Cmd+N", "Cmd+S", "Cmd+Shift+F"]),
        (KeymapPreset::Dbeaver, ["Ctrl+]", "Cmd+S", "Ctrl+Shift+F"]),
        (KeymapPreset::Navicat, ["Cmd+Q", "Cmd+S", "Cmd+Shift+F"]),
    ];
    for (preset, expected) in cases {
        let rt = runtime(probe(preset, true, true), (1440.0, 816.0));
        let keys = [
            "connections-shortcut-key-kbd",
            "connections-shortcut-save-kbd",
            "connections-shortcut-format-kbd",
        ];
        for (key, want) in keys.iter().zip(expected.iter()) {
            assert_eq!(element(&rt, key).text.as_deref(), Some(*want), "preset {preset:?} {key}");
        }
    }
}

/// An empty custom binding falls back to the preset in the RENDERED tree, as
/// React's `custom[action]` truthiness gate does.
#[test]
fn empty_custom_binding_falls_back_in_the_tree() {
    let mut p = probe(KeymapPreset::Default, true, true);
    p.vm.custom.insert("newQuery".to_owned(), String::new());
    let rt = runtime(p, (1440.0, 816.0));
    assert_eq!(element(&rt, "connections-shortcut-key-kbd").text.as_deref(), Some("Cmd+N"));
}

/// A custom binding for every action changes all three rendered kbd values.
#[test]
fn custom_bindings_override_every_preset_value() {
    let mut p = probe(KeymapPreset::Navicat, false, false);
    for (action, value) in [
        ("newQuery", "Mod-Alt-n"),
        ("saveQuery", "Ctrl-Shift-s"),
        ("formatSql", "F3"),
    ] {
        p.vm.custom.insert(action.to_owned(), value.to_owned());
    }
    let rt = runtime(p, (1440.0, 816.0));
    assert_eq!(element(&rt, "connections-shortcut-key-kbd").text.as_deref(), Some("Ctrl+Alt+N"));
    assert_eq!(element(&rt, "connections-shortcut-save-kbd").text.as_deref(), Some("Ctrl+Shift+S"));
    assert_eq!(element(&rt, "connections-shortcut-format-kbd").text.as_deref(), Some("F3"));
    assert!(element(&rt, FOOTER_KEY).key.is_some());
    assert!(element(&rt, REACT_KEY).key.is_some());
}

/// Long custom shortcut strings still fit and never overlap at either window.
#[test]
fn long_custom_bindings_fit_both_windows_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe(KeymapPreset::Default, dark, true);
            for action in ["newQuery", "saveQuery", "formatSql"] {
                p.vm.custom.insert(action.to_owned(), "Ctrl-Shift-Alt-Meta-F12".to_owned());
            }
            let rt = runtime(p, size);
            for key in ["connections-shortcut-key", "connections-shortcut-save", "connections-shortcut-format"] {
                let r = element(&rt, key).rect;
                assert!(
                    r[0] >= -0.5 && r[1] >= -0.5 && r[0] + r[2] <= size.0 + 0.5 && r[1] + r[3] <= size.1 + 0.5,
                    "dark={dark} {size:?}: {key} rect {r:?} outside"
                );
            }
        }
    }
}
