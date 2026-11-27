//! T222 audit: the AppearanceSection port, verified against the React
//! source re-read line by line and
//! `windows/settings/__tests__/AppearanceSection.test.tsx` (every case
//! mirrored). Element evidence comes from `Runtime::elements()`,
//! presses go through real pointer events.
//!
//! Tester-written: every expectation below is re-derived from the React
//! files.

use rune::{col, Env, El, Runtime};

use datazen_ui::views::appearance_section::{
    collect_theme_options, encode_wapp_theme_pack_id, parse_wapp_theme_pack_id, resolve_pack_choice,
    AppearanceSection, AppearanceWappTheme, AppearanceWappVm, PackChoice, ThemeMode,
    ThemePreference, BUILTIN_PACK_VALUE,
};

fn wapp(id: &str, enabled: bool, themes: &[(&str, &str)]) -> AppearanceWappVm {
    AppearanceWappVm {
        id: id.to_owned(),
        enabled,
        themes: themes
            .iter()
            .map(|(id, name)| AppearanceWappTheme {
                id: id.to_string(),
                name: name.to_string(),
            })
            .collect(),
    }
}

fn acme() -> AppearanceWappVm {
    wapp(
        "acme.bill-audit",
        true,
        &[("midnight-blue", "Midnight Blue"), ("solar", "Solar")],
    )
}

#[derive(Clone)]
struct AuditState {
    theme: ThemePreference,
    wapps: Vec<AppearanceWappVm>,
    error: Option<String>,
    open: Option<usize>,
    dark: bool,
    pack_adapter_enabled: bool,
    mode_writes: Vec<ThemeMode>,
    pack_writes: Vec<PackChoice>,
}

impl AuditState {
    fn scene() -> Self {
        Self {
            theme: ThemePreference::default(),
            wapps: vec![acme()],
            error: None,
            open: None,
            dark: true,
            pack_adapter_enabled: true,
            mode_writes: Vec::new(),
            pack_writes: Vec::new(),
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let section = AppearanceSection::<AuditState>::new(&s.theme, &s.wapps)
        .error(s.error.as_deref())
        .show_theme_packs(s.pack_adapter_enabled)
        .on_mode_change(|s: &mut AuditState, mode| {
            // The host's `{...theme, mode}`: the pack is kept.
            s.theme.mode = mode;
            s.mode_writes.push(mode);
        })
        .on_pack_change(|s: &mut AuditState, choice| {
            match &choice {
                PackChoice::Builtin => s.theme.pack_id = None,
                PackChoice::Pack(id) => s.theme.pack_id = Some(id.clone()),
            }
            s.pack_writes.push(choice);
        })
        .listbox(|s: &AuditState| s.open, |s: &mut AuditState, open| s.open = open)
        .build(s, s.dark, &mut layer);
    col().max_w(640.0).h_full().child(section).children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt =
        Runtime::new(scene, view, (960.0, 640.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

/// A listbox option's row key (the kit select derives `{key}-{value}`
/// rows; the audit finds them by their visible text instead).
fn option_text(rt: &Runtime<AuditState>, needle: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| !e.exiting && e.text.as_deref().is_some_and(|text| text == needle))
}

// ---- the carried pure functions (themePackApply's two) ----------------------

#[test]
fn pack_ids_encode_and_parse_strictly() {
    let encoded = encode_wapp_theme_pack_id("acme.bill-audit", "midnight-blue");
    assert_eq!(encoded, "wapp:acme.bill-audit:midnight-blue");
    assert_eq!(
        parse_wapp_theme_pack_id(Some(&encoded)),
        Some(("acme.bill-audit".to_owned(), "midnight-blue".to_owned()))
    );
    // The strict rejections: absent, no prefix, empty wapp or theme.
    assert_eq!(parse_wapp_theme_pack_id(None), None);
    assert_eq!(parse_wapp_theme_pack_id(Some("acme:x")), None);
    assert_eq!(parse_wapp_theme_pack_id(Some("wapp:nosep")), None);
    assert_eq!(parse_wapp_theme_pack_id(Some("wapp::theme")), None);
    assert_eq!(parse_wapp_theme_pack_id(Some("wapp:wapp:")), None);
    // A colon inside the wapp id is fine (the FIRST separator splits).
    let nested = encode_wapp_theme_pack_id("a:b", "t");
    assert_eq!(parse_wapp_theme_pack_id(Some(&nested)), Some(("a".to_owned(), "b:t".to_owned())));
}

// ---- collectThemeOptions ----------------------------------------------------

#[test]
fn only_enabled_wapps_contribute_options() {
    let wapps = vec![
        acme(),
        wapp("acme.disabled-themes", false, &[("ghost", "Ghost")]),
    ];
    let options = collect_theme_options(&wapps);
    let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, vec!["Midnight Blue", "Solar"]);
    // The pack ids are encoded.
    assert_eq!(options[0].pack_id, encode_wapp_theme_pack_id("acme.bill-audit", "midnight-blue"));
    assert!(!labels.contains(&"Ghost"), "a disabled wapp's themes are hidden");
}

// ---- the no-op guard (resolve_pack_choice) -----------------------------------

#[test]
fn the_pack_guard_collapses_the_sentinel_and_rejects_no_ops() {
    let builtin = ThemePreference { mode: ThemeMode::Dark, pack_id: None };
    // Builtin while builtin: nothing to apply.
    assert_eq!(resolve_pack_choice(BUILTIN_PACK_VALUE, &builtin), None);
    // The sentinel with a pack active: back to builtin.
    assert_eq!(resolve_pack_choice(BUILTIN_PACK_VALUE, &builtin), None);
    let solar = encode_wapp_theme_pack_id("acme.bill-audit", "solar");
    let with_pack = ThemePreference { mode: ThemeMode::Dark, pack_id: Some(solar.clone()) };
    assert_eq!(
        resolve_pack_choice(BUILTIN_PACK_VALUE, &with_pack),
        Some(PackChoice::Builtin)
    );
    // The same pack again: nothing.
    assert_eq!(resolve_pack_choice(&solar, &with_pack), None);
    // A different pack applies.
    let blue = encode_wapp_theme_pack_id("acme.bill-audit", "midnight-blue");
    assert_eq!(
        resolve_pack_choice(&blue, &with_pack),
        Some(PackChoice::Pack(blue.clone()))
    );
    // A pack from builtin applies.
    assert_eq!(
        resolve_pack_choice(&blue, &builtin),
        Some(PackChoice::Pack(blue))
    );
}

// ---- the runtime tree (React tests 1-11) -------------------------------------

/// React test 1: both selects render, the scheme shows the current
/// mode, the theme shows the built-in default when no pack is active.
#[test]
fn mount_shows_both_selects_with_the_builtin_default() {
    let rt = rt(AuditState::scene());
    assert!(has(&rt, "appearance-section"));
    assert!(has(&rt, "appearance-color-scheme-select-trigger"));
    assert!(has(&rt, "appearance-theme-select-trigger"));
    // Dark is the default mode: its label shows on the trigger.
    assert!(option_text(&rt, "深色"), "the scheme trigger shows 深色 (theme.dark)");
    assert!(option_text(&rt, "内置默认"), "the pack trigger shows the builtin default");
}

#[test]
fn appearance_trigger_matches_field_size_inset_and_theme_tokens_without_a_fake_pack_select() {
    for (dark, inset, ink, chevron_ink) in [
        (false, "#ffffffff", "#26364dff", "#8498afff"),
        (true, "#0a0d14ff", "#e6eaf2ff", "#5f6879ff"),
    ] {
        let mut scene = AuditState::scene();
        scene.pack_adapter_enabled = false;
        scene.dark = dark;
        scene.theme.mode = if dark { ThemeMode::Dark } else { ThemeMode::Light };
        let rt = rt(scene);
        let elements = rt.elements();
        let find = |key: &str| elements.iter()
            .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
            .unwrap_or_else(|| panic!("missing {key}"));
        let scheme = find("appearance-color-scheme-select-trigger");
        let status = find("appearance-builtin-theme-status");
        let label = elements.iter().find(|e| {
            e.path.starts_with(&format!("{}/", scheme.path)) && e.text.is_some()
        }).unwrap();
        assert_eq!(scheme.rect[3], 36.0);
        assert_eq!(scheme.radius, 9.0);
        assert_eq!(scheme.fill.hex(), inset);
        assert_eq!(label.rect[0] - scheme.rect[0], 11.0);
        assert_eq!(label.rect[3], 20.0);
        assert_eq!(label.text_color.unwrap().hex(), ink);
        assert_eq!(find("chevron").text_color.unwrap().hex(), chevron_ink);
        assert_eq!(scheme.role, "combobox");
        assert!(scheme.focusable);
        assert_eq!(status.fill.hex(), inset);
        assert!(!status.focusable);
        assert!(!has(&rt, "appearance-theme-select-trigger"));
    }
}

#[test]
fn keyboard_opens_the_appearance_field_commits_one_mode_and_returns_focus() {
    use rune::{Key, KeyEvent, Mods};

    let mut scene = AuditState::scene();
    scene.pack_adapter_enabled = false;
    scene.theme.mode = ThemeMode::Light;
    let mut rt = rt(scene);
    const TRIGGER: &str = "appearance-color-scheme-select-trigger";
    let focused = |rt: &Runtime<AuditState>| rt.elements().into_iter()
        .find(|e| e.focused && !e.exiting).and_then(|e| e.key);
    let key = |rt: &mut Runtime<AuditState>, key| {
        rt.key(KeyEvent { key, mods: Mods::default() });
        settle(rt);
    };

    for _ in 0..8 {
        if focused(&rt).as_deref() == Some(TRIGGER) {
            break;
        }
        key(&mut rt, Key::Tab);
    }
    assert_eq!(focused(&rt).as_deref(), Some(TRIGGER));
    key(&mut rt, Key::Down);
    assert_eq!(rt.state.open, Some(0));
    assert!(has(&rt, "listbox"));
    assert_eq!(focused(&rt).as_deref(), Some("option0"));
    assert!(rt.state.mode_writes.is_empty());
    key(&mut rt, Key::Down);
    assert_eq!(focused(&rt).as_deref(), Some("option1"));
    key(&mut rt, Key::Enter);
    assert_eq!(rt.state.theme.mode, ThemeMode::Dark);
    assert_eq!(rt.state.mode_writes, vec![ThemeMode::Dark]);
    assert_eq!(rt.state.open, None);
    assert_eq!(focused(&rt).as_deref(), Some(TRIGGER));
    key(&mut rt, Key::Down);
    assert_eq!(rt.state.open, Some(0));
    key(&mut rt, Key::Escape);
    assert_eq!(rt.state.open, None);
    assert_eq!(focused(&rt).as_deref(), Some(TRIGGER));
    assert_eq!(rt.state.mode_writes, vec![ThemeMode::Dark]);
    assert!(rt.state.pack_writes.is_empty());
}

#[test]
fn reserved_appearance_help_is_real_and_keeps_the_settings_row_stable() {
    let mut scene = AuditState::scene();
    scene.pack_adapter_enabled = false;
    let mut rt = rt(scene);
    let bounds = |rt: &Runtime<AuditState>, key: &str| rt.elements().into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("missing {key}"));
    let title = bounds(&rt, "appearance-section-more-label");
    let mark = bounds(&rt, "appearance-section-more-mark");
    let pack = bounds(&rt, "appearance-builtin-theme-status");
    assert_eq!(title.rect[3], 20.0);
    assert_eq!([mark.rect[2], mark.rect[3]], [16.0, 16.0]);
    assert!((mark.rect[0] - title.rect[0] - title.rect[2] - 4.0).abs() < 0.51);
    assert!(!mark.focusable);
    let hint = datazen_ui::i18n::t("settings.appearance.morePlaceholder");
    assert_eq!(mark.help.as_deref(), Some(hint));
    assert!(rt.help_tooltip().is_none(), "no hover tooltip while idle");

    let (x, y) = rt.find("appearance-section-more-mark").unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.0);
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
    let (tip, tip_rect) = rt.help_tooltip().expect("hover shows the real Rune tooltip");
    assert_eq!(tip, hint);
    assert!(tip_rect[1] + tip_rect[3] < mark.rect[1], "help opens above the reserved row");
    assert_eq!(bounds(&rt, "appearance-builtin-theme-status").rect, pack.rect);
    assert!(rt.state.mode_writes.is_empty());
    assert!(rt.state.pack_writes.is_empty());

    rt.pointer_move(900.0, 620.0);
    settle(&mut rt);
    assert!(rt.help_tooltip().is_none());
}

/// The live native host supports changing mode but does not apply plugin
/// themes. Its pack row reports what is actually rendered, never offers an
/// inert chooser, and leaves the scheme's real listbox functional.
#[test]
fn builtin_only_host_reports_palette_without_offering_pack_changes() {
    let mut scene = AuditState::scene();
    scene.pack_adapter_enabled = false;
    let mut rt = rt(scene);
    let bounds = |rt: &Runtime<AuditState>, key: &str| {
        rt.elements()
            .into_iter()
            .find(|entry| !entry.exiting && entry.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("{key} is laid out"))
    };
    let scheme = bounds(&rt, "appearance-color-scheme-select-trigger");
    let status = bounds(&rt, "appearance-builtin-theme-status");
    let footer = bounds(&rt, "appearance-section-more");
    assert!(!has(&rt, "appearance-theme-select-trigger"));
    assert!(option_text(&rt, "内置默认"));
    assert!(!status.focusable);
    assert_ne!(status.role, "combobox");
    assert_eq!(scheme.rect[3], 36.0);
    assert_eq!(status.rect[3], scheme.rect[3]);
    assert_eq!(status.rect[0], scheme.rect[0]);
    assert_eq!(status.rect[2], scheme.rect[2]);
    assert!((status.rect[1] - scheme.rect[1] - scheme.rect[3] - 16.0).abs() < 0.5);
    assert!((footer.rect[1] - status.rect[1] - status.rect[3] - 32.0).abs() < 0.5);

    press(&mut rt, "appearance-builtin-theme-status");
    assert!(rt.state.open.is_none());
    assert!(rt.state.pack_writes.is_empty());
    press(&mut rt, "appearance-color-scheme-select-trigger");
    press(&mut rt, "option2");
    assert_eq!(rt.state.mode_writes, vec![ThemeMode::System]);
    assert!(rt.state.pack_writes.is_empty());
}

#[test]
fn unsupported_pack_is_not_misrepresented_as_the_builtin_palette() {
    let mut scene = AuditState::scene();
    scene.pack_adapter_enabled = false;
    scene.theme.pack_id = Some(encode_wapp_theme_pack_id("acme.bill-audit", "solar"));
    let rt = rt(scene);
    assert!(!has(&rt, "appearance-builtin-theme-status"));
    assert!(!has(&rt, "appearance-theme-select-trigger"));
    assert!(has(&rt, "appearance-color-scheme-select-trigger"));
}

/// React tests 2-3: the theme select lists enabled wapps' themes;
/// a disabled wapp's themes never appear.
#[test]
fn the_theme_list_holds_enabled_wapps_themes_only() {
    let mut scene = AuditState::scene();
    scene.wapps.push(wapp("acme.disabled-themes", false, &[("ghost", "Ghost")]));
    let mut rt = rt(scene);
    press(&mut rt, "appearance-theme-select-trigger");
    assert!(option_text(&rt, "Midnight Blue"));
    assert!(option_text(&rt, "Solar"));
    assert!(!option_text(&rt, "Ghost"), "disabled wapp themes are hidden");
}

/// React test 4: applying a plugin theme persists the encoded pack id.
#[test]
fn picking_a_theme_writes_the_encoded_pack_id() {
    let mut rt = rt(AuditState::scene());
    press(&mut rt, "appearance-theme-select-trigger");
    press(&mut rt, "option1"); // Midnight Blue
    let state = rt.state.clone();
    assert_eq!(state.pack_writes.len(), 1);
    match &state.pack_writes[0] {
        PackChoice::Pack(id) => {
            assert_eq!(id, &encode_wapp_theme_pack_id("acme.bill-audit", "midnight-blue"))
        }
        other => panic!("a pack pick, not {other:?}"),
    }
    // The mode rode along untouched (the host kept it).
    assert_eq!(state.theme.mode, ThemeMode::Dark);
}

/// React test 5: switching back to the built-in default writes null.
#[test]
fn picking_the_builtin_writes_none() {
    let mut scene = AuditState::scene();
    scene.theme.pack_id = Some(encode_wapp_theme_pack_id("acme.bill-audit", "solar"));
    let mut rt = rt(scene);
    press(&mut rt, "appearance-theme-select-trigger");
    press(&mut rt, "option0"); // the builtin sentinel
    let state = rt.state.clone();
    assert_eq!(state.pack_writes, vec![PackChoice::Builtin]);
    assert!(state.theme.pack_id.is_none());
}

/// React test 6: picking the already-active theme re-applies nothing.
#[test]
fn picking_the_active_theme_is_a_no_op() {
    let mut scene = AuditState::scene();
    scene.theme.pack_id = Some(encode_wapp_theme_pack_id("acme.bill-audit", "solar"));
    let mut rt = rt(scene);
    press(&mut rt, "appearance-theme-select-trigger");
    press(&mut rt, "option2"); // Solar
    assert!(rt.state.pack_writes.is_empty(), "the guard rejected the no-op");
}

/// React test 7: the color scheme choice persists the new mode (pack
/// kept).
#[test]
fn picking_a_mode_writes_it() {
    let mut scene = AuditState::scene();
    scene.theme.pack_id = Some(encode_wapp_theme_pack_id("acme.bill-audit", "solar"));
    let mut rt = rt(scene);
    press(&mut rt, "appearance-color-scheme-select-trigger");
    press(&mut rt, "option2"); // system
    let state = rt.state.clone();
    assert_eq!(state.mode_writes, vec![ThemeMode::System]);
    assert_eq!(state.theme.mode, ThemeMode::System);
    // The pack was kept ({...theme, mode}).
    assert!(state.theme.pack_id.is_some());
}

/// React test 8: an apply failure shows the error line.
#[test]
fn an_apply_failure_shows_the_error_line() {
    let mut scene = AuditState::scene();
    scene.error = Some("tokens.css missing".to_owned());
    let errored = rt(scene);
    assert!(has(&errored, "appearance-error"));
    assert!(text_of(&errored, "appearance-error").contains("tokens.css missing"));
    assert!(text_of(&errored, "appearance-error").contains("应用主题失败"));
}

/// React test 9: the orphan hint when the persisted wapp theme is no
/// longer offered.
#[test]
fn the_orphan_hint_shows_for_a_missing_theme() {
    let mut scene = AuditState::scene();
    scene.theme.pack_id = Some(encode_wapp_theme_pack_id("acme.gone-plugin", "vanished"));
    let orphan = rt(scene);
    assert!(has(&orphan, "appearance-orphan-hint"));
    assert!(!text_of(&orphan, "appearance-orphan-hint").is_empty());
    // ...and no hint while the pack is offered or builtin.
    let offered = rt(AuditState::scene());
    assert!(!has(&offered, "appearance-orphan-hint"));
}

/// React test 10's seam: the mount effect's fetch is the host's step
/// (nothing to render; the empty-theme hint is the visible half —
/// React test 11).
#[test]
fn the_empty_theme_list_hides_the_pack_hint_tooltip() {
    // The empty hint rides the pack row's "?" tooltip: the trigger
    // exists when no themes are contributed.
    let mut scene = AuditState::scene();
    scene.wapps = vec![wapp("acme.no-themes", true, &[])];
    let bare = rt(scene);
    // The empty hint rides a "?" trigger on the pack row (its tooltip
    // content is hover-gated; the trigger is the mounted half).
    assert!(has(&bare, "appearance-section-pack-row-hint-mark"));
    // With themes contributed, no "?" mark exists (no hint to show).
    let themed = rt(AuditState::scene());
    assert!(!has(&themed, "appearance-section-pack-row-hint-mark"));
}
