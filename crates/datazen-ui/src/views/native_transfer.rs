//! Live SQLite transfer, schema diff and sync preview, separate from demo tools.
use crate::{native_transfer_state::NativeTool, state_def::AppState};
use datazen_ui_api::native_transfer::{TransferChoice, TransferSide, NATIVE_SYNC_ROW_LIMIT};
use rune::tok::{self, type_};
use rune::{col, row, text, El, ScrollSpec};

pub(super) fn button(
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

pub(super) fn label(key: &str, value: impl Into<String>, theme: tok::Theme) -> El {
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
    if let Some(column) = endpoint
        .selected_column()
        .filter(|_| state.native_transfer.tool == NativeTool::Transfer)
    {
        panel = panel.child(button(
            format!("native-transfer-{prefix}-column"),
            format!("Column: {column}"),
            theme,
            move |state| {
                state.native_transfer.step_column(side);
            },
        ));
    }
    if side == TransferSide::Target
        && catalog.read_only
        && state.native_transfer.tool == NativeTool::Transfer
    {
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
    let page = col()
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
            row()
                .gap(8.0)
                .child(button(
                    "native-tool-transfer",
                    "Data Transfer",
                    theme,
                    |s| s.set_native_tool(NativeTool::Transfer),
                ))
                .child(button(
                    "native-tool-schema-diff",
                    "Schema Diff",
                    theme,
                    |s| s.set_native_tool(NativeTool::SchemaDiff),
                ))
                .child(button("native-tool-data-sync", "Data Sync", theme, |s| {
                    s.set_native_tool(NativeTool::SyncPreview)
                })),
        );
    match transfer.tool {
        NativeTool::SchemaDiff => return schema_diff_view(page, state, theme),
        NativeTool::SyncPreview => return sync_preview_view(page, state, theme),
        NativeTool::Transfer => {}
    }
    let mut page = page
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

fn schema_diff_view(page: rune::El, state: &AppState, theme: tok::Theme) -> El {
    let comparison = &state.native_transfer;
    let mut page = page
        .child(
            text("SQLite Schema Diff")
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(label(
            "native-schema-scope",
            "Read-only comparison of table columns. No SQL is applied.",
            theme,
        ))
        .child(endpoint(state, TransferSide::Source, theme))
        .child(endpoint(state, TransferSide::Target, theme));
    if comparison.comparing_schema {
        return page.child(label("native-schema-running", "Comparing columns…", theme));
    }
    page = page.child(button(
        "native-schema-compare",
        "Compare columns",
        theme,
        |s| s.compare_native_schema(),
    ));
    if let Some(error) = &comparison.schema_error {
        return page.child(label("native-schema-error", error.clone(), theme));
    }
    let Some(diff) = &comparison.schema_diff else {
        return page;
    };
    if diff.missing_on_target.is_empty()
        && diff.extra_on_target.is_empty()
        && diff.changed.is_empty()
    {
        return page.child(label(
            "native-schema-equal",
            "No column differences.",
            theme,
        ));
    }
    let mut results = col().key("native-schema-results").w_full().gap(4.0);
    for (key, heading, columns) in [
        ("missing", "Missing on target", &diff.missing_on_target),
        ("extra", "Extra on target", &diff.extra_on_target),
    ] {
        for (index, column) in columns.iter().enumerate() {
            results = results.child(label(
                &format!("native-schema-{key}-{index}"),
                format!("{heading}: {column}"),
                theme,
            ));
        }
    }
    for (index, (column, aspects)) in diff.changed.iter().enumerate() {
        results = results.child(label(
            &format!("native-schema-changed-{index}"),
            format!("Changed on target: {column} ({})", aspects.join(", ")),
            theme,
        ));
    }
    page.child(results)
}

fn sync_preview_view(page: El, state: &AppState, theme: tok::Theme) -> El {
    let sync = &state.native_transfer;
    let mut page = page
        .child(text("SQLite Data Sync Preview").font(type_::TITLE).color(theme.text))
        .child(label(
            "native-sync-scope",
            format!("Read-only preview. Matching columns and primary keys required; up to {NATIVE_SYNC_ROW_LIMIT} rows per table."),
            theme,
        ))
        .child(endpoint(state, TransferSide::Source, theme))
        .child(endpoint(state, TransferSide::Target, theme));
    if sync.previewing_sync {
        return page.child(label("native-sync-running", "Comparing rows…", theme));
    }
    page = page.child(button(
        "native-sync-compare",
        "Preview row differences",
        theme,
        |s| s.preview_native_sync(),
    ));
    if let Some(error) = &sync.sync_error {
        return page.child(label("native-sync-error", error.clone(), theme));
    }
    let Some(preview) = &sync.sync_preview else {
        return page;
    };
    let mut results = col().key("native-sync-results").w_full().gap(4.0);
    for (key, caption, count) in [
        ("native-sync-insert", "Would insert", preview.source_only),
        ("native-sync-update", "Would update", preview.changed),
        ("native-sync-unchanged", "Unchanged", preview.unchanged),
        (
            "native-sync-target-only",
            "Target-only (kept)",
            preview.target_only,
        ),
    ] {
        results = results.child(label(key, format!("{caption}: {count}"), theme));
    }
    page.child(results)
}
