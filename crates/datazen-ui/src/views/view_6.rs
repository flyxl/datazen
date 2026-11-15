//! View functions for the DataZen Rune UI.

use super::*;
use crate::constants::*;
use crate::icons::{self, ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT, section_icon};
use crate::state_def::{StructureState, WidgetBarsState};
use datazen_ui_api::{ColumnDraft, MemoryWorkspace};

use crate::state_def::AppState;
use crate::types::{Section, TableView, Tool};
use crate::constants::*;
use crate::{splitter, sql_editor};

use datazen_ui_api::{ConnectionProfile, DashboardWidget, ProgressSnapshot, QueryApi, Setting, SettingsApi, TabKind, Workflow, WorkflowStepResult, WorkspaceApi};
use rune::{FileDialogOptions, TextEdit, Tx};
use rune::tok::{self, type_};
use rune::{Color, Cx, El, col, oklch, row, text, text_input};

pub(crate) fn connection_list(state: &AppState, theme: tok::Theme) -> El {
    let mut list = col().key("connection-list").w_full().gap(12.0);

    let port_connections = state.workspace.connections();
    let connections = state
        .desktop_connections
        .as_deref()
        .unwrap_or(port_connections);

    // A1: home hero — summary aligned toward React ConnectionWorkspaceHome.
    let connection_count = connections.len();
    let mut engines: Vec<String> = connections.iter().map(|c| c.engine.clone()).collect();
    engines.sort();
    engines.dedup();
    let engine_count = engines.len();
    let hero = col()
        .key("connections-home-hero")
        .w_full()
        .gap(6.0)
        .child(
            text("Select a connection")
                .key("connections-home-title")
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            text(format!(
                "{connection_count} connections · {engine_count} engine types"
            ))
            .key("connections-home-subtitle")
            .font(type_::LABEL)
            .color(theme.text_secondary),
        );
    list = list.child(hero);

    // The desktop adapter's answer: its list once it is wired (H3), or its
    // fixed error over the memory port's demo list until then (H2).
    if let Some(message) = &state.desktop_error {
        list = list.child(
            row()
                .w_full()
                .h(28.0)
                .px(10.0)
                .center_y()
                .radius(8.0)
                .fill(theme.control_off)
                .child(
                    text(message.clone())
                        .key("desktop-adapter-message")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    // N3: a live host's open job in flight.
    if state.connect_loading.is_some() {
        list = list.child(
            row()
                .w_full()
                .h(28.0)
                .px(10.0)
                .center_y()
                .radius(8.0)
                .fill(theme.control_off)
                .child(
                    text("Connecting…")
                        .key("connect-loading")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }

    let mut cards = col()
        .key("connections-card-list")
        .w_full()
        .gap(6.0)
        .child(
            text("Your connections")
                .key("connections-section-title")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );
    for profile in connections {
        let id = profile.id.clone();
        cards = cards.child(
            row()
                .key(format!("connection-{id}"))
                .w_full()
                .h(44.0)
                .px(12.0)
                .center_y()
                .radius(8.0)
                .fill(theme.control_off)
                .gap(10.0)
                .child(
                    col()
                        .grow(1.0)
                        .gap(2.0)
                        .child(text(profile.name.clone()).font(type_::LABEL).color(theme.text))
                        .child(
                            text(profile.engine.clone())
                                .font(type_::LABEL)
                                .color(theme.text_secondary),
                        ),
                )
                .child(
                    row()
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.bg)
                        .child(text("Connect").font(type_::LABEL).color(theme.text)),
                )
                .on_press(move |state: &mut AppState| state.open_connection(&id)),
        );
    }
    if connection_count == 0 {
        cards = cards.child(
            row()
                .key("connections-empty")
                .w_full()
                .h(40.0)
                .px(12.0)
                .center_y()
                .radius(8.0)
                .fill(theme.control_off)
                .child(
                    text("No connections yet")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    list = list.child(cards);

    // A1: recent queries section (structural; history source may be unavailable).
    list = list.child(
        col()
            .key("connections-recent-queries")
            .w_full()
            .gap(6.0)
            .child(
                text("Recent queries")
                    .key("connections-recent-title")
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            )
            .child(
                row()
                    .key("connections-recent-empty")
                    .w_full()
                    .h(36.0)
                    .px(12.0)
                    .center_y()
                    .radius(8.0)
                    .fill(theme.control_off)
                    .child(
                        text("No recent queries")
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            ),
    );

    let mut form = col().key("connection-form").w_full().gap(8.0).child(
        row()
            .key("connection-form-fields")
            .w_full()
            .h(28.0)
            .gap(8.0)
            .center_y()
            .child(
                text_input(&state.connection_name, type_::LABEL)
                    .key("connection-name-input")
                    .w(150.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_name),
            )
            .child(
                text_input(&state.connection_engine, type_::LABEL)
                    .key("connection-engine-input")
                    .w(110.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_engine),
            )
            .child(
                text_input(&state.connection_address, type_::LABEL)
                    .key("connection-address-input")
                    .w(170.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_address),
            )
            .child(
                text_input(&state.connection_username, type_::LABEL)
                    .key("connection-username-input")
                    .w(110.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_username),
            )
            .child(
                text_input(&state.connection_password, type_::LABEL)
                    .key("connection-password-input")
                    .w(110.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_password),
            )
            .child(
                text_input(&state.connection_database, type_::LABEL)
                    .key("connection-database-input")
                    .w(110.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_database),
            )
            .child(
                row()
                    .key("connection-save")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(text("Save connection").font(type_::LABEL).color(theme.text))
                    .on_press(|state: &mut AppState| state.save_connection_form()),
            ),
    );
    if let Some(error) = &state.connection_error {
        form = form.child(
            row()
                .key("connection-form-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    let mut import = col().key("connection-import").w_full().gap(8.0).child(
        row()
            .key("connection-import-form")
            .w_full()
            .h(28.0)
            .gap(8.0)
            .center_y()
            .child(
                text_input(&state.import_password, type_::LABEL)
                    .key("import-password-input")
                    .w(160.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.import_password),
            )
            .child(
                row()
                    .key("import-connections")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(
                        text("Import connections…".to_owned())
                            .font(type_::LABEL)
                            .color(theme.text),
                    )
                    .on_press_tx(|_state: &mut AppState, tx: &mut Tx| {
                        // T7: an Open dialog picks the export file; the host
                        // reads and parses it.
                        let options = FileDialogOptions {
                            title: "Choose a connection export".into(),
                            ..Default::default()
                        };
                        tx.open_file(options, |state: &mut AppState, result, _| match result {
                            Ok(Some(path)) => {
                                state.begin_connections_import(&path.display().to_string());
                            }
                            Ok(None) => {}
                            Err(error) => {
                                state.import_preview = None;
                                state.import_notice = Some(format!(
                                    "the file picker is unavailable ({error}); the import needs a file"
                                ));
                            }
                        });
                    }),
            ),
    );
    if let Some(preview) = &state.import_preview {
        let mut panel = col()
            .key("import-preview")
            .w_full()
            .gap(4.0)
            .child(
                text(format!(
                    "{} · {} connections",
                    preview.format,
                    preview.connections.len()
                ))
                .key("import-preview-title")
                .font(type_::LABEL)
                .color(theme.text),
            );
        for (index, connection) in preview.connections.iter().enumerate() {
            panel = panel.child(
                row()
                    .key(format!("imported-connection-{index}"))
                    .w_full()
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(
                        text(format!(
                            "{} · {} · {}",
                            connection.name, connection.engine, connection.address
                        ))
                        .font(type_::LABEL)
                        .color(theme.text),
                    ),
            );
        }
        panel = panel.child(
            row()
                .key("import-confirm")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!(
                        "Import {} connections",
                        preview.connections.len()
                    ))
                    .font(type_::LABEL)
                    .color(theme.text),
                )
                .on_press(|state: &mut AppState| state.confirm_connections_import()),
        );
        import = import.child(panel);
    }
    if let Some(notice) = &state.import_notice {
        import = import.child(
            row()
                .key("import-notice")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(notice.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    list.child(form).child(import)
}

/// L6: 40pt content status bar — replaces the migration preview banner.
/// Shows connection state, context counts (table/columns/rows) when available,
/// and shortcut hints.
pub(crate) fn content_status_bar(state: &AppState, theme: tok::Theme) -> El {
    let connected = state.session_open();
    let dot = if connected {
        tok::accent(2, true)
    } else {
        theme.text_secondary
    };

    // Resolve connection label when a desktop session is known.
    let connection_label = state.desktop_connection.as_ref().and_then(|id| {
        state
            .desktop_connections
            .as_ref()
            .and_then(|list| list.iter().find(|c| &c.id == id))
            .map(|c| c.name.clone())
    });

    // Center context: database/table/columns/rows derived from current state.
    let mut center_parts: Vec<String> = Vec::new();
    if let Some(name) = connection_label {
        if !name.is_empty() {
            center_parts.push(name);
        }
    }
    if let Some(tab) = state.tabs.selected() {
        if tab.kind == datazen_ui_api::TabKind::Table {
            if !tab.title.is_empty() {
                center_parts.push(tab.title.clone());
            }
            if state.grid.has_page() {
                if state.grid.cols > 0 {
                    center_parts.push(format!("{} cols", state.grid.cols));
                }
                if state.grid.total_rows() > 0 {
                    center_parts.push(format!("{} rows", state.grid.total_rows()));
                }
            }
        } else if tab.kind == datazen_ui_api::TabKind::Query {
            center_parts.push("Query".to_owned());
            if let Some(result) = &state.result_grid {
                if result.has_page() {
                    if result.cols > 0 {
                        center_parts.push(format!("{} cols", result.cols));
                    }
                    if result.total_rows() > 0 {
                        center_parts.push(format!("{} rows", result.total_rows()));
                    }
                }
            }
        }
    }
    let center_text = if center_parts.is_empty() {
        state.active.label().to_owned()
    } else {
        center_parts.join(" · ")
    };

    row()
        .key("content-status-bar")
        .w_full()
        .h(40.0)
        .px(12.0)
        .center_y()
        .fill(theme.control_off)
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(
                    col()
                        .key("status-connection-dot")
                        .w(8.0)
                        .h(8.0)
                        .radius(4.0)
                        .fill(dot),
                )
                .child(
                    text(if connected { "Connected" } else { "Disconnected" })
                        .key("status-connection-label")
                        .font(type_::LABEL)
                        .color(theme.text),
                ),
        )
        .child(
            row()
                .grow(1.0)
                .center_y()
                .justify_center()
                .child(
                    text(center_text)
                        .key("status-context")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(
                    text("⌘N New query · ⌘W Close")
                        .key("status-shortcuts")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
}
