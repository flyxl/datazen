//! The real modal dialog layer (the React `Dialog`): a dimming scrim over
//! the whole window with the dialog panel centered on it. While a layer is
//! mounted the rail, the navigator and the page stay inert and Escape
//! dismisses (`El::modal`); the panel keys are the ones the import flows and
//! the connection-form tests press.

use super::*;

use crate::state_def::AppState;
use rune::tok::{self, type_};
use rune::{col, oklcha, row, text, Color, El, TextStyle};

/// The floating layer for whatever dialog is open — the import dialog first
/// (the React share dialog opens at most one), else the connection form.
/// `None` mounts nothing and the window keeps its plain layout.
pub(crate) fn modal_layer(state: &AppState, theme: tok::Theme) -> Option<El> {
    if state.import_dialog_open {
        Some(import_dialog_layer(state, theme))
    } else if state.connection_dialog_open {
        Some(connection_dialog_layer(state, theme))
    } else {
        None
    }
}

/// The scrim + centered panel both dialogs ride on: full-window, dimmed, and
/// modal — presses outside stay inert, Escape runs the dismissal.
fn dialog_layer(
    key: &str,
    panel: El,
    scrim: Color,
    dismiss: impl Fn(&mut AppState) + 'static,
) -> El {
    col()
        .key(key.to_owned())
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .fill(scrim)
        .modal(false, dismiss)
        .child(panel)
}

/// A dialog panel: the React `Dialog`'s rounded bordered card with a header
/// (title + X close), a body and an optional footer row.
fn panel(
    key: &str,
    title: &str,
    title_key: &str,
    width: f32,
    theme: tok::Theme,
    close: El,
    body: El,
    footer: Option<El>,
) -> El {
    let mut card = col()
        .key(key.to_owned())
        .w(width)
        .radius(12.0)
        .fill(theme.card)
        .border(1.0, theme.card_border)
        .child(
            row()
                .key(format!("{key}-header"))
                .w_full()
                .h(48.0)
                .px(16.0)
                .gap(8.0)
                .center_y()
                .child(
                    text(title.to_owned())
                        .key(title_key.to_owned())
                        .font(TextStyle::new(14.0, 600.0))
                        .color(theme.text)
                        .grow(1.0),
                )
                .child(close),
        )
        .child(
            col()
                .key(format!("{key}-body"))
                .w_full()
                .min_h(0.0)
                .px(16.0)
                .py(12.0)
                .gap(10.0)
                .child(body),
        );
    if let Some(footer) = footer {
        card = card.child(footer);
    }
    card
}

/// The close press both dialogs share (the React header X).
fn close_button(key: &str, theme: tok::Theme, close: impl Fn(&mut AppState) + 'static) -> El {
    row()
        .key(key.to_owned())
        .size(24.0, 24.0)
        .center()
        .radius(6.0)
        .label("Close")
        .hover(theme.hover)
        .child(rune::icon(&crate::icons::ICON_X, 14.0).color(theme.text_secondary))
        .on_press(move |state: &mut AppState| close(state))
}

/// The import dialog (React `ConnectionShareDialog` import mode): the
/// formats hint, the picked file, the password for the encrypted formats,
/// then the parsed preview and its confirm, then the outcome notice. The
/// file pick is the dialog's own button (`import-choose-file`), so the
/// journey is: open dialog → choose file → preview → import.
fn import_dialog_layer(state: &AppState, theme: tok::Theme) -> El {
    let mut body = col().key("import-dialog-fields").w_full().gap(10.0);
    body = body.child(
        text("Supports DataZen (.datazenconnection), DBX, DataGrip (dataSources.xml), Navicat (.ncx), DBeaver (data-sources.json), and TablePlus (.tableplusconnection).")
            .key("import-formats-hint")
            .w_full()
            .font(type_::LABEL)
            .color(theme.text_secondary)
            .wrap(),
    );
    if let Some(file) = &state.import_file {
        body = body.child(
            row()
                .key("import-selected-file-row")
                .w_full()
                .min_h(24.0)
                .gap(8.0)
                .center_y()
                .child(
                    text("Selected file")
                        .key("import-selected-file-label")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    text(short_file_name(file))
                        .key("import-selected-file")
                        .font(type_::LABEL)
                        .color(theme.text)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    row()
                        .key("import-change-file")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .label("Change file")
                        .child(text("Change file").font(type_::LABEL).color(theme.text))
                        .on_press_tx(import_press),
                ),
        );
    } else {
        body = body.child(
            row()
                .key("import-choose-file")
                .h(28.0)
                .px(10.0)
                .center_y()
                .radius(7.0)
                .fill(theme.control_off)
                .label("Choose file")
                .child(text("Choose file…").font(type_::LABEL).color(theme.text))
                .on_press_tx(import_press),
        );
    }
    // The password, the parsed preview, its confirm and the outcome notice
    // (the keys the import flows and tests press) ride the dialog body.
    body = body.child(import_bar(state, theme));

    let card = panel(
        "import-dialog",
        "Import connections",
        "import-dialog-title",
        460.0,
        theme,
        close_button("import-dialog-close", theme, |state: &mut AppState| {
            state.close_import_dialog()
        }),
        body,
        Some(dialog_footer(theme, |state: &mut AppState| {
            state.close_import_dialog()
        })),
    );
    dialog_layer(
        "import-dialog-layer",
        card,
        scrim(),
        |state: &mut AppState| state.close_import_dialog(),
    )
}

/// The connection form over the React `Dialog` chrome: the same grouped
/// fields, error row and actions the inline form had, floating now. The
/// form's own actions row is the footer, so no extra one is added.
fn connection_dialog_layer(state: &AppState, theme: tok::Theme) -> El {
    let card = panel(
        "connection-dialog",
        "New connection",
        "connection-dialog-title",
        700.0,
        theme,
        close_button("connection-dialog-close", theme, |state: &mut AppState| {
            state.close_connection_dialog()
        }),
        connection_form_fields(state, theme),
        None,
    );
    dialog_layer(
        "connection-dialog-layer",
        card,
        scrim(),
        |state: &mut AppState| state.close_connection_dialog(),
    )
}

/// The footer the import dialog shares with every future dialog: the Cancel
/// press at the trailing edge (the React dialog's secondary button).
fn dialog_footer(theme: tok::Theme, close: impl Fn(&mut AppState) + Copy + 'static) -> El {
    row()
        .key("dialog-footer")
        .w_full()
        .h(44.0)
        .px(16.0)
        .gap(8.0)
        .center_y()
        .fill(theme.bg)
        .child(col().grow(1.0))
        .child(
            row()
                .key("dialog-cancel")
                .h(28.0)
                .px(10.0)
                .center_y()
                .radius(7.0)
                .fill(theme.control_off)
                .label("Cancel")
                .child(text("Cancel").font(type_::LABEL).color(theme.text))
                .on_press(move |state: &mut AppState| close(state)),
        )
}

/// The React scrim (`bg-black/50`) in both appearances.
fn scrim() -> Color {
    oklcha(0.0, 0.0, 256.0, 0.5)
}

/// The file's own name, for the dialog's picked-file row.
fn short_file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}
