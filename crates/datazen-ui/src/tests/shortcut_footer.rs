//! T435: `ShortcutFooter` port — keys, keymap resolution/display, soft
//! degradation and rect geometry at 960×640 / 1440×816 in both palettes.

use std::collections::BTreeMap;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
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

fn probe(preset: KeymapPreset) -> Probe {
    Probe {
        vm: ShortcutFooterVm {
            preset,
            custom: BTreeMap::new(),
        },
        dark: true,
        is_mac: true,
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

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

#[test]
fn footer_pins_home_keys_and_localized_labels() {
    let rt = runtime(probe(KeymapPreset::Default), (1440.0, 816.0));
    for key in [
        FOOTER_KEY,
        REACT_KEY,
        "connections-shortcut-key",
        "connections-shortcut-save",
        "connections-shortcut-format",
        "connections-shortcut-key-kbd",
        "connections-shortcut-key-label",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for key in [
        "keymap.action.newQuery",
        "keymap.action.saveQuery",
        "keymap.action.formatSql",
    ] {
        assert!(rendered.iter().any(|text| text == t(key)), "{key}: exact translated label");
    }
    assert_eq!(element(&rt, "connections-shortcut-key-kbd").text.as_deref(), Some("Cmd+N"));
}

#[test]
fn preset_and_custom_keymap_resolution_matches_react() {
    let empty = BTreeMap::new();
    assert_eq!(get_action_shortcut(KeymapAction::NewQuery, KeymapPreset::Default, &empty), "Mod-n");
    assert_eq!(get_action_shortcut(KeymapAction::NewQuery, KeymapPreset::Dbeaver, &empty), "Ctrl-]");
    assert_eq!(get_action_shortcut(KeymapAction::NewQuery, KeymapPreset::Navicat, &empty), "Mod-q");
    assert_eq!(get_action_shortcut(KeymapAction::SaveQuery, KeymapPreset::Navicat, &empty), "Mod-s");

    let mut custom = BTreeMap::new();
    custom.insert("newQuery".to_owned(), "Ctrl-Alt-n".to_owned());
    assert_eq!(get_action_shortcut(KeymapAction::NewQuery, KeymapPreset::Default, &custom), "Ctrl-Alt-n");
    custom.insert("newQuery".to_owned(), String::new());
    assert_eq!(get_action_shortcut(KeymapAction::NewQuery, KeymapPreset::Default, &custom), "Mod-n",
        "an empty custom binding falls back to the preset");
}

#[test]
fn display_formatting_follows_the_host_platform() {
    assert_eq!(format_shortcut_for_display("Mod-Shift-f", true), "Cmd+Shift+F");
    assert_eq!(format_shortcut_for_display("Mod-Shift-f", false), "Ctrl+Shift+F");
    assert_eq!(format_shortcut_for_display("Ctrl-]", true), "Ctrl+]");
    assert_eq!(format_shortcut_for_display("Alt-x", true), "Option+X");
    assert_eq!(format_shortcut_for_display("Alt-x", false), "Alt+X");
}

#[test]
fn custom_binding_changes_the_rendered_kbd() {
    let mut p = probe(KeymapPreset::Default);
    p.vm.custom.insert("formatSql".to_owned(), "Mod-Alt-f".to_owned());
    let rt = runtime(p, (1440.0, 816.0));
    assert_eq!(element(&rt, "connections-shortcut-format-kbd").text.as_deref(), Some("Cmd+Option+F"));
    assert!(has(&rt, "connections-shortcut-format-label"));
}

#[test]
fn footer_wraps_and_fits_both_windows_in_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe(KeymapPreset::Default);
            p.dark = dark;
            let rt = runtime(p, size);
            let items: Vec<rune::ElementInfo> = rt
                .elements()
                .into_iter()
                .filter(|e| {
                    matches!(
                        e.key.as_deref(),
                        Some("connections-shortcut-key")
                            | Some("connections-shortcut-save")
                            | Some("connections-shortcut-format")
                    )
                })
                .collect();
            assert_eq!(items.len(), 3, "all three shortcuts are visible");
            for item in &items {
                assert!(
                    item.rect[0] >= -0.5 && item.rect[1] >= -0.5
                        && item.rect[0] + item.rect[2] <= size.0 + 0.5
                        && item.rect[1] + item.rect[3] <= size.1 + 0.5,
                    "dark={dark} {size:?}: shortcut item {:?} outside the window",
                    item.key
                );
            }
            // Wrapping is soft: items may share a row or stack, never overlap.
            for (i, a) in items.iter().enumerate() {
                for b in items.iter().skip(i + 1) {
                    let overlap = a.rect[0] < b.rect[0] + b.rect[2]
                        && b.rect[0] < a.rect[0] + a.rect[2]
                        && a.rect[1] < b.rect[1] + b.rect[3]
                        && b.rect[1] < a.rect[1] + a.rect[3];
                    assert!(!overlap, "shortcut items must not overlap");
                }
            }
        }
    }
}
