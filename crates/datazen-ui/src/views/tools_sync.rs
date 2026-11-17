//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;
use crate::i18n::t;

use crate::constants::*;
use crate::icons::{self, section_icon};
use crate::state_def::{AppState, StructureState, WidgetBarsState};
use crate::types::{Section, TableView, Tool};
use crate::{splitter, sql_editor};
use datazen_ui_api::{
    ColumnDraft, ConnectionProfile, DashboardWidget, MemoryWorkspace, ProgressSnapshot, QueryApi,
    Setting, SettingsApi, TabKind, Workflow, WorkflowStepResult, WorkspaceApi,
};
use rune::tok::{self, type_};
use rune::{col, row, text, text_input, Color, Cx, El};
use rune::{FileDialogOptions, TextEdit, Tx};

pub(crate) fn tools_page(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let entry = |key: &str, label: &str, tool: Tool, active: bool| {
        row()
            .key(key)
            .h(24.0)
            .px(10.0)
            .center_y()
            .radius(6.0)
            .fill(if active {
                tok::accent(0, true)
            } else {
                theme.control_off
            })
            .child(text(label).font(type_::LABEL).color(theme.text))
            .on_press(move |state: &mut AppState| state.set_tool(tool))
    };
    col()
        .key("tools-page")
        .w_full()
        .gap(8.0)
        .child(
            row()
                .key("tool-switcher")
                .w_full()
                .h(28.0)
                .gap(6.0)
                .center_y()
                .child(
                    text("Data tools".to_owned())
                        .font(type_::TITLE)
                        .color(theme.text),
                )
                .child(entry(
                    "tool-sync",
                    "Sync",
                    Tool::Sync,
                    state.tool == Tool::Sync,
                ))
                .child(entry(
                    "tool-compare",
                    t("sync.compare"),
                    Tool::Compare,
                    state.tool == Tool::Compare,
                ))
                .child(entry(
                    "tool-transfer",
                    "Transfer",
                    Tool::Transfer,
                    state.tool == Tool::Transfer,
                ))
                .child(entry(
                    "tool-search",
                    "Search values",
                    Tool::ValueSearch,
                    state.tool == Tool::ValueSearch,
                )),
        )
        .child(match state.tool {
            Tool::Sync => sync_tool(state, theme, dark),
            Tool::Compare => compare_tool(state, theme),
            Tool::Transfer => transfer_tool(state, theme),
            Tool::ValueSearch => value_search_tool(state, theme, dark),
        })
}
pub(crate) fn sync_tool(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let mut section = col().key("sync-tool").w_full().gap(6.0);
    match state.sync_step {
        0 => {
            section = section.child(
                row()
                    .key("sync-step-1")
                    .w_full()
                    .h(28.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        row()
                            .key("sync-source")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(
                                text(format!("source: {}", state.sync_source))
                                    .font(type_::LABEL)
                                    .color(theme.text),
                            )
                            .on_press(|state: &mut AppState| state.cycle_sync_source()),
                    )
                    .child(
                        row()
                            .key("sync-target")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(
                                text(format!("target: {}", state.sync_target))
                                    .font(type_::LABEL)
                                    .color(theme.text),
                            )
                            .on_press(|state: &mut AppState| state.cycle_sync_target()),
                    )
                    .child(
                        row()
                            .key("sync-endpoints-next")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text(t("sync.next")).font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.sync_next_endpoints()),
                    ),
            )
        }
        1 => {
            section = section.child(
                row()
                    .key("sync-step-1-done")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(
                        text(format!("{} → {}", state.sync_source, state.sync_target))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            );
            let mut tables = col().key("sync-step-2").w_full().gap(4.0);
            for table in state.workspace.connection_tables(&state.sync_source) {
                let selected = state.sync_tables.contains(&table);
                tables = tables.child(
                    row()
                        .key(format!("sync-table-{table}"))
                        .w(220.0)
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(if selected {
                            tok::accent(0, true)
                        } else {
                            theme.control_off
                        })
                        .child(text(table.clone()).font(type_::LABEL).color(theme.text))
                        .on_press(move |state: &mut AppState| state.toggle_sync_table(&table)),
                );
            }
            section = section.child(tables).child(
                row()
                    .key("sync-table-controls")
                    .w_full()
                    .h(28.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        row()
                            .key("sync-select-all")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Select all").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.sync_select_all()),
                    )
                    .child(
                        row()
                            .key("sync-clear")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text(t("filter.clear")).font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.sync_clear_tables()),
                    )
                    .child(
                        row()
                            .key("sync-tables-next")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text(t("sync.next")).font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.sync_next_tables()),
                    ),
            );
        }
        2 => {
            section = section.child(
                row()
                    .key("sync-step-1-done")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(
                        text(format!("{} → {}", state.sync_source, state.sync_target))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            );
            let mut options = col().key("sync-step-3").w_full().gap(4.0);
            for (index, label) in AppState::SYNC_OPTION_LABELS.iter().enumerate() {
                let on = state.sync_options[index];
                options = options.child(
                    row()
                        .key(format!("sync-option-{index}"))
                        .w(260.0)
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(if on {
                            tok::accent(0, true)
                        } else {
                            theme.control_off
                        })
                        .child(
                            text(format!("{}: {}", if on { "on" } else { "off" }, label))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(move |state: &mut AppState| state.toggle_sync_option(index)),
                );
            }
            section = section.child(options).child(
                row()
                    .key("sync-preview-next")
                    .w(120.0)
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(text(t("common.preview")).font(type_::LABEL).color(theme.text))
                    .on_press(|state: &mut AppState| state.sync_show_preview()),
            );
        }
        3 => {
            section = section.child(
                row()
                    .key("sync-step-1-done")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(
                        text(format!("{} → {}", state.sync_source, state.sync_target))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            );
            if let Some(grid) = &state.sync_preview_grid {
                section = section.child(crate::data_grid::result_panel(
                    grid,
                    crate::data_grid::GridSlot::SyncPreview,
                    theme.text,
                    theme.text_secondary,
                    theme.control_off,
                    dark,
                ));
            }
            section = section.child(
                row()
                    .key("sync-run-controls")
                    .w_full()
                    .h(28.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        row()
                            .key("sync-run")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Run sync").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.sync_run()),
                    )
                    .child(
                        row()
                            .key("sync-refresh")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Refresh").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.refresh_sync_progress()),
                    ),
            );
            if let Some(snapshot) = &state.sync_snapshot {
                section = section.child(progress_panel(snapshot, "sync", theme));
            }
        }
        _ => {
            section = section.child(
                row()
                    .key("sync-step-1-done")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(
                        text(format!("{} → {}", state.sync_source, state.sync_target))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            )
        }
    }
    if let Some(error) = &state.sync_error {
        section = section.child(
            row()
                .key("sync-error")
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
