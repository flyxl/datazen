//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;

use crate::constants::*;
use crate::icons::{self, ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT, section_icon};
use crate::state_def::{AppState, StructureState, WidgetBarsState};
use crate::types::{Section, TableView, Tool};
use crate::{splitter, sql_editor};
use datazen_ui_api::{ColumnDraft, ConnectionProfile, DashboardWidget, MemoryWorkspace, ProgressSnapshot, QueryApi, Setting, SettingsApi, TabKind, Workflow, WorkflowStepResult, WorkspaceApi};
use rune::{FileDialogOptions, TextEdit, Tx};
use rune::tok::{self, type_};
use rune::{Color, Cx, El, col, oklch, row, text, text_input};

pub(crate) fn navigator_connections_tree(state: &AppState, theme: tok::Theme) -> El {
    let filter = state.navigator_filter.text.trim().to_lowercase();
    let visible = state.visible_connections();
    let matches = |c: &ConnectionProfile| {
        filter.is_empty()
            || c.name.to_lowercase().contains(&filter)
            || c.engine.to_lowercase().contains(&filter)
            || c.id.to_lowercase().contains(&filter)
    };

    let mut recent: Vec<ConnectionProfile> = Vec::new();
    for id in &state.recent_connection_ids {
        if let Some(c) = visible.iter().find(|c| &c.id == id) {
            if matches(c) {
                recent.push(c.clone());
            }
        }
    }
    let all: Vec<ConnectionProfile> = visible.into_iter().filter(matches).collect();

    let mut tree = col().key("navigator-connections").w_full().gap(4.0);
    tree = tree.child(
        text("Connections")
            .key("navigator-connections-title")
            .font(type_::LABEL)
            .color(theme.text_secondary),
    );

    tree = tree.child(
        text("Recent")
            .key("nav-group-recent")
            .font(type_::LABEL)
            .color(theme.text_secondary),
    );
    if recent.is_empty() {
        tree = tree.child(
            row()
                .key("nav-group-recent-empty")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(
                    text(if filter.is_empty() { "No recent connections" } else { "No matches" })
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    } else {
        for c in recent {
            let id = c.id.clone();
            let label = format!("{} · {}", c.name, c.engine);
            tree = tree.child(
                row()
                    .key(format!("nav-conn-recent-{}", c.id))
                    .w_full()
                    .h(28.0)
                    .px(16.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .label(label.clone())
                    .child(text(label).font(type_::LABEL).color(theme.text))
                    .on_press(move |state: &mut AppState| state.open_connection(&id)),
            );
        }
    }

    tree = tree.child(
        text("All connections")
            .key("nav-group-all")
            .font(type_::LABEL)
            .color(theme.text_secondary),
    );
    if all.is_empty() {
        tree = tree.child(
            row()
                .key("nav-group-all-empty")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(
                    text(if filter.is_empty() { "No connections" } else { "No matches" })
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    } else {
        for c in all {
            let id = c.id.clone();
            let label = format!("{} · {}", c.name, c.engine);
            tree = tree.child(
                row()
                    .key(format!("nav-conn-{}", c.id))
                    .w_full()
                    .h(28.0)
                    .px(16.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .label(label.clone())
                    .child(text(label).font(type_::LABEL).color(theme.text))
                    .on_press(move |state: &mut AppState| state.open_connection(&id)),
            );
        }
    }

    tree
}
pub(crate) fn navigator_panel(state: &AppState, theme: tok::Theme) -> El {
    let width = state.navigator_width() as f32;
    col()
        .key("datazen-navigator")
        .w(width)
        .h_full()
        .px(12.0)
        .py(12.0)
        .gap(8.0)
        .fill(theme.bg)
        .child(
            text("Navigator")
                .key("navigator-title")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(
            text_input(&state.navigator_filter, type_::LABEL)
                .key("navigator-filter-input")
                .w_full()
                .h(28.0)
                .on_edit(|state: &mut AppState| &mut state.navigator_filter),
        )
        .child(navigator_connections_tree(state, theme))
        .child(object_tree(state, theme))
        .child(refresh_button(state, theme))
        .child(er_button(state, theme))
        .child(open_query_button(theme))
        .child(disconnect_button(state, theme))
}
/// The connection after `current` in the port's list, wrapping around.
/// A workflow as YAML text.
pub(crate) fn object_tree(state: &AppState, theme: tok::Theme) -> El {
    let catalog = state.catalog();
    let filter = state.navigator_filter.text.trim().to_lowercase();
    let mut rows: Vec<El> = Vec::new();
    for database in &catalog.databases {
        let db_match = filter.is_empty() || database.name.to_lowercase().contains(&filter);
        let mut schema_rows: Vec<El> = Vec::new();
        let mut db_has_match = db_match;
        for schema in &database.schemas {
            let schema_match = db_match || schema.name.to_lowercase().contains(&filter);
            let mut table_rows: Vec<El> = Vec::new();
            for name in &schema.tables {
                if !filter.is_empty()
                    && !db_match
                    && !schema_match
                    && !name.to_lowercase().contains(&filter)
                {
                    continue;
                }
                let title = name.clone();
                table_rows.push(
                    row()
                        .key(format!("tree-table-{name}"))
                        .w_full()
                        .h(28.0)
                        .px(32.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text(name.clone()).font(type_::LABEL).color(theme.text))
                        .on_press(move |state: &mut AppState| {
                            state.open_table_tab(&title);
                        }),
                );
            }
            if table_rows.is_empty() && !(filter.is_empty() || schema_match) {
                continue;
            }
            db_has_match = true;
            let schema_path = format!("{}/{}", database.name, schema.name);
            let schema_key = format!("tree-schema-{}-{}", database.name, schema.name);
            let collapsed = state.collapsed.contains(&schema_path) && filter.is_empty();
            schema_rows.push(
                row()
                    .key(schema_key)
                    .w_full()
                    .h(28.0)
                    .px(20.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(
                        text(format!("{} · schema", schema.name))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    )
                    .on_press(move |state: &mut AppState| state.toggle_expand(&schema_path)),
            );
            if !collapsed {
                schema_rows.extend(table_rows);
            }
        }
        if !db_has_match && !filter.is_empty() {
            continue;
        }
        let db_path = database.name.clone();
        let db_collapsed = state.collapsed.contains(&database.name) && filter.is_empty();
        rows.push(
            row()
                .key(format!("tree-db-{}", database.name))
                .w_full()
                .h(28.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!("{} · database", database.name))
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .on_press(move |state: &mut AppState| state.toggle_expand(&db_path)),
        );
        if !db_collapsed {
            rows.extend(schema_rows);
        }
    }
    if rows.is_empty() {
        let msg = if filter.is_empty() {
            "No objects"
        } else {
            "No matches"
        };
        rows.push(
            row()
                .key("object-tree-empty")
                .w_full()
                .h(28.0)
                .px(8.0)
                .center_y()
                .child(text(msg).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    let count = rows.len() as f32;
    let height = if count == 0.0 { 0.0 } else { count * 28.0 + (count - 1.0) * 4.0 };
    let mut tree = col().key("object-tree").w_full().h(height).gap(4.0);
    for row in rows {
        tree = tree.child(row);
    }
    tree
}
pub(crate) fn open_query_button(theme: tok::Theme) -> El {
    row()
        .key("open-query")
        .w_full()
        .h(32.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("New query").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| {
            state.open_query_tab("Query");
        })
}
pub(crate) fn er_button(state: &AppState, theme: tok::Theme) -> El {
    if !state.session_open() {
        return row().key("er-button-absent").h(0.0);
    }
    row()
        .key("er-button")
        .w_full()
        .h(28.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("ER diagram").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.open_er())
}
pub(crate) fn refresh_button(state: &AppState, theme: tok::Theme) -> El {
    if !state.session_open() {
        return row().key("tree-refresh-absent").h(0.0);
    }
    row()
        .key("tree-refresh")
        .w_full()
        .h(28.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("Refresh catalog").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.refresh_catalog())
}
pub(crate) fn disconnect_button(state: &AppState, theme: tok::Theme) -> El {
    if !state.session_open() {
        return row().key("disconnect-absent").h(0.0);
    }
    row()
        .key("disconnect")
        .w_full()
        .h(32.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("Disconnect").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.disconnect())
}
