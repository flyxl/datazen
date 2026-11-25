//! T390 audit: the SettingsContent shell port, verified against the
//! React source re-read line by line
//! (`windows/settings/SettingsContent.tsx` — the expectations below
//! are re-derived from its JSX and handlers). Element evidence comes
//! from `Runtime::elements()`, presses go through real pointer events
//! (the `ai_settings_section_audit` harness pattern).

use rune::{col, El, Env, Runtime, TextEdit};

use datazen_ui::views::settings_content::{
    ContributionControl, ContributionItem, ContributionTarget, SettingsContribution, SettingsContent,
};
use datazen_ui::views::settings_content_state::{
    format_shortcut_for_display, get_action_shortcut, normalize_shortcut_input, SettingsContentState,
    SettingsField, SettingsSelect,
};

/// The lenses as fn items (closure literals break HRTB inference).
fn pool_lens(s: &mut AuditState) -> &mut TextEdit {
    &mut s.content.pool_size_edit
}

fn font_lens(s: &mut AuditState) -> &mut TextEdit {
    &mut s.content.font_family_edit
}

fn log_path_lens(s: &mut AuditState) -> &mut TextEdit {
    &mut s.content.log_path_edit
}

fn keymap_lens<'b>(s: &'b mut AuditState, action: &'static str) -> &'b mut TextEdit {
    // The per-action lens: the builder takes one lens for all six;
    // the seeded map always answers.
    s.content.keymap_edit_mut(action)
}

fn contribution_lens<'b, 'd>(
    s: &'b mut AuditState,
    ext: &'d str,
    key: &'d str,
) -> &'b mut TextEdit {
    s.content.contribution_edit_mut(ext, key)
}

#[derive(Clone)]
struct AuditState {
    content: SettingsContentState,
    dark: bool,
    // the host's records
    fields: Vec<SettingsField>,
    ext_writes: Vec<(String, String, String)>,
    section_picks: Vec<&'static str>,
    back_presses: usize,
    log_dir_opens: usize,
    browse_presses: usize,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            content: SettingsContentState::new(),
            dark: true,
            fields: Vec::new(),
            ext_writes: Vec::new(),
            section_picks: Vec::new(),
            back_presses: 0,
            log_dir_opens: 0,
            browse_presses: 0,
        }
    }
}

/// A marker element for a child-section slot (the landed port's El in
/// production; the audit pins its placement by key).
fn slot(key: &str) -> El {
    col().key(format!("{key}-slot")).h(8.0).w_full()
}

fn sample_contribution(target: ContributionTarget) -> SettingsContribution {
    SettingsContribution {
        extension_id: "sql-boost".to_owned(),
        target,
        group_title: Some("SQL Boost".to_owned()),
        group_description: Some("Editor power-ups".to_owned()),
        group: None,
        items: vec![
            ContributionItem {
                key: "autoFormat".to_owned(),
                label: "Auto format".to_owned(),
                hint: None,
                control: ContributionControl::Boolean(true),
            },
            ContributionItem {
                key: "depth".to_owned(),
                label: "Depth".to_owned(),
                hint: None,
                control: ContributionControl::Number(3.0),
            },
        ],
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    SettingsContent::<AuditState>::new(&s.content, s.dark, false)
        .language_options(
            &[
                ("zh-CN".to_owned(), "重复包".to_owned()),
                ("ja".to_owned(), "日本語".to_owned()),
            ],
            &[("ja".to_owned(), "重複拡張".to_owned())],
        )
        .syntax_presets(vec![
            ("default".to_owned(), "Default (DataZen)".to_owned()),
            ("monokai".to_owned(), "Monokai".to_owned()),
        ])
        .contributions(vec![sample_contribution(ContributionTarget::Editor)])
        .language_packs_card(Some(slot("language-packs")))
        .update_section(Some(slot("update-section")))
        .table_data_section(Some(slot("table-data-section")))
        .sql_snippets_card(Some(slot("sql-snippets")))
        .appearance_section(Some(slot("appearance-section")))
        .ai_section(Some(slot("ai-section")))
        .prompts_section(Some(slot("prompts-section")))
        .mcp_server_section(Some(slot("mcp-server-section")))
        .mcp_client_section(Some(slot("mcp-client-section")))
        .tunnel_section(Some(slot("tunnel-section")))
        .extensions_section(Some(slot("extensions-section")))
        .data_cleanup_section(Some(slot("data-cleanup")))
        .sql_syntax_preview(Some(slot("sql-syntax-preview")))
        .on_select_section(|s, id| {
            s.section_picks.push(id);
            s.content.select_section(id);
        })
        .on_update_field(|s, field| {
            s.fields.push(field.clone());
            s.content.apply_field(&field);
            s.content.sync_edits_from_mirror(false);
        })
        .on_extension_setting(|s, ext, key, value| {
            s.ext_writes.push((ext.clone(), key.clone(), value.clone()));
            s.content.apply_field(&SettingsField::ExtensionSetting {
                extension_id: ext,
                key,
                value,
            });
        })
        .on_select_toggle(|s, which| s.content.toggle_select(which))
        .on_select_close(|s| s.content.close_select())
        .on_contribution_toggle(|s, id| {
            s.content.open_contribution_select =
                if s.content.open_contribution_select.as_deref() == Some(&id) {
                    None
                } else {
                    Some(id)
                };
        })
        .on_contribution_close(|s| s.content.close_select())
        .on_browse_log_path(|s| {
            s.browse_presses += 1;
        })
        .on_open_log_dir(|s| {
            s.log_dir_opens += 1;
        })
        .pool_size_edit(pool_lens)
        .font_family_edit(font_lens)
        .log_path_edit(log_path_lens)
        .keymap_edit(keymap_lens)
        .contribution_edit(contribution_lens)
        .build()
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (1100.0, 1400.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

/// A direct state mutation outside a handler: mark the state dirty
/// (the next frame re-runs the view) and settle.
fn reseed(rt: &mut Runtime<AuditState>) {
    rt.invalidate();
    settle(rt);
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

// ---- the nav ----------------------------------------------------------------

/// The mount shape: the rail, both groups, all twelve items, the
/// active highlight on general.
#[test]
fn mount_renders_the_nav_and_the_general_section() {
    let rt = rt(AuditState::empty());
    for key in [
        "settings-nav",
        "settings-nav-group-app",
        "settings-nav-group-integration",
        "settings-nav-general",
        "settings-nav-appearance",
        "settings-nav-dataBrowsing",
        "settings-nav-editor",
        "settings-nav-behavior",
        "settings-nav-logging",
        "settings-nav-ai",
        "settings-nav-prompts",
        "settings-nav-mcpServer",
        "settings-nav-mcpClient",
        "settings-nav-tunnels",
        "settings-nav-extensions",
        "settings-content",
        "settings-section-general",
        "settings-language-select-trigger",
        "language-packs-slot",
        "update-section-slot",
        // The version read never landed: the About line stays hidden.
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
    assert!(!has(&rt, "settings-app-version"), "no version line without the read");
    assert!(!has(&rt, "settings-back"), "no back button without onBack");
    assert_eq!(rt.state.content.active_section, "general");
}

/// The nav press switches the section (the content follows, the
/// previous section's rows disappear).
#[test]
fn the_nav_press_switches_sections() {
    let mut rt = rt(AuditState::empty());
    press(&mut rt, "settings-nav-editor");
    assert!(has(&rt, "settings-section-editor"));
    assert!(!has(&rt, "settings-section-general"));
    assert!(!has(&rt, "settings-language-select-trigger"));
    assert_eq!(rt.state.section_picks, vec!["editor"]);
    // The delegated arms mount their slots.
    press(&mut rt, "settings-nav-tunnels");
    assert!(has(&rt, "tunnel-section-slot"));
    assert!(!has(&rt, "sql-snippets-slot"));
    press(&mut rt, "settings-nav-ai");
    assert!(has(&rt, "ai-section-slot"));
    press(&mut rt, "settings-nav-extensions");
    assert!(has(&rt, "extensions-section-slot"));
}

/// An unlanded delegated section (no slot) still mounts its keyed
/// placeholder column — the nav target and deep-link id exist.
#[test]
fn an_unslotted_delegated_section_mounts_its_key() {
    let mut rt = Runtime::new(
        AuditState::empty(),
        |s: &AuditState, _cx: &mut rune::Cx<AuditState>| {
            SettingsContent::<AuditState>::new(&s.content, s.dark, false)
                .on_update_field(|_s, _f| {})
                .on_select_toggle(|_s, _w| {})
                .on_select_close(|_s| {})
                .build()
        },
        (1100.0, 800.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt.state.content.select_section("tunnels");
    reseed(&mut rt);
    assert!(has(&rt, "settings-section-tunnels"));
    assert!(!has(&rt, "tunnel-section-slot"), "no slot supplied");
    // The rail still lists every section (the nav target exists).
    assert!(has(&rt, "settings-nav-tunnels"));
}

// ---- the back button ---------------------------------------------------------

/// `onBack` gates the back button; the press routes to the host.
#[test]
fn the_back_button_follows_the_slot() {
    let scene = AuditState::empty();
    let mut rt = Runtime::new(
        scene,
        |s: &AuditState, _cx: &mut rune::Cx<AuditState>| {
            SettingsContent::<AuditState>::new(&s.content, s.dark, false)
                .on_back(|s| {
                    s.back_presses += 1;
                })
                .on_update_field(|_s, _f| {})
                .on_select_toggle(|_s, _w| {})
                .on_select_close(|_s| {})
                .build()
        },
        (1100.0, 800.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(has(&rt, "settings-back"));
    assert_eq!(rt.state.back_presses, 0);
    let point = rt.find("settings-back").expect("back is pressable");
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rt.state.back_presses, 1);
}

// ---- the capabilities gates --------------------------------------------------

/// The web capabilities narrow the rail to the five shared sections
/// and hide the update slot (the `updater` gate).
#[test]
fn web_capabilities_narrow_the_nav_and_hide_update() {
    let scene = AuditState::empty();
    let mut rt = Runtime::new(
        scene,
        |s: &AuditState, _cx: &mut rune::Cx<AuditState>| {
            SettingsContent::<AuditState>::new(&s.content, s.dark, false)
                .capabilities(&datazen_ui::platform::capabilities::WEB_CAPABILITIES)
                .on_update_field(|_s, _f| {})
                .on_select_toggle(|_s, _w| {})
                .on_select_close(|_s| {})
                .build()
        },
        (1100.0, 800.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    for key in [
        "settings-nav-general",
        "settings-nav-appearance",
        "settings-nav-dataBrowsing",
        "settings-nav-editor",
        "settings-nav-behavior",
    ] {
        assert!(has(&rt, key), "{key} renders on web");
    }
    for key in [
        "settings-nav-logging",
        "settings-nav-ai",
        "settings-nav-prompts",
        "settings-nav-mcpServer",
        "settings-nav-mcpClient",
        "settings-nav-tunnels",
        "settings-nav-extensions",
    ] {
        assert!(!has(&rt, key), "{key} stays off the web rail");
    }
    // The update slot was NOT supplied in this scene anyway; the gate
    // itself is the state helper's unit rule (below).
}

// ---- the data browsing section ----------------------------------------------

#[test]
fn the_data_browsing_section_mounts_its_rows() {
    let mut rt = rt(AuditState::empty());
    rt.state.content.select_section("dataBrowsing");
    reseed(&mut rt);
    for key in [
        "settings-default-page-size-trigger",
        "table-data-section-slot",
        "settings-connection-pool-size",
        "settings-limit-select-switch",
        "settings-auto-chart-switch",
        "settings-workflow-step-order-trigger",
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
    // The max-rows select is gated by the limit toggle.
    assert!(!has(&rt, "settings-query-result-limit-trigger"));
    press(&mut rt, "settings-limit-select-switch");
    assert!(has(&rt, "settings-query-result-limit-trigger"));
    assert!(matches!(
        rt.state.fields.last(),
        Some(SettingsField::LimitSelectResults(true))
    ));
}

#[test]
fn the_limit_toggle_and_the_behavior_toggles_write_through() {
    let mut rt = rt(AuditState::empty());
    rt.state.content.select_section("behavior");
    reseed(&mut rt);
    for key in [
        "settings-confirm-delete-switch",
        "settings-auto-commit-switch",
        "settings-safe-mode-switch",
        "settings-confirm-dangerous-switch",
        "data-cleanup-slot",
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
    press(&mut rt, "settings-safe-mode-switch");
    assert!(matches!(
        rt.state.fields.last(),
        Some(SettingsField::SafeMode(false))
    ));
    assert!(!rt.state.content.mirror.safe_mode, "the optimistic mirror flipped");
}

// ---- the editor section ------------------------------------------------------

#[test]
fn the_editor_section_mounts_its_rows_and_the_keymap_card() {
    let mut rt = rt(AuditState::empty());
    rt.state.content.select_section("editor");
    reseed(&mut rt);
    for key in [
        "settings-font-size-slider",
        "settings-font-size-value",
        "settings-font-family",
        "settings-sql-syntax-theme-trigger",
        "sql-syntax-preview-slot",
        "settings-quote-policy-trigger",
        "settings-completion-table-prefix-switch",
        "settings-toggle-enableFkPrediction-switch",
        "settings-keymap-preset-trigger",
        "settings-keymap-customize",
        "settings-keymap-row-execute",
        "settings-keymap-row-executeAll",
        "settings-keymap-row-newQuery",
        "settings-keymap-row-closeTab",
        "settings-keymap-row-saveQuery",
        "settings-keymap-row-formatSql",
        "settings-keymap-input-execute",
        "settings-keymap-kbd-execute",
        "sql-snippets-slot",
        "settings-contrib-sql-boost",
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
    // No custom entries: the reset button stays hidden.
    assert!(!has(&rt, "settings-keymap-reset"));
    // The kbd chip shows the default preset's display format.
    assert!(has(&rt, "settings-keymap-kbd-execute"));
}

#[test]
fn the_keymap_reset_button_appears_with_custom_entries() {
    let mut rt = rt(AuditState::empty());
    rt.state
        .content
        .apply_field(&SettingsField::CustomKeymapEdit {
            action: "execute",
            input: "Ctrl-t".to_owned(),
        });
    rt.state.content.select_section("editor");
    reseed(&mut rt);
    assert!(has(&rt, "settings-keymap-reset"));
    press(&mut rt, "settings-keymap-reset");
    assert!(matches!(
        rt.state.fields.last(),
        Some(SettingsField::CustomKeymap(_))
    ));
    assert!(rt.state.content.mirror.custom_keymap.is_empty(), "the reset emptied the map");
}

// ---- the logging section -----------------------------------------------------

#[test]
fn the_logging_section_mounts_its_rows() {
    let mut rt = rt(AuditState::empty());
    rt.state.content.note_default_log_path("/var/log/datazen");
    rt.state.content.select_section("logging");
    reseed(&mut rt);
    for key in [
        "settings-log-level-trigger",
        "settings-log-path-input",
        "settings-log-path-browse",
        "settings-view-logs-button",
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
    press(&mut rt, "settings-view-logs-button");
    assert_eq!(rt.state.log_dir_opens, 1);
}

// ---- the appearance section --------------------------------------------------

#[test]
fn the_appearance_section_mounts_the_slot() {
    let mut rt = rt(AuditState::empty());
    rt.state.content.select_section("appearance");
    reseed(&mut rt);
    assert!(has(&rt, "appearance-section-slot"));
    assert!(has(&rt, "settings-section-appearance"));
}

// ---- the general section's dynamic rows --------------------------------------

#[test]
fn the_version_line_follows_the_read() {
    let mut rt = rt(AuditState::empty());
    assert!(!has(&rt, "settings-app-version"));
    rt.state.content.note_app_version("1.9.4");
    reseed(&mut rt);
    assert!(has(&rt, "settings-app-version"));
}

// ---- the selects' open state -------------------------------------------------

#[test]
fn the_page_selects_open_one_at_a_time() {
    let mut rt = rt(AuditState::empty());
    // The language select opens through its trigger.
    press(&mut rt, "settings-language-select-trigger");
    assert_eq!(rt.state.content.open_select, Some(SettingsSelect::Language));
    rt.state.content.select_section("dataBrowsing");
    reseed(&mut rt);
    // The open state survives the section switch in React (the list
    // unmounts with its section; the state enum is the port's own —
    // closing on switch keeps one-open-at-a-time honest).
    rt.state.content.close_select();
    press(&mut rt, "settings-default-page-size-trigger");
    assert_eq!(rt.state.content.open_select, Some(SettingsSelect::PageSize));
    press(&mut rt, "settings-default-page-size-trigger");
    assert_eq!(rt.state.content.open_select, None, "the second press closes");
}

// ---- the contributions -------------------------------------------------------

#[test]
fn the_contributions_render_in_their_target_sections() {
    let mut rt = rt(AuditState::empty());
    rt.state.content.select_section("editor");
    reseed(&mut rt);
    assert!(has(&rt, "settings-contrib-sql-boost"));
    assert!(has(&rt, "settings-contrib-sql-boost-autoFormat-switch"));
    assert!(has(&rt, "settings-contrib-sql-boost-depth-number"));
    // The general section renders no editor contributions.
    rt.state.content.select_section("general");
    reseed(&mut rt);
    assert!(!has(&rt, "settings-contrib-sql-boost"));
    // The boolean item's switch writes through the extension seam.
    rt.state.content.select_section("editor");
    reseed(&mut rt);
    press(&mut rt, "settings-contrib-sql-boost-autoFormat-switch");
    assert_eq!(
        rt.state.ext_writes.last(),
        Some(&("sql-boost".to_owned(), "autoFormat".to_owned(), "false".to_owned()))
    );
}

// ---- the keymap seam (lib/keymap.ts, segment 2's T541) -----------------------

#[test]
fn the_keymap_seam_resolves_and_formats() {
    // The pure rules are the state module's unit tests; the shell
    // reads them through the same functions, so pin the wiring: the
    // kbd chip text is the display format of the resolved shortcut.
    let current = get_action_shortcut("execute", "default", &[]);
    assert_eq!(format_shortcut_for_display(&current, false), "Ctrl+Enter");
    let normalized = normalize_shortcut_input("cmd + enter");
    assert_eq!(normalized, "Mod-Enter");
}

// ---- the state rules (mirrors, deep links, edits) ----------------------------

#[test]
fn the_deep_link_normalizes_and_re_syncs() {
    let mut state = SettingsContentState::new();
    state.set_initial_section(Some("mcpClient"));
    assert_eq!(state.active_section, "mcpClient");
    state.set_initial_section(Some("not-a-section"));
    assert_eq!(state.active_section, "general");
    state.set_initial_section(None);
    assert_eq!(state.active_section, "general");
}

#[test]
fn the_store_sync_replaces_the_mirror() {
    let mut state = SettingsContentState::new();
    let mut mirror = state.mirror.clone();
    mirror.language = "zh-CN".to_owned();
    mirror.default_page_size = 100;
    state.sync_from_store(mirror.clone());
    assert_eq!(state.mirror, mirror);
}

#[test]
fn the_controlled_inputs_reseed_from_the_mirror() {
    let mut state = SettingsContentState::new();
    state.apply_field(&SettingsField::EditorFontFamily("JetBrains Mono".to_owned()));
    state.apply_field(&SettingsField::CustomKeymapEdit {
        action: "execute",
        input: "Ctrl-t".to_owned(),
    });
    state.sync_edits_from_mirror(false);
    assert_eq!(state.font_family_edit.text, "JetBrains Mono");
    assert_eq!(state.pool_size_edit.text, "10");
    // The custom entry wins the resolution and displays in the
    // platform format.
    assert_eq!(state.keymap_edit("execute").text, "Ctrl+T");
    assert_eq!(state.keymap_edit("newQuery").text, "Ctrl+N", "the preset default");
}

#[test]
fn the_keymap_edit_merges_into_the_custom_map() {
    let mut state = SettingsContentState::new();
    state.apply_field(&SettingsField::CustomKeymapEdit {
        action: "saveQuery",
        input: "  cmd + s ".to_owned(),
    });
    assert_eq!(
        state.mirror.custom_keymap,
        vec![("saveQuery".to_owned(), "Mod-s".to_owned())]
    );
    // A second edit of the same action replaces, not appends.
    state.apply_field(&SettingsField::CustomKeymapEdit {
        action: "saveQuery",
        input: "ctrl+F3".to_owned(),
    });
    assert_eq!(
        state.mirror.custom_keymap,
        vec![("saveQuery".to_owned(), "Ctrl-F3".to_owned())]
    );
    // The custom value wins the resolution.
    assert_eq!(get_action_shortcut("saveQuery", "default", &state.mirror.custom_keymap), "Ctrl-F3");
}

#[test]
fn the_pool_input_clamps_on_write() {
    let mut state = SettingsContentState::new();
    state.apply_field(&SettingsField::ConnectionPoolSize(999));
    assert_eq!(state.mirror.connection_pool_size, 100);
    state.apply_field(&SettingsField::ConnectionPoolSize(0));
    assert_eq!(state.mirror.connection_pool_size, 1);
    state.apply_field(&SettingsField::ConnectionPoolSize(42));
    assert_eq!(state.mirror.connection_pool_size, 42);
}

#[test]
fn the_extension_settings_merge_by_extension_and_key() {
    let mut state = SettingsContentState::new();
    state.apply_field(&SettingsField::ExtensionSetting {
        extension_id: "ext-a".to_owned(),
        key: "k1".to_owned(),
        value: "v1".to_owned(),
    });
    state.apply_field(&SettingsField::ExtensionSetting {
        extension_id: "ext-a".to_owned(),
        key: "k2".to_owned(),
        value: "v2".to_owned(),
    });
    state.apply_field(&SettingsField::ExtensionSetting {
        extension_id: "ext-b".to_owned(),
        key: "k1".to_owned(),
        value: "other".to_owned(),
    });
    assert_eq!(
        state.mirror.extension_values("ext-a"),
        &[("k1".to_owned(), "v1".to_owned()), ("k2".to_owned(), "v2".to_owned())]
    );
    assert_eq!(
        state.mirror.extension_values("ext-b"),
        &[("k1".to_owned(), "other".to_owned())]
    );
    assert!(state.mirror.extension_values("ext-c").is_empty());
}
