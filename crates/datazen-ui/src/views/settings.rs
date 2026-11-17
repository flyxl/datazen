//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;

use crate::constants::*;
use crate::i18n::t;
use crate::icons::{self, section_icon};
use crate::settings_state::{SettingsSectionDef, SETTINGS_NAV_GROUPS, SETTINGS_SECTIONS};
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

/// A6: grouped settings navigation (2 groups, 12 sections) with deep-link
/// selection by section key. Only the active section's content renders so
/// navigation switching is observable and addressable.
pub(crate) fn settings_page(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let active = state.settings_section();
    // The page is one row of the page column, which also holds the content
    // toolbar, the tab strip and the status bar: `h_full` measured the whole
    // column (the padding box) and pushed the status bar past the window's
    // bottom edge. A grown leftover keeps the page inside its slot.
    row()
        .key("settings-page")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .gap(0.0)
        .child(settings_nav(state, theme, active))
        .child(settings_content(state, theme, dark, active))
}

fn settings_nav(state: &AppState, theme: tok::Theme, active: &str) -> El {
    let mut nav = col()
        .key("settings-nav")
        .w(180.0)
        .h_full()
        .px(8.0)
        .py(8.0)
        .gap(2.0)
        .fill(theme.bg);

    for (group_id, group_label) in SETTINGS_NAV_GROUPS {
        let mut group = col()
            .key(format!("settings-nav-group-{group_id}"))
            .w_full()
            .gap(2.0)
            .pt(10.0)
            .child(
                row()
                    .key(format!("settings-nav-group-label-{group_id}"))
                    .w_full()
                    .h(20.0)
                    .px(12.0)
                    .center_y()
                    .child(
                        text(t(group_label).to_owned())
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            );
        for section in SETTINGS_SECTIONS
            .iter()
            .filter(|section| section.group == group_id)
        {
            let is_active = section.id == active;
            let id = section.id;
            let fill = if is_active {
                theme.control_off
            } else {
                theme.bg
            };
            let label_color = if is_active {
                theme.text
            } else {
                theme.text_secondary
            };
            group = group.child(
                row()
                    .key(format!("settings-nav-{id}"))
                    .w_full()
                    .h(30.0)
                    .px(12.0)
                    .center_y()
                    .radius(6.0)
                    .fill(fill)
                    .child(
                        text(t(section.label_key).to_owned())
                            .font(type_::LABEL)
                            .color(label_color),
                    )
                    .on_press(move |state: &mut AppState| state.set_settings_section(id)),
            );
        }
        nav = nav.child(group);
    }
    nav
}

fn settings_content(state: &AppState, theme: tok::Theme, dark: bool, active: &str) -> El {
    let body = match active {
        "general" => general_section(state, theme, dark),
        "appearance" => appearance_section(state, theme),
        "dataBrowsing" => data_browsing_section(state, theme, dark),
        "editor" => editor_section(state, theme, dark),
        "behavior" => behavior_section(state, theme),
        "logging" => placeholder_section("logging", t("settings.logging"), theme),
        "ai" => placeholder_section("ai", t("common.aiAssistant"), theme),
        "prompts" => placeholder_section("prompts", t("settings.prompts"), theme),
        "mcpServer" => placeholder_section("mcpServer", t("settings.mcp.title"), theme),
        "mcpClient" => placeholder_section("mcpClient", t("settings.mcpClient.title"), theme),
        "tunnels" => placeholder_section("tunnels", t("settings.tunnels.title"), theme),
        "extensions" => placeholder_section("extensions", t("settings.extensions.title"), theme),
        _ => placeholder_section(active, t("settings.title"), theme),
    };
    col()
        .key(format!("settings-content-{active}"))
        .grow(1.0)
        .h_full()
        .px(24.0)
        .py(16.0)
        .gap(12.0)
        .child(body)
}

fn general_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    col()
        .key("settings-section-general")
        .w_full()
        .gap(12.0)
        .child(section_title(t("settings.general"), theme))
        .child(
            text("Application settings and saved queries.".to_owned())
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(favorites_section(state, theme, dark))
}

fn appearance_section(state: &AppState, theme: tok::Theme) -> El {
    let settings = state.ui_settings();
    let dark = settings
        .iter()
        .find_map(|setting| match setting {
            Setting::Theme { dark } => Some(*dark),
            _ => None,
        })
        .unwrap_or(true);
    col()
        .key("settings-section-appearance")
        .w_full()
        .gap(12.0)
        .child(section_title(t("settings.appearance"), theme))
        .child(
            row()
                .key("settings-appearance")
                .w_full()
                .h(32.0)
                .gap(12.0)
                .center_y()
                .child(
                    row()
                        .key("settings-theme")
                        .h(28.0)
                        .px(10.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(if dark { "Theme: Dark" } else { "Theme: Light" }.to_owned())
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.toggle_theme()),
                ),
        )
        .child(settings_error_row(state, theme))
}

fn data_browsing_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let settings = state.ui_settings();
    let page_size = settings
        .iter()
        .find_map(|setting| match setting {
            Setting::TablePageSize { rows } => Some(*rows),
            _ => None,
        })
        .unwrap_or(TABLE_PAGE_SIZE);
    let max_rows = settings
        .iter()
        .find_map(|setting| match setting {
            Setting::QueryMaxRows { rows } => Some(*rows),
            _ => None,
        })
        .unwrap_or(100);
    col()
        .key("settings-section-dataBrowsing")
        .w_full()
        .gap(12.0)
        .child(section_title(t("settings.dataBrowsing"), theme))
        .child(
            row()
                .key("settings-data")
                .w_full()
                .min_h(32.0)
                .gap(8.0)
                .center_y()
                .flex_wrap()
                .child(
                    text(format!("Rows per page: {page_size}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    TextField::new("settings-page-size-input", &state.page_size_input)
                        .height(24.0)
                        .width(90.0)
                        .on_edit(|state: &mut AppState| &mut state.page_size_input)
                        .build(dark),
                )
                .child(
                    row()
                        .key("settings-page-size-set")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Set").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.set_table_page_size()),
                )
                .child(
                    text(format!("Query max rows: {max_rows}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    TextField::new("settings-max-rows-input", &state.max_rows_input)
                        .height(24.0)
                        .width(90.0)
                        .on_edit(|state: &mut AppState| &mut state.max_rows_input)
                        .build(dark),
                )
                .child(
                    row()
                        .key("settings-max-rows-set")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Set").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.set_query_max_rows()),
                ),
        )
        .child(settings_error_row(state, theme))
        .child(export_section(state, theme, dark))
        .child(import_section(state, theme, dark))
        .child(backup_section(state, theme))
}

fn editor_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    col()
        .key("settings-section-editor")
        .w_full()
        .gap(12.0)
        .child(section_title(t("settings.editor"), theme))
        .child(snippets_section(state, theme, dark))
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
    // SQL snippets: add, update, delete.
    let mut snippets = col()
        .key("settings-snippets")
        .w_full()
        .gap(6.0)
        .child(
            text("SQL snippets".to_owned())
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            row()
                .key("snippet-form")
                .w_full()
                .min_h(28.0)
                .gap(8.0)
                .center_y()
                .flex_wrap()
                .child(
                    TextField::new("snippet-name-input", &state.snippet_name)
                        .height(24.0)
                        .width(140.0)
                        .on_edit(|state: &mut AppState| &mut state.snippet_name)
                        .build(dark),
                )
                .child(
                    TextField::new("snippet-sql-input", &state.snippet_sql)
                        .height(24.0)
                        .width(280.0)
                        .min_width(0.0)
                        .on_edit(|state: &mut AppState| &mut state.snippet_sql)
                        .build(dark),
                )
                .child(
                    row()
                        .key("snippet-save")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text(t("common.save")).font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.save_snippet_form()),
                )
                .child(
                    row()
                        .key("snippet-update")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(t("common.save")).font(type_::LABEL).color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.update_snippet_form()),
                ),
        );
    for (index, snippet) in state.host_snippets().iter().enumerate() {
        let name = snippet.name.clone();
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
                        .on_press(move |state: &mut AppState| state.delete_snippet(&name)),
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
pub(crate) fn favorites_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
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
pub(crate) fn backup_section(state: &AppState, theme: tok::Theme) -> El {
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
pub(crate) fn import_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
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
pub(crate) fn export_section(state: &AppState, theme: tok::Theme, dark: bool) -> El {
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
pub(crate) fn progress_panel(snapshot: &ProgressSnapshot, prefix: &str, theme: tok::Theme) -> El {
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
