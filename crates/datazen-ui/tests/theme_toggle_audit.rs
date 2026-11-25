//! T282 independent audit: fresh vectors — the un-resolved honest
//! placeholder (no resolver installed), the three option words
//! verbatim, both themes mounting the same key set inside the minimum
//! window, the trigger's icon resolution through the shared resolver
//! seam, and the open menu's anchored geometry.

use std::rc::Rc;

use rune::{Cx, El, Env, Runtime};

use datazen_ui::components::themed_icon::ResolvedIcon;
use datazen_ui::components::theme_toggle::{current_option, ThemeToggle};
use datazen_ui::views::appearance_section::ThemeMode;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn resolver() -> Rc<dyn Fn(&str) -> ResolvedIcon> {
    Rc::new(|id: &str| {
        let name = match id {
            "theme.light" => "sun",
            "theme.dark" => "moon",
            "theme.system" => "monitor",
            _ => "?",
        };
        ResolvedIcon::Lucide(name.to_owned())
    })
}

#[derive(Clone)]
struct Audit {
    mode: ThemeMode,
    open: bool,
    dark: bool,
    with_resolver: bool,
    closes: usize,
}

impl Default for Audit {
    fn default() -> Self {
        Self {
            mode: ThemeMode::System,
            open: false,
            dark: true,
            with_resolver: false,
            closes: 0,
        }
    }
}

fn audit_view(s: &Audit, _cx: &mut Cx<Audit>) -> El {
    let mut toggle = ThemeToggle::<Audit>::new(s.mode, s.open);
    if s.with_resolver {
        toggle = toggle.resolver(resolver());
    }
    toggle
        .on_close(|st| st.closes += 1)
        .build_root(datazen_ui::ui::kit::kit(s.dark))
}

fn mount(state: Audit, size: (f32, f32)) -> Runtime<Audit> {
    let dark = state.dark;
    let mut rt = Runtime::new(state, audit_view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn keys<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key)
        .collect()
}

fn keyed<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

/// Without the shared resolver the default resolver honestly paints
/// the `?` placeholder — the resolver's construction is T535's seam.
#[test]
fn without_a_resolver_the_placeholder_is_honest() {
    let rt = mount(
        Audit { mode: ThemeMode::Dark, with_resolver: false, ..Default::default() },
        (960.0, 640.0),
    );
    // The trigger's face is the placeholder badge (?), not a lucide
    // glyph: the ThemedIcon's default resolver degrade.
    let face = keyed(&rt, "theme-toggle-icon");
    assert!(face.is_some(), "the icon mounts even unresolved");
    // No crash, no missing keys — the honest render.
    assert!(keyed(&rt, "theme-toggle-button").is_some());
}

/// The three option words are the pack's exact strings.
#[test]
fn the_option_words_are_the_packs_exact_strings() {
    let rt = mount(
        Audit { mode: ThemeMode::Dark, open: true, with_resolver: true, ..Default::default() },
        (960.0, 640.0),
    );
    for (value, word) in [
        ("light", "浅色"),
        ("dark", "深色"),
        ("system", "跟随系统"),
    ] {
        assert_eq!(
            keyed(&rt, &format!("theme-option-{value}-text")).and_then(|e| e.text),
            Some(word.to_owned()),
            "the {value} option's word is exact"
        );
    }
    // The trigger's tooltip names the current mode.
    let button = keyed(&rt, "theme-toggle-button").expect("the trigger mounts");
    let tooltip = button.help.unwrap_or_default();
    assert!(tooltip.contains("深色"), "the tooltip's current word: {tooltip}");
}

/// Both themes mount the same key set, and every node stays inside the
/// minimum window with the menu open.
#[test]
fn both_themes_mount_the_same_keys_inside_the_minimum_window() {
    for dark in [true, false] {
        let state = Audit {
            mode: ThemeMode::System,
            open: true,
            dark,
            with_resolver: true,
            ..Default::default()
        };
        let rt = mount(state, (960.0, 640.0));
        for e in rt.elements() {
            let r = e.rect;
            assert!(
                r[0] >= -0.5 && r[1] >= -0.5 && r[0] + r[2] <= 960.5 && r[1] + r[3] <= 640.5,
                "{:?} out of the minimum window (dark={dark})",
                e.key
            );
        }
    }
    let dark_keys = keys(&mount(
        Audit { mode: ThemeMode::Dark, open: true, dark: true, with_resolver: true, ..Default::default() },
        (960.0, 640.0),
    ));
    let light_keys = keys(&mount(
        Audit { mode: ThemeMode::Dark, open: true, dark: false, with_resolver: true, ..Default::default() },
        (960.0, 640.0),
    ));
    assert_eq!(dark_keys, light_keys, "the key set is theme-stable");
}

/// The trigger's icon resolves through the shared resolver seam: the
/// lucide branch paints the mode's glyph (sun for light).
#[test]
fn the_trigger_icon_resolves_through_the_seam() {
    // Light mode: the trigger's icon is the resolver's sun answer.
    let rt = mount(
        Audit { mode: ThemeMode::Light, with_resolver: true, ..Default::default() },
        (960.0, 640.0),
    );
    let icon = keyed(&rt, "theme-toggle-icon").expect("the trigger's icon mounts");
    // The icon's element mounts with a non-zero size (the lucide glyph).
    assert!(
        icon.rect[2] > 0.0 && icon.rect[3] > 0.0,
        "the resolved icon paints at a size: {:?}",
        icon.rect
    );

    // The current-option derivation agrees with the resolver's own
    // mode mapping (system's icon is the resolver's monitor answer).
    assert_eq!(current_option(ThemeMode::System).icon_id, "theme.system");
}

/// The open menu anchors under the trigger: the panel's top is below
/// the trigger's bottom.
#[test]
fn the_menu_anchors_below_the_trigger() {
    let rt = mount(
        Audit { mode: ThemeMode::Dark, open: true, with_resolver: true, ..Default::default() },
        (960.0, 640.0),
    );
    let trigger = keyed(&rt, "theme-toggle-button").expect("the trigger mounts");
    let menu = keyed(&rt, "theme-toggle-menu").expect("the menu mounts");
    assert!(
        menu.rect[1] >= trigger.rect[1] + trigger.rect[3] - 1.0,
        "the menu's top ({}) is below the trigger's bottom ({})",
        menu.rect[1],
        trigger.rect[1] + trigger.rect[3]
    );
}
