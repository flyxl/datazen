//! The modal dialog layer (React-aligned): a dimming scrim over the window
//! with the dialog panel centered on it. While mounted the background stays
//! inert and Escape dismisses (`El::modal`). Required panel/field keys are
//! preserved for existing flows and tests.

use super::*;

use crate::state_def::AppState;
use crate::ui::kit::kit;
use rune::tok::{self, type_};
use rune::{col, oklch, oklcha, row, text, Color, El, ScrollSpec, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize};

/// The floating layer for whatever dialog is open — the import dialog first
/// (the React share dialog opens at most one), else the connection form.
/// `None` mounts nothing and the window keeps its plain layout.
pub(crate) fn modal_layer(state: &AppState, theme: tok::Theme, dark: bool) -> Option<El> {
    if state.import_dialog_open {
        Some(import_dialog_layer(state, theme, dark))
    } else if state.connection_dialog_open {
        Some(connection_dialog_layer(state, theme, dark))
    } else {
        None
    }
}

/// The scrim + centered panel: full-window, dimmed, and modal.
fn dialog_layer(
    key: &str,
    panel: El,
    dismiss: impl Fn(&mut AppState) + 'static,
) -> El {
    col()
        .key(key.to_owned())
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .fill(scrim())
        .modal(false, dismiss)
        .child(panel)
}

fn scrim() -> Color {
    oklcha(0.0, 0.0, 256.0, 0.5)
}

/// Stable accent hue per engine id (aligned with connections home badges).
fn engine_hue(engine: &str) -> f32 {
    match engine.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" | "pg" => 256.0,
        "sqlite" | "sqlite3" => 152.0,
        "mysql" | "mariadb" => 58.0,
        "mongodb" | "mongo" => 152.0,
        "redis" => 25.0,
        "sqlserver" | "mssql" => 220.0,
        "clickhouse" => 300.0,
        "duckdb" => 80.0,
        _ => {
            let mut hash: u64 = 0xcbf29ce484222325;
            for byte in engine.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
            (hash % 360) as f32
        }
    }
}

/// Engine badge icon (colored rounded badge + monogram) for driver list.
fn engine_badge_icon(engine: &str, size: f32, dark: bool) -> El {
    let hue = engine_hue(engine);
    let fill = oklch(if dark { 0.5 } else { 0.58 }, 0.12, hue);
    let initial = engine
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    row()
        .size(size, size)
        .radius(size * 0.28)
        .center()
        .fill(fill)
        .child(
            text(initial)
                .font(TextStyle::new(size * 0.46, 700.0))
                .color(oklch(0.99, 0.005, 256.0)),
        )
}

fn close_button(key: &str, theme: tok::Theme, close: impl Fn(&mut AppState) + 'static) -> El {
    row()
        .key(key.to_owned())
        .size(28.0, 28.0)
        .center()
        .radius(6.0)
        .label("Close")
        .hover(theme.hover)
        .child(rune::icon(&crate::icons::ICON_X, 14.0).color(theme.text_secondary))
        .on_press(move |state: &mut AppState| close(state))
}

/// Import dialog (React ConnectionShareDialog, import/file mode layout).
fn import_dialog_layer(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let body = connections::import_dialog_body(state, theme);

    let footer = row()
        .key("dialog-footer")
        .w_full()
        .h(56.0)
        .px(20.0)
        .center_y()
        .justify_end()
        .gap(8.0)
        .border(1.0, theme.separator)
        .child(
            Button::new("取消")
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::ExtraLarge)
                .on_press(|state: &mut AppState| state.close_import_dialog())
                .build(kit(dark))
                .key("dialog-cancel"),
        )
        .child(
            Button::new("导入连接")
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .on_press(|state: &mut AppState| state.submit_connections_import())
                .build(kit(dark))
                .key("import-confirm"),
        );

    let panel = col()
        .key("import-dialog")
        .w(520.0)
        .max_h(560.0)
        .radius(12.0)
        .fill(theme.card)
        .border(1.0, theme.card_border)
        .child(
            row()
                .w_full()
                .h(56.0)
                .px(20.0)
                .center_y()
                .justify_between()
                .border(1.0, theme.separator)
                .child(
                    text("导入连接")
                        .key("import-dialog-title")
                        .font(type_::TITLE)
                        .color(theme.text),
                )
                .child(close_button("import-dialog-close", theme, |state: &mut AppState| {
                    state.close_import_dialog();
                })),
        )
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .px(20.0)
                .py(20.0)
                .child(body),
        )
        .child(footer);

    dialog_layer("import-dialog-layer", panel, |state: &mut AppState| {
        state.close_import_dialog();
    })
}

/// New-connection dialog (React-aligned: left type sidebar + right header/body/footer).
fn connection_dialog_layer(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let sidebar = connection_type_sidebar(state, theme, dark);

    let header = row()
        .w_full()
        .h(60.0)
        .px(24.0)
        .center_y()
        .justify_between()
        .border(1.0, theme.separator)
        .child(
            text("新建连接")
                .key("connection-dialog-title")
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(close_button(
            "connection-dialog-close",
            theme,
            |state: &mut AppState| state.close_connection_dialog(),
        ));

    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .px(24.0)
        .py(24.0)
        .child(connections::connection_form_fields(state, theme));

    let footer = row()
        .key("connection-dialog-footer")
        .w_full()
        .h(64.0)
        .px(24.0)
        .center_y()
        .justify_end()
        .gap(8.0)
        .border(1.0, theme.separator)
        .child(
            Button::new("测试连接")
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::ExtraLarge)
                // The connection probe lands with the A2 form port; until then the
                // action keeps its place and its button role, exactly as before.
                .on_press(|_state: &mut AppState| {})
                .build(kit(dark))
                .key("connection-test"),
        )
        .child(
            Button::new("取消")
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::ExtraLarge)
                .on_press(|state: &mut AppState| state.close_connection_dialog())
                .build(kit(dark))
                .key("dialog-cancel"),
        )
        .child(
            Button::new("保存")
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .on_press(|state: &mut AppState| state.save_connection_form())
                .build(kit(dark))
                .key("connection-save"),
        );

    let right = col()
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .child(header)
        .child(body)
        .child(footer);

    let panel = row()
        .key("connection-dialog")
        .w(920.0)
        .h(620.0)
        .max_h(700.0)
        .radius(12.0)
        .fill(theme.card)
        .border(1.0, theme.card_border)
        .child(sidebar)
        .child(right);

    dialog_layer("connection-dialog-layer", panel, |state: &mut AppState| {
        state.close_connection_dialog();
    })
}

/// Left sidebar: database type/engine picker (React-aligned structure).
fn connection_type_sidebar(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let mut list = col().w_full().gap(2.0).px(8.0).py(8.0);
    let current = state.connection_engine.text.trim().to_ascii_lowercase();
    let mut engines = state.desktop.available_engines();
    if engines.is_empty() {
        engines = vec![
            "postgresql".to_owned(),
            "mysql".to_owned(),
            "sqlite".to_owned(),
            "redis".to_owned(),
            "mongodb".to_owned(),
        ];
    }

    for engine in engines {
        let is_selected = if current.is_empty() {
            engine.eq_ignore_ascii_case("postgresql")
        } else {
            current == engine.to_ascii_lowercase()
        };
        let eng = engine.clone();
        let label = engine.clone();
        let row_bg = if is_selected { theme.hover } else { Color::TRANSPARENT };
        list = list.child(
            row()
                .key(format!("connection-engine-option-{}", engine))
                .w_full()
                .h(36.0)
                .px(10.0)
                .gap(10.0)
                .center_y()
                .radius(8.0)
                .fill(row_bg)
                .hover(theme.hover)
                .on_press(move |state: &mut AppState| {
                    state.connection_engine = rune::TextEdit::new(eng.clone());
                })
                .child(engine_badge_icon(&engine, 20.0, dark))
                .child(
                    text(label)
                        .font(type_::LABEL)
                        .color(if is_selected { theme.text } else { theme.text_secondary })
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate(),
                ),
        );
    }

    col()
        .w(220.0)
        .h_full()
        .border(1.0, theme.separator)
        .fill(theme.bg)
        .child(
            col()
                .w_full()
                .px(12.0)
                .py(12.0)
                .gap(8.0)
                .border(1.0, theme.separator)
                .child(text("选择数据库类型").font(type_::LABEL).color(theme.text))
                .child(
                    text("从左侧选择类型")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec { stick_bottom: false })
                .child(list),
        )
}