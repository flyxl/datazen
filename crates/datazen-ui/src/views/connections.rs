//! The connections page's shell: the home body (`connections_home`), the
//! form dialog (A2) and the import plumbing (T7: password, file pick,
//! preview, confirm).

use super::*;

use crate::state_def::AppState;
use rune::tok::{self, type_};
use rune::{FileDialogOptions, Tx};
use rune::{col, row, text, text_input, El};

/// The connections page: the home body with the form dialog over it.
pub(crate) fn connection_list(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let mut page = connections_home(state, theme, dark);
    if state.connection_dialog_open {
        page = page.child(connection_form_dialog(state, theme));
    }
    page
}

/// T7: an Open dialog picks the export file; the host reads and parses it.
/// The password field serves the encrypted export formats.
pub(crate) fn import_press(_state: &mut AppState, tx: &mut Tx) {
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
}

/// The import bar (T7): the password for encrypted exports, then the parsed
/// preview and its confirm, then the outcome notice. Keys are the ones the
/// import flows and tests press. The bar hugs its content; callers give it
/// width (the landing's full-width column, the empty state's centered row).
pub(crate) fn import_bar(state: &AppState, theme: tok::Theme) -> El {
    let mut import = col().key("connection-import").gap(8.0).child(
        row()
            .key("connection-import-form")
            .h(28.0)
            .gap(8.0)
            .center_y()
            .child(
                text_input(&state.import_password, type_::LABEL)
                    .key("import-password-input")
                    .w(160.0)
                    .h(24.0)
                    .px(8.0)
                    .radius(6.0)
                    .fill(theme.card)
                    .border(1.0, theme.card_border)
                    .placeholder("Import password", theme.text_secondary)
                    .on_edit(|state: &mut AppState| &mut state.import_password),
            )
            .child(
                text("Password for encrypted exports")
                    .key("import-password-hint")
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
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
    import
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