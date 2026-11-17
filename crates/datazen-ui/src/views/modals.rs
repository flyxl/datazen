//! The modal dialog layer (React-aligned): `ui::dialog`'s scrim and centered
//! panel carry the import and connection dialogs, keeping their panel, field
//! and action keys for the flows and tests that drive them.

use super::*;

use crate::i18n::t;
use crate::state_def::AppState;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use rune::tok::{self, type_};
use rune::{col, row, text, Color, El, ScrollSpec, TextStyle};

use crate::ui::palette;
use rune_kit::{Button, ButtonVariant, ControlSize};

/// The floating layer for whatever dialog is open — the import dialog first
/// (the React share dialog opens at most one), else the connection form.
/// Empty mounts nothing and the window keeps its plain layout.
pub(crate) fn modal_layer(state: &AppState, theme: tok::Theme, dark: bool) -> Vec<El> {
    let mut layer = Vec::new();
    let dialog = if state.import_dialog_open {
        Some(import_dialog(state, theme, dark))
    } else if state.connection_dialog_open {
        Some(connection_dialog(state, theme, dark))
    } else {
        None
    };
    if let Some(dialog) = dialog {
        dialog.build(kit(dark), &mut layer);
    }
    layer
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
    let fill = palette::engine_badge_fill(dark, hue);
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
                .color(palette::badge_ink(dark)),
        )
}

/// Import dialog (React ConnectionShareDialog, import/file mode layout).
fn import_dialog(state: &AppState, theme: tok::Theme, dark: bool) -> Dialog {
    Dialog::new("import-dialog", "导入连接")
        .open(true)
        .open_epoch(state.import_dialog_epoch)
        .width(520.0)
        .fit()
        // React caps the body at 70vh (571.2 at the 1440x816 default window).
        // A view gets no viewport from `rune::Cx` (only `Runtime` has it), so
        // this keeps the pre-port figure for the 640pt minimum window: 448
        // leaves header + body + footer under the 560 the panel used to cap
        // and inside 640. A taller window gets React's content height; a body
        // past the cap scrolls.
        .body_max_height(448.0)
        .body_pad((20.0, 20.0))
        .child(connections::import_dialog_body(state, theme))
        .footer(
            Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::ExtraLarge)
                .on_press(|state: &mut AppState| state.close_import_dialog())
                .build(kit(dark))
                .key("dialog-cancel"),
        )
        .footer(
            Button::new(t("common.importConnections"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .on_press(|state: &mut AppState| state.submit_connections_import())
                .build(kit(dark))
                .key("import-confirm"),
        )
        .on_close(|state: &mut AppState| state.close_import_dialog())
}

/// New-connection dialog (React-aligned: left type sidebar, then the form and
/// the always-visible action row below it).
fn connection_dialog(state: &AppState, theme: tok::Theme, dark: bool) -> Dialog {
    Dialog::new("connection-dialog", "新建连接")
        .open(true)
        .open_epoch(state.connection_dialog_epoch)
        .width(920.0)
        .fill_height()
        .max_height(680.0)
        .header_pad((24.0, 16.0))
        .body_pad((24.0, 24.0))
        .footer_pad((24.0, 12.0))
        .footer_height(64.0)
        .sidebar(connection_type_sidebar(state, theme, dark))
        .child(connections::connection_form_fields(state, theme))
        .footer(
            Button::new(t("newConn.testConnection"))
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::ExtraLarge)
                // The connection probe lands with the A2 form port; until then the
                // action keeps its place and its button role, exactly as before.
                .on_press(|_state: &mut AppState| {})
                .build(kit(dark))
                .key("connection-test"),
        )
        .footer(
            Button::new(t("common.cancel"))
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::ExtraLarge)
                .on_press(|state: &mut AppState| state.close_connection_dialog())
                .build(kit(dark))
                .key("dialog-cancel"),
        )
        .footer(
            Button::new(t("common.save"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .on_press(|state: &mut AppState| state.save_connection_form())
                .build(kit(dark))
                .key("connection-save"),
        )
        .on_close(|state: &mut AppState| state.close_connection_dialog())
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
