//! Live SQLite transfer, intentionally separate from the demo data tools.
use crate::state_def::AppState;
use datazen_ui_api::native_transfer::{TransferChoice, TransferSide};
use rune::tok::{self, type_};
use rune::{col, row, text, El, ScrollSpec};

fn button(
    key: impl Into<String>,
    label: impl Into<String>,
    theme: tok::Theme,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    let label = label.into();
    row()
        .key(key.into())
        .h(24.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .hover(theme.hover)
        .role("button")
        .focusable(true)
        .label(label.clone())
        .child(text(label).font(type_::LABEL).color(theme.text))
        .on_press(on_press)
}

fn label(key: &str, value: impl Into<String>, theme: tok::Theme) -> El {
    text(value.into())
        .key(key)
        .font(type_::LABEL)
        .w_full()
        .color(theme.text)
}

fn endpoint(state: &AppState, side: TransferSide, theme: tok::Theme) -> El {
    let prefix = side.key();
    let endpoint = state.native_transfer.side(side);
    let mut panel = col()
        .key(format!("native-transfer-{prefix}"))
        .w_full()
        .gap(5.0);
    panel = panel.child(label(
        "native-transfer-side",
        if side == TransferSide::Source {
            "Source"
        } else {
            "Target"
        },
        theme,
    ));
    let profiles = state.visible_connections();
    let mut list = row().w_full().gap(5.0).flex_wrap();
    for profile in profiles.iter().filter(|profile| profile.engine == "sqlite") {
        let connection_id = profile.id.clone();
        let selected = endpoint
            .active
            .as_ref()
            .is_some_and(|choice| choice.connection_id == connection_id);
        let caption = if selected {
            format!("✓ {}", profile.name)
        } else {
            profile.name.clone()
        };
        list = list.child(button(
            format!("native-transfer-{prefix}-profile-{connection_id}"),
            caption,
            theme,
            move |state| {
                state.choose_transfer(
                    side,
                    TransferChoice {
                        connection_id: connection_id.clone(),
                        table: None,
                    },
                );
            },
        ));
    }
    panel = panel.child(list);
    if endpoint.loading {
        return panel.child(label("native-transfer-loading", "Reading catalog…", theme));
    }
    if let Some(error) = &endpoint.error {
        return panel.child(label(
            "native-transfer-endpoint-error",
            error.clone(),
            theme,
        ));
    }
    let Some(catalog) = &endpoint.catalog else {
        return panel;
    };
    let current = catalog.choice.table.as_deref().unwrap_or("No tables");
    panel = panel.child(label("native-transfer-selected-table", current, theme));
    if !catalog.tables.is_empty() {
        let next = catalog
            .tables
            .iter()
            .position(|t| t == current)
            .map_or(0, |i| (i + 1) % catalog.tables.len());
        let choice = TransferChoice {
            connection_id: catalog.choice.connection_id.clone(),
            table: Some(catalog.tables[next].clone()),
        };
        panel = panel.child(button(
            format!("native-transfer-{prefix}-table"),
            "Next table",
            theme,
            move |state| {
                state.choose_transfer(side, choice.clone());
            },
        ));
    }
    if let Some(column) = endpoint.selected_column() {
        panel = panel.child(button(
            format!("native-transfer-{prefix}-column"),
            format!("Column: {column}"),
            theme,
            move |state| {
                state.native_transfer.step_column(side);
            },
        ));
    }
    if side == TransferSide::Target && catalog.read_only {
        panel = panel.child(label(
            "native-transfer-read-only",
            "Target is read-only",
            theme,
        ));
    }
    panel
}

pub(super) fn view(state: &AppState, theme: tok::Theme) -> El {
    let transfer = &state.native_transfer;
    let mut page = col()
        .key("native-transfer")
        .w_full()
        .h_full()
        .min_h(0.0)
        .gap(9.0)
        .padding(10.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            text("SQLite Data Transfer")
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(label(
            "native-transfer-scope",
            "Insert into an existing table. No schema changes or overwrites.",
            theme,
        ));
    if let Some(approved) = &transfer.confirming {
        let caption = |id: &str| {
            state
                .visible_connections()
                .into_iter()
                .find(|p| p.id == id)
                .map_or_else(|| id.to_owned(), |p| p.name)
        };
        let reviewed = format!(
            "From {} / {} to {} / {}",
            caption(&approved.source.connection_id),
            approved.source.table,
            caption(&approved.target.connection_id),
            approved.target.table
        );
        page = page.child(label("native-transfer-confirm", reviewed, theme));
        for (index, (left, right)) in approved.columns.iter().enumerate() {
            page = page.child(label(
                &format!("native-transfer-confirm-pair-{index}"),
                format!("{left} → {right}"),
                theme,
            ));
        }
        return page.child(
            row()
                .gap(8.0)
                .child(button("native-transfer-yes", "Insert rows", theme, |s| {
                    s.start_transfer()
                }))
                .child(button("native-transfer-no", "Cancel", theme, |s| {
                    s.native_transfer.confirming = None
                })),
        );
    }
    if transfer.running {
        return page.child(label(
            "native-transfer-running",
            "Transferring rows…",
            theme,
        ));
    }
    page = page
        .child(endpoint(state, TransferSide::Source, theme))
        .child(endpoint(state, TransferSide::Target, theme))
        .child(button(
            "native-transfer-map",
            "Map selected columns",
            theme,
            |s| s.native_transfer.add_pair(),
        ));
    for (index, (left, right)) in transfer.pairs.iter().enumerate() {
        page = page.child(
            row()
                .gap(6.0)
                .child(
                    text(format!("{left} → {right}"))
                        .key(format!("native-transfer-pair-{index}"))
                        .grow(1.0)
                        .min_w(0.0)
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .child(button(
                    format!("native-transfer-remove-{index}"),
                    "Remove",
                    theme,
                    move |s| s.native_transfer.remove_pair(index),
                )),
        );
    }
    page = page.child(button(
        "native-transfer-review",
        "Review transfer",
        theme,
        |s| s.native_transfer.review(),
    ));
    if let Some(error) = &transfer.error {
        page = page.child(label("native-transfer-error", error.clone(), theme));
    }
    if let Some(rows) = transfer.inserted {
        page = page.child(label(
            "native-transfer-success",
            format!("Inserted {rows} rows."),
            theme,
        ));
    }
    page
}
