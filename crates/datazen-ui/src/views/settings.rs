//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;

use crate::constants::*;
use crate::i18n::t;
use crate::icons::{self, section_icon};
use crate::state_def::{AppState, StructureState, WidgetBarsState};
use crate::types::{Section, TableView, Tool};
use crate::ui::field::TextField;
use crate::{splitter, sql_editor};
use datazen_ui_api::{
    ColumnDraft, ConnectionProfile, DashboardWidget, MemoryWorkspace, ProgressSnapshot, QueryApi,
    Setting, SettingsApi, TabKind, Workflow, WorkflowStepResult, WorkspaceApi,
};
use rune::tok::{self, type_};
use rune::{col, row, text, Color, Cx, El};
use rune::{FileDialogOptions, TextEdit, Tx};

// The windows/settings/ai subtree port (T015: AiContextSettings). One mod
// line per ported React file; the ai/ directory mirrors the React layout.
pub mod ai;
// T172: the AI settings section shell (context dir, models block,
// strict-egress safeguard).
pub mod ai_settings_section;
// T198: the AI tool bridge switches (plugins + workflows).
pub mod ai_tool_bridge_section;
// T222: the appearance section (color scheme, theme packs).
pub mod appearance_section;
// T244: the history-data cleanup section.
pub mod data_cleanup_section;
mod data_browsing_live;
mod editor_syntax_live;
// T265: the driver settings section (per-driver blocks over the
// generated entries seam).
pub mod driver_settings_section;
// T304: the generic JSON-schema settings form (the driver section's
// schema seam: every boolean property one toggle row).
pub mod json_schema_settings_form;
// T285: the install-language-pack dialog (pick → review → install).
pub mod install_language_pack_dialog;
// T321: the language-packs card (the installed list, install/remove,
// the active pack's English-fallback-first removal).
/// `windows/settings/McpClientSection.tsx` (T336, split): the MCP
/// client administration surface — the saved stdio server configs
/// (add/edit/delete/enable), the per-server connect/reconnect actions
/// with their error strips, and the runtime connected list with its
/// expandable tool inventories.
pub mod mcp_client_section;
/// `windows/settings/MemorySection.tsx` (T360, split): the AI memory
/// administration surface — the settings card (the first-use privacy
/// gate, auto-learn, the cloud policy), the read-only org list and
/// the entries list (filters, candidates bar, merge, export).
pub mod memory_section;
/// T360's state half: the load walk, the deleted-ids tombstones, the
/// merge plan and the settings-draft rules (split for the file-size
/// rule).
pub mod memory_section_state;

/// `windows/settings/McpSettingsSection.tsx` (T348, split): the MCP
/// host service's administration surface — the stdio toggle, the HTTP
/// listener block, the permission modes, the connection allowlist, the
/// scope textareas, the agent snippet card, the exposed tools and the
/// audit log.
pub mod mcp_settings_section;
/// T348's state half: the view-models, the flow plans, the scope and
/// tools rules and the carried mcpAgentConfig pure functions (split
/// for the file-size rule; T557 absorbs the carry).
pub mod mcp_settings_section_state;

/// T336's state half: the view-models, the env converters, the form
/// draft and the section state machine (split for the file-size rule).
pub mod mcp_client_section_state;
/// `windows/settings/PromptSettingsSection.tsx` (T369): the AI prompt
/// template editor (the driver filter, the scenario cards with their
/// variables hints and source badges, the override save, the reset
/// confirm).
pub mod prompt_settings_section;
pub(crate) mod prompts_live;
pub mod language_packs_card;
// T381: the "?" help hint (hover/focus tooltip with the 120ms close
// grace and the focus-retention rule).
pub mod setting_hint;
// T417: the SQL snippet add/edit/duplicate dialog.
pub mod snippet_edit_dialog;
pub(super) mod snippet_add_live;
// T426: the SQL snippets card (the custom list, the builtin list,
// the export/import row) over the T417 dialog + the confirm hook.
pub mod sql_snippets_card;
// T434: the table-data settings section (the row count mode + the
// auto-only threshold, the default sort, the grid density + toggles)
// with T474's delegated slot.
pub mod table_data_settings_section;
// T441's absorbed lib: the reference-integrity cleanup for
// delete_tunnel (the unbind-then-delete ordering + the rollback).
pub mod tunnel_deletion;
// T441: the tunnel delete confirmation (the usage read, the
// affected list, the unbind-then-delete outcome reporting).
pub mod tunnel_delete_dialog;
// T447's carried lib: the string-field tunnel draft model (T637's
// until it lands) — the validation order + the save conversions.
pub mod tunnel_draft;
// T447: the tunnel create/edit dialog (the open-reset fetch, the
// draft patches, the problem-gated save).
pub mod tunnel_edit_dialog;
// T459: the tunnel live-probe dialog (the target inputs, the
// kind-scoped honesty note, the elapsed/error answers).
pub mod tunnel_test_dialog;
// T638's carried lib (segment 2 — until it lands): the updater's
// pure core (channel/variant reasoning, the manifest defence, the
// progress/result contracts).
pub mod updater;
// T464: the update card (the three channel faces, the check /
// download / restart flows, the restart confirm).
pub mod update_section;
// T469: the global value-presentation rule's edit form (the
// validated save, the same-road sample preview).
pub mod value_presentation_rule_editor;
// T474's split half: the draft / preview pure family (the stored
// read, the PRD §4 anchors, the same-road preview).
pub mod value_presentation_section_helpers;
// T474: the time-display settings section (the draft + explicit
// save, the T11 per-path sync, the editable rule list over T469's
// editor).
pub mod value_presentation_settings_section;
// T453: the tunnel settings section (the secret-free list, the
// usage batch, the copy action, the three dialog mounts).
pub mod tunnel_settings_section;
// T408: the AI skills settings panel (the opt-in switches, the
// custom directory, the discovered listing).
pub mod skills_section;
// T390: the settings content shell (the nav rail + the six own
// sections + the delegated section slots), split with its state.
pub mod settings_content;
// T390: the shell's state (the section selection, the local settings
// mirror, the host reads, the keymap seam).
pub mod settings_content_state;
// T399: the settings subtree's shared primitives (SectionTitle,
// SettingRow, ToggleRow).
pub mod settings_ui;

/// A6: grouped settings navigation (2 groups, 12 sections) with deep-link
/// selection by section key. Only the active section's content renders so
/// navigation switching is observable and addressable.
pub fn settings_page(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    use settings_content_state::SettingsField;

    let active = state.settings_section();
    let shell = settings_content::SettingsContent::new(
        &state.settings_content,
        dark,
        cfg!(target_os = "macos"),
    )
    .nav_space_target(|state: &mut AppState| &mut state.settings_content.nav_space_target)
    .on_select_section(|state: &mut AppState, section| state.set_settings_section(section));
    if active == "general" {
        shell
            .builtin_language(state.builtin_language())
            .on_select_toggle(|state, which| state.settings_content.toggle_select(which))
            .on_select_close(|state| state.settings_content.close_select())
            .on_update_field(|state, field| {
                if let SettingsField::Language(code) = field {
                    state.save_builtin_language(&code);
                }
            })
            .general_footer(
                col()
                    .gap(12.0)
                    .child(settings_error_row(state, theme))
                    .child(favorites_section(state, theme, dark)),
            )
            .build()
    } else {
        shell
            .host_body(settings_content(state, theme, dark, active))
            .build()
    }
}

fn settings_content(state: &AppState, theme: tok::Theme, dark: bool, active: &str) -> El {
    let body = match active {
        "appearance" => appearance_section(state, theme, dark),
        "dataBrowsing" => data_browsing_live::view(state, theme, dark),
        "editor" => editor_section(state, theme, dark),
        "behavior" => behavior_section(state, theme),
        "logging" => placeholder_section("logging", t("settings.logging"), theme),
        "ai" => placeholder_section("ai", t("common.aiAssistant"), theme),
        "prompts" => prompts_live::view(state, dark),
        "mcpServer" => placeholder_section("mcpServer", t("settings.mcp.title"), theme),
        "mcpClient" => placeholder_section("mcpClient", t("settings.mcpClient.title"), theme),
        "tunnels" => placeholder_section("tunnels", t("settings.tunnels.title"), theme),
        "extensions" => placeholder_section("extensions", t("settings.extensions.title"), theme),
        _ => placeholder_section(active, t("settings.title"), theme),
    };
    col()
        .key(format!("settings-content-{active}"))
        .w_full()
        .gap(12.0)
        .child(body)
}

fn appearance_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    use appearance_section::{AppearanceSection, ThemeMode, ThemePreference};

    let preference = ThemePreference { mode: state.theme_mode(), pack_id: None };
    let mut overlays = Vec::new();
    let section = AppearanceSection::new(&preference, &[])
        .supported_modes(&ThemeMode::MODES)
        .show_theme_packs(false)
        .on_mode_change(|state: &mut AppState, mode| state.save_theme_mode(mode))
        .listbox(
            |state| state.appearance_select,
            |state, value| state.appearance_select = value,
        )
        .build(state, dark, &mut overlays);

    col()
        .key("settings-section-appearance")
        .w_full()
        .child(section)
        .child(settings_error_row(state, theme))
        .children(overlays)
}

fn editor_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let k = crate::ui::kit::kit(dark);
    let mut overlays = Vec::new();
    let heading = settings_ui::section_title(
        &k,
        &mut overlays,
        "settings-live-editor-title".into(),
        t("settings.editor"),
        None,
    );
    let font_size = font_size_row(state, dark, &k, &mut overlays);
    col()
        .key("settings-section-editor")
        .w_full()
        .gap(20.0)
        .child(heading)
        .child(font_size)
        .child(editor_syntax_live::font_family_row(state, &k, &mut overlays))
        .child(editor_syntax_live::row(state, dark, &k, &mut overlays))
        .children(state.settings_error.as_ref().map(|_| settings_error_row(state, theme)))
        // Add uses the native name/SQL store; keep existing update and delete.
        .child(snippets_section(state, theme, dark))
        .children(overlays)
}

fn font_size_row(state: &AppState, dark: bool, k: &rune_kit::Kit, overlays: &mut Vec<El>) -> El {
    use crate::ui::slider::Slider;
    use datazen_ui_api::{MAX_EDITOR_FONT_SIZE, MIN_EDITOR_FONT_SIZE};

    let size = state.editor_font_size();
    let label = t("settings.fontSize");
    let slider = Slider::<AppState>::new("settings-font-size", f64::from(size))
        .min(f64::from(MIN_EDITOR_FONT_SIZE))
        .max(f64::from(MAX_EDITOR_FONT_SIZE))
        .step(1.0)
        .label(label)
        .surface_thumb()
        .on_change(|state, size| state.save_editor_font_size(size as u32))
        .build(dark);
    settings_ui::setting_row(
        k,
        overlays,
        "settings-font-size-row".into(),
        label,
        None,
        row()
            .key("settings-font-size-slider")
            .w_full()
            .gap(12.0)
            .center_y()
            .child(col().grow(1.0).min_w(0.0).child(slider))
            .child(
                row().w(48.0).justify_end().child(
                    text(format!("{size}px"))
                        .key("settings-font-size-value")
                        .font(rune::TextStyle::new(14.0, 400.0).lh(20.0))
                        .color(k.c.text_muted),
                ),
            ),
    )
}


fn behavior_section(_state: &AppState, theme: tok::Theme) -> El {
    placeholder_section("behavior", t("settings.behavior"), theme)
}

fn section_title(label: &str, theme: tok::Theme) -> El {
    row()
        .w_full()
        .h(28.0)
        .center_y()
        .child(text(label.to_owned()).font(type_::TITLE).color(theme.text))
}

fn placeholder_section(id: &str, label: &str, theme: tok::Theme) -> El {
    col()
        .key(format!("settings-section-{id}"))
        .w_full()
        .gap(8.0)
        .child(section_title(label, theme))
        .child(
            text(format!("{label}设置在此 Rune 构建中尚不可用"))
                .font(type_::LABEL)
                .color(theme.text_secondary)
            .w_full()
            .wrap(),
        )
}

fn settings_error_row(state: &AppState, theme: tok::Theme) -> El {
    if let Some(error) = &state.settings_error {
        row()
            .key("settings-error")
            .w_full()
            .h(24.0)
            .px(8.0)
            .center_y()
            .radius(6.0)
            .fill(theme.control_off)
            .child(
                text(error.clone())
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            )
    } else {
        row().key("settings-error-empty").w(0.0).h(0.0)
    }
}

fn snippets_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let k = crate::ui::kit::kit(dark);
    let saved = state.host_snippets();
    let mut snippets = snippet_add_live::card_header(k, dark, saved.len());
    if !saved.is_empty() {
        // Existing Native update workflow: key by the saved name, not React's
        // unsupported persistent ID or rename contract.
        snippets = snippets.child(
            row()
                .key("snippet-form")
                .w_full()
                .min_h(28.0)
                .gap(8.0)
                .center_y()
                .flex_wrap()
                .child(
                    TextField::new("snippet-update-name", &state.snippet_name)
                        .height(24.0).width(140.0).pad_x(8.0).radius(7.0)
                        .fill(k.c.surface_base).border(1.0, k.c.border_base)
                        .label(t("settings.nativeSnippet.name")).placeholder(t("settings.nativeSnippet.name"))
                        .on_edit(|state: &mut AppState| &mut state.snippet_name)
                        .build(dark),
                )
                .child(
                    TextField::new("snippet-update-sql", &state.snippet_sql)
                        .height(24.0).width(280.0).min_width(0.0).pad_x(8.0).radius(7.0)
                        .fill(k.c.surface_base).border(1.0, k.c.border_base)
                        .label(t("query.snippets.template")).placeholder(t("query.snippets.template"))
                        .on_edit(|state: &mut AppState| &mut state.snippet_sql)
                        .build(dark),
                )
                .child(
                    row()
                        .key("snippet-update")
                        .role("button").focusable(true).label(t("settings.nativeSnippet.update"))
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text(t("settings.nativeSnippet.update")).font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.update_snippet_form()),
                ),
        );
    }
    for (index, snippet) in saved.iter().enumerate() {
        let target = snippet.clone();
        snippets = snippets.child(
            row()
                .key(format!("snippet-row-{index}"))
                .w_full()
                .h(24.0)
                .gap(8.0)
                .center_y()
                .px(8.0)
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(snippet.name.clone())
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .child(
                    text(snippet.sql.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    row()
                        .key(format!("snippet-delete-{index}"))
                        .role("button")
                        .label(t("common.delete"))
                        .focusable(true)
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(
                            text(t("common.delete"))
                                .font(type_::LABEL)
                                .color(theme.text_secondary),
                        )
                        .on_press(move |state: &mut AppState| state.request_snippet_delete(&target)),
                ),
        );
    }
    if let Some(error) = &state.snippet_error {
        snippets = snippets.child(
            row()
                .key("snippet-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(error.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    snippets
}

/// The favorites section (T6): save the current query, list the host's
/// favorites, open one into a query tab, rename one (the file stays), delete
/// one (the bytes move to the store's trash).
pub fn favorites_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let mut section = col()
        .key("settings-favorites")
        .w_full()
        .gap(6.0)
        .child(
            text(t("query.favorites").to_owned())
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            row()
                .key("favorite-form")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    TextField::new("favorite-title-input", &state.favorite_title)
                        .height(24.0)
                        .grow()
                        .min_width(0.0)
                        .on_edit(|state: &mut AppState| &mut state.favorite_title)
                        .build(dark),
                )
                .child(
                    row()
                        .key("favorite-save")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text("Save current query".to_owned())
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.save_favorite_form()),
                ),
        );
    let favorites = state.host_favorites();
    if favorites.is_empty() {
        section = section.child(
            row()
                .key("favorite-empty")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(
                    text(t("query.noFavorites").to_owned())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    for (index, favorite) in favorites.iter().enumerate() {
        section = section.child(
            row()
                .key(format!("favorite-row-{index}"))
                .w_full()
                .h(24.0)
                .gap(8.0)
                .center_y()
                .px(8.0)
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(favorite.title.clone())
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .child(
                    text(favorite.sql.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    row()
                        .key(format!("favorite-open-{index}"))
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(text("Open").font(type_::LABEL).color(theme.text))
                        .on_press(move |state: &mut AppState| state.open_favorite(index)),
                )
                .child(
                    row()
                        .key(format!("favorite-rename-{index}"))
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(text(t("filter.view.rename")).font(type_::LABEL).color(theme.text))
                        .on_press(move |state: &mut AppState| state.rename_favorite_row(index)),
                )
                .child(
                    row()
                        .key(format!("favorite-delete-{index}"))
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(
                            text(t("common.delete"))
                                .font(type_::LABEL)
                                .color(theme.text_secondary),
                        )
                        .on_press(move |state: &mut AppState| state.delete_favorite_row(index)),
                ),
        );
    }
    if let Some(error) = &state.favorite_error {
        section = section.child(
            row()
                .key("favorite-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(error.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    section
}

/// The backup page: start, stop, and the same progress panel as exports.
pub fn backup_section(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("backup-section")
        .w_full()
        .gap(6.0)
        .child(
            text("Backup".to_owned())
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            row()
                .key("backup-controls")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("backup-start")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text(t("backup.startBackup")).font(type_::LABEL).color(theme.text))
                        .on_press_tx(|state: &mut AppState, tx: &mut Tx| {
                            state.begin_backup_form(tx)
                        }),
                )
                .child(
                    row()
                        .key("backup-stop")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text(t("common.stop")).font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.stop_backup()),
                )
                .child(
                    row()
                        .key("backup-refresh")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Refresh").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.refresh_backup_progress()),
                ),
        );
    if let Some(error) = &state.backup_error {
        section = section.child(
            row()
                .key("backup-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(error.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    if let Some(snapshot) = &state.backup_snapshot {
        section = section.child(progress_panel(snapshot, "backup", theme));
    }
    section
}

/// The import wizard's first two steps: source, then target table. No target,
/// no next step.
pub fn import_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    if state.desktop.supports_native_transfer() && state.import_step != 0 {
        return super::native_csv_import::view(state, theme, dark);
    }
    let mut section = col().key("import-section").w_full().gap(6.0).child(
        text(t("import.import").to_owned())
            .font(type_::TITLE)
            .color(theme.text),
    );
    match state.import_step {
        0 => section = section.child(
            row()
                .key("import-step-1")
                .w_full()
                .min_h(28.0)
                .gap(8.0)
                .center_y()
                .flex_wrap()
                .child(
                    text("Step 1 · Source".to_owned())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    TextField::new("import-source-input", &state.import_source)
                        .height(24.0)
                        .width(240.0)
                        .on_edit(|state: &mut AppState| &mut state.import_source)
                        .build(dark),
                )
                .child(
                    row()
                        .key("import-browse")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Browse…").font(type_::LABEL).color(theme.text))
                        .on_press_tx(|_state: &mut AppState, tx: &mut Tx| {
                            // T2: an Open dialog fills the source input; a
                            // dialog that cannot show says so honestly.
                            let options = FileDialogOptions {
                                title: "Choose an import source".into(),
                                ..Default::default()
                            };
                            tx.open_file(options, |state: &mut AppState, result, _| match result {
                                Ok(Some(path)) => {
                                    state.import_source = TextEdit::new(path.display().to_string());
                                    state.import_error = None;
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    state.import_error = Some(format!(
                                        "the file picker is unavailable ({error}); type the path instead"
                                    ));
                                }
                            });
                        }),
                )
                .child(
                    row()
                        .key("import-next")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text(t("sync.next")).font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.import_next()),
                ),
        ),
        1 => section = section
            .child(
                row()
                    .key("import-step-2")
                    .w_full()
                    .h(28.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        text("Step 2 · Target table".to_owned())
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    )
                    .child(
                        TextField::new("import-target-input", &state.import_target)
                            .height(24.0)
                            .width(180.0)
                            .on_edit(|state: &mut AppState| &mut state.import_target)
                            .build(dark),
                    )
                    .child(
                        row()
                            .key("import-finish")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text(t("onboarding.common.finish")).font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.import_finish()),
                    )
                    .child(
                        row()
                            .key("import-back")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text(t("common.back")).font(type_::LABEL).color(theme.text_secondary))
                            .on_press(|state: &mut AppState| state.import_back()),
                    ),
            ),
        _ => section = section.child(
            row()
                .key("import-ready")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!(
                        "Import ready: {} into {}",
                        state.import_source.text.trim(),
                        state.import_target.text.trim()
                    ))
                    .font(type_::LABEL)
                    .color(theme.text),
                ),
        ),
    }
    if let Some(error) = &state.import_error {
        section = section.child(
            row()
                .key("import-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(error.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    section
}
pub fn export_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let mut section = col()
        .key("export-section")
        .w_full()
        .gap(6.0)
        .child(
            text(t("export.export").to_owned())
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            row()
                .key("export-form")
                .w_full()
                .min_h(28.0)
                .gap(8.0)
                .center_y()
                .flex_wrap()
                .child(
                    row()
                        .key("export-format")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("format: {}", state.export_format()))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_export_format()),
                )
                .child(
                    TextField::new("export-range-input", &state.export_range)
                        .height(24.0)
                        .width(220.0)
                        .on_edit(|state: &mut AppState| &mut state.export_range)
                        .build(dark),
                )
                .child(
                    row()
                        .key("export-start")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text(t("export.export")).font(type_::LABEL).color(theme.text))
                        .on_press_tx(|state: &mut AppState, tx: &mut Tx| {
                            state.begin_export_form(tx)
                        }),
                ),
        );
    if let Some(error) = &state.export_error {
        section = section.child(
            row()
                .key("export-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(error.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    if let Some(job) = state.export_job {
        section = section.child(
            row()
                .key("export-job")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!("job #{job}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    row()
                        .key("export-refresh")
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(text("Refresh").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.refresh_export_progress()),
                ),
        );
    }
    if let Some(snapshot) = &state.export_snapshot {
        section = section.child(progress_panel(snapshot, "export", theme));
    }
    section
}

/// One job's progress drawn from a single snapshot: status, percent, the log.
/// Export and backup share this component.
pub fn progress_panel(snapshot: &ProgressSnapshot, prefix: &str, theme: tok::Theme) -> El {
    let mut panel = col()
        .key(format!("{prefix}-progress"))
        .w_full()
        .gap(4.0)
        .px(8.0)
        .py(6.0)
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key(format!("{prefix}-status"))
                .w_full()
                .h(24.0)
                .center_y()
                .child(
                    text(format!("{} · {}%", snapshot.status, snapshot.percent))
                        .font(type_::LABEL)
                        .color(theme.text),
                ),
        );
    for (index, line) in snapshot.log.iter().enumerate() {
        panel = panel.child(
            row()
                .key(format!("{prefix}-log-{index}"))
                .w_full()
                .h(18.0)
                .center_y()
                .child(
                    text(line.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    panel
}
