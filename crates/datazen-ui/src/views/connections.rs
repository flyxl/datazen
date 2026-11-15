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
    let hero = row()
        .key("connections-home-hero")
        .w_full()
        .gap(8.0)
        .center_y()
        .child(
            col()
                .grow(1.0)
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
                ),
        )
        .child(
            row()
                .key("connection-new")
                .h(28.0)
                .px(10.0)
                .center_y()
                .radius(8.0)
                .fill(theme.control_off)
                .child(text("New connection").font(type_::LABEL).color(theme.text))
                .on_press(|state: &mut AppState| state.open_connection_dialog()),
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

    if state.connection_dialog_open {
        list = list.child(connection_form_dialog(state, theme));
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
    list.child(import)
}

fn connection_form_dialog(state: &AppState, theme: tok::Theme) -> El {
    let mut dialog = col()
        .key("connection-dialog")
        .w_full()
        .gap(10.0)
        .px(12.0)
        .py(10.0)
        .radius(10.0)
        .fill(theme.control_off)
        .child(
            row()
                .key("connection-dialog-header")
                .w_full()
                .h(28.0)
                .center_y()
                .gap(8.0)
                .child(
                    text("New connection")
                        .key("connection-dialog-title")
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .child(row().grow(1.0))
                .child(
                    row()
                        .key("connection-dialog-close")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.bg)
                        .child(text("Close").font(type_::LABEL).color(theme.text_secondary))
                        .on_press(|state: &mut AppState| state.close_connection_dialog()),
                ),
        );

    let basic = col()
        .key("connection-section-basic")
        .w_full()
        .gap(6.0)
        .child(
            text("Basic")
                .key("connection-section-basic-title")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(
            row()
                .key("connection-basic-fields")
                .w_full()
                .gap(8.0)
                .child(
                    col()
                        .w(180.0)
                        .gap(2.0)
                        .child(text("Name").font(type_::LABEL).color(theme.text_secondary))
                        .child(
                            text_input(&state.connection_name, type_::LABEL)
                                .key("connection-name-input")
                                .w(180.0)
                                .h(24.0)
                                .on_edit(|state: &mut AppState| &mut state.connection_name),
                        ),
                )
                .child(
                    col()
                        .w(140.0)
                        .gap(2.0)
                        .child(text("Engine").font(type_::LABEL).color(theme.text_secondary))
                        .child(
                            text_input(&state.connection_engine, type_::LABEL)
                                .key("connection-engine-input")
                                .w(140.0)
                                .h(24.0)
                                .on_edit(|state: &mut AppState| &mut state.connection_engine),
                        ),
                ),
        );

    let connection = col()
        .key("connection-section-connection")
        .w_full()
        .gap(6.0)
        .child(
            text("Connection")
                .key("connection-section-connection-title")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(
            row()
                .key("connection-connection-fields")
                .w_full()
                .gap(8.0)
                .child(
                    col()
                        .w(200.0)
                        .gap(2.0)
                        .child(text("Address").font(type_::LABEL).color(theme.text_secondary))
                        .child(
                            text_input(&state.connection_address, type_::LABEL)
                                .key("connection-address-input")
                                .w(200.0)
                                .h(24.0)
                                .on_edit(|state: &mut AppState| &mut state.connection_address),
                        ),
                )
                .child(
                    col()
                        .w(140.0)
                        .gap(2.0)
                        .child(text("Database").font(type_::LABEL).color(theme.text_secondary))
                        .child(
                            text_input(&state.connection_database, type_::LABEL)
                                .key("connection-database-input")
                                .w(140.0)
                                .h(24.0)
                                .on_edit(|state: &mut AppState| &mut state.connection_database),
                        ),
                )
                .child(
                    col()
                        .w(140.0)
                        .gap(2.0)
                        .child(text("Username").font(type_::LABEL).color(theme.text_secondary))
                        .child(
                            text_input(&state.connection_username, type_::LABEL)
                                .key("connection-username-input")
                                .w(140.0)
                                .h(24.0)
                                .on_edit(|state: &mut AppState| &mut state.connection_username),
                        ),
                )
                .child(
                    col()
                        .w(140.0)
                        .gap(2.0)
                        .child(text("Password").font(type_::LABEL).color(theme.text_secondary))
                        .child(
                            text_input(&state.connection_password, type_::LABEL)
                                .key("connection-password-input")
                                .w(140.0)
                                .h(24.0)
                                .on_edit(|state: &mut AppState| &mut state.connection_password),
                        ),
                ),
        );

    let ssh_label = if state.connection_ssh_enabled {
        "SSH tunnel: enabled (UI only, not saved)"
    } else {
        "SSH tunnel (UI only, not saved)"
    };
    let ssh = col()
        .key("connection-section-ssh")
        .w_full()
        .gap(6.0)
        .child(
            text(ssh_label)
                .key("connection-section-ssh-title")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(
            row()
                .key("connection-ssh-toggle-row")
                .w_full()
                .h(24.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("connection-ssh-toggle")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.bg)
                        .child(text(if state.connection_ssh_enabled { "Disable SSH tunnel" } else { "Enable SSH tunnel" }).font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| {
                            state.connection_ssh_enabled = !state.connection_ssh_enabled;
                        }),
                ),
        )
        .child(
            row()
                .key("connection-ssh-fields")
                .w_full()
                .gap(8.0)
                .child(
                    col()
                        .w(180.0)
                        .gap(2.0)
                        .child(text("SSH host").font(type_::LABEL).color(theme.text_secondary))
                        .child(
                            text_input(&state.connection_ssh_host, type_::LABEL)
                                .key("connection-ssh-host-input")
                                .w(180.0)
                                .h(24.0)
                                .on_edit(|state: &mut AppState| &mut state.connection_ssh_host),
                        ),
                )
                .child(
                    col()
                        .w(90.0)
                        .gap(2.0)
                        .child(text("SSH port").font(type_::LABEL).color(theme.text_secondary))
                        .child(
                            text_input(&state.connection_ssh_port, type_::LABEL)
                                .key("connection-ssh-port-input")
                                .w(90.0)
                                .h(24.0)
                                .on_edit(|state: &mut AppState| &mut state.connection_ssh_port),
                        ),
                )
                .child(
                    col()
                        .w(160.0)
                        .gap(2.0)
                        .child(text("SSH username").font(type_::LABEL).color(theme.text_secondary))
                        .child(
                            text_input(&state.connection_ssh_username, type_::LABEL)
                                .key("connection-ssh-username-input")
                                .w(160.0)
                                .h(24.0)
                                .on_edit(|state: &mut AppState| &mut state.connection_ssh_username),
                        ),
                ),
        );

    let form_fields = row()
        .key("connection-form-fields")
        .w_full()
        .gap(8.0)
        .child(col().w_full().gap(10.0).child(basic).child(connection).child(ssh));

    let mut form = col().key("connection-form").w_full().gap(10.0).child(form_fields);
    let error_text = state.connection_error.clone().unwrap_or_default();
    form = form.child(
        row()
            .key("connection-form-error")
            .w_full()
            .h(if state.connection_error.is_some() { 24.0 } else { 0.0 })
            .px(8.0)
            .center_y()
            .radius(6.0)
            .fill(theme.bg)
            .child(text(error_text).font(type_::LABEL).color(theme.text_secondary)),
    );
    form = form.child(
        row()
            .key("connection-form-actions")
            .w_full()
            .h(28.0)
            .gap(8.0)
            .center_y()
            .child(
                row()
                    .key("connection-save")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.bg)
                    .child(text("Save connection").font(type_::LABEL).color(theme.text))
                    .on_press(|state: &mut AppState| state.save_connection_form()),
            )
            .child(
                row()
                    .key("connection-cancel")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.bg)
                    .child(text("Cancel").font(type_::LABEL).color(theme.text_secondary))
                    .on_press(|state: &mut AppState| state.close_connection_dialog()),
            ),
    );

    dialog.child(form)
}
