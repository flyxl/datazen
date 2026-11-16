//! The connections page's shell: the home body (`connections_home`), the
//! form dialog (A2) and the import plumbing (T7: password, file pick,
//! preview, confirm).

use super::*;

use crate::state_def::AppState;
use rune::tok::{self, type_};
use rune::{col, oklcha, row, text, text_input, El};
use rune::{FileDialogOptions, Tx};

/// The connections page: the home body. The connection form and the import
/// dialog float over it from the root modal layer (`views/modals.rs`).
pub(crate) fn connection_list(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    connections_home(state, theme, dark)
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

fn short_file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

/// Import dialog body (React-aligned layout) with preserved keys:
/// `import-dialog-fields`, `import-choose-file`, `import-password-input`,
/// `import-preview`/`import-notice` and `connection-import` content.
pub(crate) fn import_dialog_body(state: &AppState, theme: tok::Theme) -> El {
    let mut body = col().key("import-dialog-fields").w_full().gap(14.0);

    body = body.child(
        text("支持导入 DataZen / JSON / DBX / YAML / Navicat XML 等连接文件。选择文件后可输入密码并确认导入。")
            .key("import-formats-hint")
            .w_full()
            .font(type_::LABEL)
            .color(theme.text_secondary)
            .wrap(),
    );

    // File picker row (React: dashed choose or filename + replace).
    if let Some(file) = &state.import_file {
        body = body.child(
            col()
                .w_full()
                .gap(6.0)
                .child(text("导入文件").font(type_::LABEL).color(theme.text_secondary))
                .child(
                    row()
                        .key("import-selected-file-row")
                        .w_full()
                        .gap(8.0)
                        .center_y()
                        .child(
                            row()
                                .grow(1.0)
                                .min_w(0.0)
                                .h(36.0)
                                .px(12.0)
                                .center_y()
                                .radius(8.0)
                                .border(1.0, theme.card_border)
                                .fill(theme.control_off)
                                .child(
                                    text(short_file_name(file))
                                        .key("import-selected-file")
                                        .font(type_::LABEL)
                                        .color(theme.text)
                                        .grow(1.0)
                                        .min_w(0.0)
                                        .truncate(),
                                ),
                        )
                        .child(
                            row()
                                .key("import-choose-file")
                                .h(36.0)
                                .px(12.0)
                                .center_y()
                                .radius(8.0)
                                .border(1.0, theme.card_border)
                                .hover(theme.hover)
                                .label("更换文件")
                                .child(text("更换文件").font(type_::LABEL).color(theme.text))
                                .on_press_tx(import_press),
                        ),
                ),
        );
    } else {
        body = body.child(
            row()
                .key("import-choose-file")
                .w_full()
                .h(44.0)
                .center()
                .radius(8.0)
                .border(1.0, theme.card_border)
                .fill(theme.card)
                .hover(theme.hover)
                .label("选择文件")
                .child(text("选择导入文件…").font(type_::LABEL).color(theme.text_secondary))
                .on_press_tx(import_press),
        );
    }

    // Password + preview/notice (reuses preserved keys and import flow).
    body = body.child(import_bar(state, theme));
    body
}

/// The import bar (T7): password, parsed preview and outcome notice.
/// Keys preserved for import flows/tests (`import-password-input`,
/// `connection-import`, `import-preview`, `import-confirm`, `import-notice`).
pub(crate) fn import_bar(state: &AppState, theme: tok::Theme) -> El {
    let mut import = col().key("connection-import").w_full().gap(10.0).child(
        col()
            .key("connection-import-form")
            .w_full()
            .gap(6.0)
            .child(text("导入密码（可选）").font(type_::LABEL).color(theme.text_secondary))
            .child(
                text_input(&state.import_password, type_::LABEL)
                    .key("import-password-input")
                    .w_full()
                    .h(36.0)
                    .px(12.0)
                    .single_line()
                    .center_y()
                    .radius(8.0)
                    .fill(theme.card)
                    .border(1.0, theme.card_border)
                    .placeholder("加密导出文件请输入密码", theme.text_secondary)
                    .secure()
                    .on_edit(|state: &mut AppState| &mut state.import_password),
            )
            .child(
                text("仅加密导出需要密码；普通文件可留空。")
                    .key("import-password-hint")
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            ),
    );
    if let Some(preview) = &state.import_preview {
        let mut panel = col().key("import-preview").w_full().gap(6.0).child(
            text(format!(
                "{} · {} 个连接",
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
                    .min_h(28.0)
                    .px(10.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(
                        text(format!(
                            "{} · {} · {}",
                            connection.name, connection.engine, connection.address
                        ))
                        .font(type_::LABEL)
                        .color(theme.text)
                        .truncate(),
                    ),
            );
        }
        import = import.child(panel);
    }
    if let Some(notice) = &state.import_notice {
        let is_error = notice.contains("失败")
            || notice.contains("unavailable")
            || notice.to_lowercase().contains("error");
        import = import.child(
            row()
                .key("import-notice")
                .w_full()
                .px(10.0)
                .py(8.0)
                .radius(8.0)
                .fill(if is_error { theme.control_off } else { theme.bg })
                .child(
                    text(notice.clone())
                        .font(type_::LABEL)
                        .color(if is_error { theme.text } else { theme.text_secondary })
                        .wrap(),
                ),
        );
    }
    import
}

fn field_label(label: &str, theme: tok::Theme) -> El {
    text(label.to_owned())
        .font(type_::LABEL)
        .color(theme.text_secondary)
}

fn form_input(
    key: &str,
    value: &rune::TextEdit,
    theme: tok::Theme,
    edit: fn(&mut AppState) -> &mut rune::TextEdit,
) -> El {
    text_input(value, type_::LABEL).single_line()
        .key(key.to_owned())
        .w_full()
        .h(36.0)
        .px(12.0)
        .center_y()
        .radius(8.0)
        .fill(theme.card)
        .border(1.0, theme.card_border)
        .on_edit(edit)
}

fn password_input(
    key: &str,
    value: &rune::TextEdit,
    theme: tok::Theme,
    edit: fn(&mut AppState) -> &mut rune::TextEdit,
) -> El {
    form_input(key, value, theme, edit).secure()
}

fn section_toggle(key: &str, title: &str, subtitle: Option<&str>, theme: tok::Theme) -> El {
    let mut left = col().grow(1.0).min_w(0.0).gap(2.0).child(
        text(title.to_owned())
            .font(type_::LABEL)
            .color(theme.text_secondary),
    );
    if let Some(sub) = subtitle {
        left = left.child(text(sub.to_owned()).font(type_::LABEL).color(theme.text_secondary));
    }
    row()
        .key(key.to_owned())
        .w_full()
        .h(40.0)
        .px(12.0)
        .center_y()
        .gap(8.0)
        .radius(8.0)
        .border(1.0, theme.card_border)
        .fill(theme.card)
        .child(rune::icon(&crate::icons::ICON_CHEVRON_DOWN, 14.0).color(theme.text_secondary))
        .child(left)
}

/// Connection form body (React-aligned grid/sections) with preserved keys for
/// tests and flows.
pub(crate) fn connection_form_fields(state: &AppState, theme: tok::Theme) -> El {
    // Basic: name + engine (engine also selectable from sidebar).
    let basic = col()
        .key("connection-section-basic")
        .w_full()
        .gap(12.0)
        .child(
            row()
                .key("connection-basic-fields")
                .w_full()
                .gap(16.0)
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(6.0)
                        .child(field_label("连接名称 *", theme))
                        .child(form_input(
                            "connection-name-input",
                            &state.connection_name,
                            theme,
                            |state: &mut AppState| &mut state.connection_name,
                        )),
                )
                .child(
                    col()
                        .w(220.0)
                        .gap(6.0)
                        .child(field_label("数据库类型", theme))
                        .child(form_input(
                            "connection-engine-input",
                            &state.connection_engine,
                            theme,
                            |state: &mut AppState| &mut state.connection_engine,
                        )),
                ),
        );

    // Connection fields in 2-column grid (React StandardConnectionFields order/layout).
    let row_host_port = row()
        .key("connection-connection-fields")
        .w_full()
        .gap(16.0)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(6.0)
                .child(field_label("主机 / 地址 *", theme))
                .child(form_input(
                    "connection-address-input",
                    &state.connection_address,
                    theme,
                    |state: &mut AppState| &mut state.connection_address,
                )),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(6.0)
                .child(field_label("数据库", theme))
                .child(form_input(
                    "connection-database-input",
                    &state.connection_database,
                    theme,
                    |state: &mut AppState| &mut state.connection_database,
                )),
        );

    let row_user_pass = row()
        .w_full()
        .gap(16.0)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(6.0)
                .child(field_label("用户名", theme))
                .child(form_input(
                    "connection-username-input",
                    &state.connection_username,
                    theme,
                    |state: &mut AppState| &mut state.connection_username,
                )),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(6.0)
                .child(field_label("密码", theme))
                .child(password_input(
                    "connection-password-input",
                    &state.connection_password,
                    theme,
                    |state: &mut AppState| &mut state.connection_password,
                )),
        );

    let connection = col()
        .key("connection-section-connection")
        .w_full()
        .gap(12.0)
        .child(row_host_port)
        .child(row_user_pass);

    // Advanced (React-like toggle header; content minimal as state supports).
    let advanced = col()
        .w_full()
        .gap(10.0)
        .child(section_toggle("connection-advanced-toggle", "高级设置", None, theme));

    // Tunnel/SSH section (React tunnel toggle + panel style), preserving SSH keys.
    let ssh_label = if state.connection_ssh_enabled {
        "SSH 隧道（UI，仅本次编辑）"
    } else {
        "隧道"
    };
    let ssh = col()
        .key("connection-section-ssh")
        .w_full()
        .gap(10.0)
        .child(
            row()
                .w_full()
                .h(40.0)
                .px(12.0)
                .center_y()
                .gap(8.0)
                .radius(8.0)
                .border(1.0, theme.card_border)
                .fill(theme.card)
                .child(rune::icon(&crate::icons::ICON_CHEVRON_DOWN, 14.0).color(theme.text_secondary))
                .child(
                    text(ssh_label)
                        .key("connection-section-ssh-title")
                        .font(type_::LABEL)
                        .color(theme.text_secondary)
                        .grow(1.0),
                )
                .child(
                    row()
                        .key("connection-ssh-toggle")
                        .h(28.0)
                        .px(10.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .hover(theme.hover)
                        .child(
                            text(if state.connection_ssh_enabled {
                                "已启用 SSH"
                            } else {
                                "启用 SSH"
                            })
                            .font(type_::LABEL)
                            .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| {
                            state.connection_ssh_enabled = !state.connection_ssh_enabled;
                        }),
                ),
        )
        .child(
            col()
                .key("connection-ssh-fields")
                .w_full()
                .gap(12.0)
                .px(12.0)
                .py(12.0)
                .radius(8.0)
                .border(1.0, theme.card_border)
                .fill(theme.bg)
                .child(
                    row()
                        .w_full()
                        .gap(16.0)
                        .child(
                            col()
                                .grow(1.0)
                                .min_w(0.0)
                                .gap(6.0)
                                .child(field_label("SSH 主机", theme))
                                .child(form_input(
                                    "connection-ssh-host-input",
                                    &state.connection_ssh_host,
                                    theme,
                                    |state: &mut AppState| &mut state.connection_ssh_host,
                                )),
                        )
                        .child(
                            col()
                                .w(120.0)
                                .gap(6.0)
                                .child(field_label("SSH 端口", theme))
                                .child(form_input(
                                    "connection-ssh-port-input",
                                    &state.connection_ssh_port,
                                    theme,
                                    |state: &mut AppState| &mut state.connection_ssh_port,
                                )),
                        )
                        .child(
                            col()
                                .grow(1.0)
                                .min_w(0.0)
                                .gap(6.0)
                                .child(field_label("SSH 用户名", theme))
                                .child(form_input(
                                    "connection-ssh-username-input",
                                    &state.connection_ssh_username,
                                    theme,
                                    |state: &mut AppState| &mut state.connection_ssh_username,
                                )),
                        ),
                ),
        );

    let error_text = state.connection_error.clone().unwrap_or_default();
    let error_row = row()
        .key("connection-form-error")
        .w_full()
        .min_h(if state.connection_error.is_some() { 20.0 } else { 0.0 })
        .child(
            text(error_text)
                .font(type_::LABEL)
                .color(oklcha(0.63, 0.22, 25.0, 1.0))
                .wrap(),
        );

    col()
        .key("connection-form")
        .w_full()
        .gap(16.0)
        .child(col().key("connection-form-fields").w_full().gap(16.0).child(basic).child(connection).child(advanced).child(ssh))
        .child(error_row)
}
