//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;

use crate::constants::*;
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

use crate::ui::palette;
use rune::{FileDialogOptions, TextEdit, Tx};

pub(crate) fn value_search_tool(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("value-search-tool")
        .w_full()
        .gap(8.0)
        .child(
            text("Value search".to_owned())
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            row()
                .key("value-search-form")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    TextField::new("value-search-input", &state.value_search_value)
                        .height(24.0)
                        .width(220.0)
                        .on_edit(|state: &mut AppState| &mut state.value_search_value)
                        .build(),
                )
                .child(
                    row()
                        .key("value-search-mode")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("mode: {}", state.value_search_mode()))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_value_search_mode()),
                )
                .child(
                    row()
                        .key("value-search-start")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text("Search".to_owned())
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.begin_value_search()),
                )
                .child(
                    row()
                        .key("value-search-cancel")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text("Cancel".to_owned())
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cancel_value_search()),
                ),
        );
    if state.value_search_loading.is_some() {
        section = section.child(
            row()
                .key("value-search-loading")
                .w_full()
                .h(20.0)
                .center_y()
                .child(
                    text("Searching…".to_owned())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    if let Some(error) = &state.value_search_error {
        section = section.child(
            row()
                .key("value-search-error")
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
    if !state.value_search_hits.is_empty() {
        let mut list = col().key("value-search-hits").w_full().gap(4.0);
        for (index, hit) in state.value_search_hits.iter().enumerate() {
            let index_for_press = index;
            list = list.child(
                row()
                    .key(format!("value-hit-{index}"))
                    .w_full()
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(
                        text(format!(
                            "{} · {} · {} rows",
                            hit.table, hit.column, hit.matches
                        ))
                        .font(type_::LABEL)
                        .color(theme.text),
                    )
                    .on_press(move |state: &mut AppState| {
                        state.locate_value_search_hit(index_for_press);
                    }),
            );
        }
        section = section.child(list);
    } else if state.value_search_loading.is_none() && state.value_search_error.is_none() {
        section = section.child(
            row()
                .key("value-search-empty")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(
                    text("No hits yet".to_owned())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    section
}

/// The transfer tool: pair the two tables' columns; unmapped columns are
/// listed explicitly.
pub(crate) fn transfer_tool(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("transfer-tool")
        .w_full()
        .gap(6.0)
        .child(
            row()
                .key("transfer-tables")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("transfer-left-table")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("left: {}", state.transfer_left))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_transfer_table(true)),
                )
                .child(
                    row()
                        .key("transfer-right-table")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("right: {}", state.transfer_right))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_transfer_table(false)),
                ),
        )
        .child(
            row()
                .key("transfer-pair-controls")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("transfer-left-col")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("col: {}", state.transfer_pair_left))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_transfer_pair_column(true)),
                )
                .child(
                    row()
                        .key("transfer-right-col")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("col: {}", state.transfer_pair_right))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_transfer_pair_column(false)),
                )
                .child(
                    row()
                        .key("transfer-pair")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Pair").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.pair_transfer_columns()),
                ),
        );
    for (index, (left, right)) in state.transfer_pairs.iter().enumerate() {
        section = section.child(
            row()
                .key(format!("transfer-pair-row-{index}"))
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!("{left} → {right}"))
                        .font(type_::LABEL)
                        .color(theme.text),
                ),
        );
    }
    for (side, key) in [
        (true, "transfer-unmapped-left"),
        (false, "transfer-unmapped-right"),
    ] {
        let label = if side { "left" } else { "right" };
        let unmapped = state.transfer_unmapped(side);
        let mut block = col()
            .key(key)
            .w_full()
            .gap(2.0)
            .px(8.0)
            .py(4.0)
            .radius(6.0)
            .fill(theme.control_off)
            .child(
                text(format!("unmapped {label}: {}", unmapped.join(", ")))
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            );
        let _ = &mut block;
        section = section.child(block);
    }
    // D10: confirm before the port hands out a job; cancelling leaves none.
    if state.transfer_confirming {
        section = section.child(
            row()
                .key("transfer-confirm")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text("Transfer now?".to_owned())
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .child(
                    row()
                        .key("transfer-yes")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text("Confirm".to_owned())
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.confirm_transfer()),
                )
                .child(
                    row()
                        .key("transfer-no")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text("Cancel".to_owned())
                                .font(type_::LABEL)
                                .color(theme.text_secondary),
                        )
                        .on_press(|state: &mut AppState| state.cancel_transfer()),
                ),
        );
    } else {
        section = section.child(
            row()
                .key("transfer-start")
                .w(140.0)
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text("Transfer".to_owned())
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .on_press(|state: &mut AppState| state.begin_transfer_confirm()),
        );
    }
    if let Some(error) = &state.transfer_error {
        section = section.child(
            row()
                .key("transfer-error")
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
    if let Some(job) = state.transfer_job {
        section = section.child(
            row()
                .key("transfer-job")
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
                        .key("transfer-refresh")
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(text("Refresh").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.refresh_transfer_progress()),
                ),
        );
    }
    if let Some(snapshot) = &state.transfer_snapshot {
        section = section.child(progress_panel(snapshot, "transfer", theme));
    }
    section
}

/// The structure comparison. Step 1: the objects, same names marked.
pub(crate) fn compare_tool(state: &AppState, theme: tok::Theme) -> El {
    let objects = state
        .workspace
        .compare_objects(&state.compare_source, &state.compare_target)
        .unwrap_or_default();
    let mut section = col().key("compare-tool").w_full().gap(6.0).child(
        row()
            .key("compare-endpoints")
            .w_full()
            .h(28.0)
            .gap(8.0)
            .center_y()
            .child(
                row()
                    .key("compare-source")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(
                        text(format!("source: {}", state.compare_source))
                            .font(type_::LABEL)
                            .color(theme.text),
                    )
                    .on_press(|state: &mut AppState| state.cycle_compare_source()),
            )
            .child(
                row()
                    .key("compare-target")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(
                        text(format!("target: {}", state.compare_target))
                            .font(type_::LABEL)
                            .color(theme.text),
                    )
                    .on_press(|state: &mut AppState| state.cycle_compare_target()),
            ),
    );
    let mut list = col().key("compare-objects").w_full().gap(4.0);
    for object in &objects {
        let mark = if object.present_on_target {
            format!("compare-match-{}", object.name)
        } else {
            format!("compare-missing-{}", object.name)
        };
        let label = format!(
            "{} {}",
            object.name,
            if object.present_on_target {
                "= both ends"
            } else {
                "= source only"
            }
        );
        list = list.child(
            row()
                .key(format!("compare-object-{}", object.name))
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(label).font(type_::LABEL).color(theme.text))
                .child(row().key(mark).w(10.0).h(10.0).radius(5.0).fill(
                    if object.present_on_target {
                        palette::status_ok(false)
                    } else {
                        palette::status_fail(false)
                    },
                )),
        );
    }
    // The plan text: the port's string verbatim; an empty plan has its own state.
    match state
        .workspace
        .compare_plan(&state.compare_source, &state.compare_target)
    {
        Ok(plan) if !plan.is_empty() => {
            let mut block = col()
                .key("compare-plan")
                .w_full()
                .gap(2.0)
                .px(12.0)
                .py(8.0)
                .radius(8.0)
                .fill(theme.control_off);
            for line in plan.lines() {
                block = block.child(
                    text(line.to_owned())
                        .font(crate::sql_editor::editor_font())
                        .color(theme.text),
                );
            }
            section = section.child(block);
        }
        Ok(_) => {
            section = section.child(
                row()
                    .key("compare-plan-empty")
                    .w_full()
                    .h(40.0)
                    .px(12.0)
                    .center_y()
                    .radius(8.0)
                    .fill(theme.control_off)
                    .child(
                        text("No differences".to_owned())
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            )
        }
        Err(_) => {}
    }
    // D8: apply the plan; the progress reuses the snapshot panel.
    section = section.child(
        row()
            .key("compare-apply-controls")
            .w_full()
            .h(28.0)
            .gap(8.0)
            .center_y()
            .child(
                row()
                    .key("compare-apply")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(text("Apply plan").font(type_::LABEL).color(theme.text))
                    .on_press(|state: &mut AppState| state.apply_compare_plan()),
            )
            .child(
                row()
                    .key("compare-refresh")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(text("Refresh").font(type_::LABEL).color(theme.text))
                    .on_press(|state: &mut AppState| state.refresh_compare_progress()),
            ),
    );
    if let Some(error) = &state.compare_error {
        section = section.child(
            row()
                .key("compare-error")
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
    if let Some(snapshot) = &state.compare_snapshot {
        section = section.child(progress_panel(snapshot, "compare", theme));
    }
    section.child(list)
}
