//! Port of `packages/frontend/src/windows/connection/query/QueryBuilderHostAdapter.tsx`.
//!
//! React's adapter is the bridge between the query editor and the Pro query
//! builder: it supplies the active tab's schema and host-owned services to the
//! contribution's `Panel`. `QueryEditorSection` mounts it only while the
//! contribution's `openPanelId === panelId`, toggles it from the toolbar, and
//! rebinds it (via `openFor(panelId, contextKey)`) before paint when the tab's
//! database/schema context changes. All of that wiring is reproduced here:
//!
//! * **mount / unmount** — `None` when no contribution or the builder is closed;
//! * **state relay** — the derived catalog, dialect family, context key and the
//!   pass-through props are rendered into the panel slot so tests can observe
//!   what the real panel would receive;
//! * **actions** — toggle, rebind, commit, cancel and the four service callbacks
//!   are [`QueryBuilderAction`]s dispatched through [`QueryBuilderHostPort`].
//!   No fake IPC, promise or timer lives in the view.
//!
//! ## Keys
//!
//! React's toolbar handle (`editor-visual-builder-button`), the two
//! `query.visualBuilder.*` i18n keys and `common.ok` / `common.cancel` are kept
//! verbatim. The `qb-*` keys name Rune-side handles for the catalog relay and
//! service controls until the real Pro panel is mounted by the host.
//!
//! ## Leftover L1 (removal stage)
//!
//! Mount the real contribution panel from the query-editor host, feed the
//! session schema / dialect registry / settings from its store slice, execute
//! the service requests against the real schema service, and drop the per-item
//! allows and the `qb-*` relay markers.

use rune::{col, icon, row, text, El};

use crate::i18n::t;
use crate::icons::{ICON_SPARKLES, ICON_TABLE, ICON_X};
use crate::ui::kit::kit;

mod catalog;
mod model;

pub use catalog::{
    build_catalog, dialect_family, json_quote, json_stringify_array, qb_context_key,
    relation_identity,
};
pub use model::{
    ColumnVm, CommitMode, QueryBuilderAction, QueryBuilderCatalogVm, QueryBuilderHostPort,
    QueryBuilderVm, RelationColumnsVm, RelationVm, SessionSchemaVm,
};

use catalog::dialect_family as resolve_dialect;

/// Small labelled, pressable control used for the catalog service requests.
/// The labels are plain identifiers (the React panel is Pro-owned), never
/// invented i18n keys.
fn service_button<S: 'static, F: Fn(&mut S) -> Option<QueryBuilderAction> + 'static>(
    key: &str,
    label: &str,
    dark: bool,
    host: &QueryBuilderHostPort<S>,
    action: F,
) -> El {
    let k = kit(dark);
    let mut el = row()
        .key(key)
        .label(label.to_owned())
        .role("button")
        .focusable(true)
        .h(22.0)
        .px(6.0)
        .gap(4.0)
        .center_y()
        .fill(k.c.surface_weak)
        .child(text(label.to_owned()).font(k.ty.text_xsmall).color(k.c.text_muted));
    if let Some(on_action) = host.on_action.clone() {
        el = el.on_press(move |s: &mut S| {
            if let Some(requested) = action(s) {
                on_action(s, requested);
            }
        });
    }
    el
}

/// Build the adapter slot. `None` means React's "no contribution / closed" —
/// nothing is mounted.
#[allow(dead_code)] // view entry point; consumer: the query-editor host (not wired).
pub fn query_builder_host_adapter<S: 'static>(
    vm: &QueryBuilderVm,
    dark: bool,
    host: &QueryBuilderHostPort<S>,
) -> Option<El> {
    if !vm.contribution_available {
        return None;
    }
    let k = kit(dark);
    let open = vm.is_open();
    let context_key = vm.context_key();

    let mut root = col()
        .key("query-builder-host-adapter")
        .w_full()
        .h_full()
        .gap(0.0);

    // Toolbar toggle (`qbOpen ? 'secondary' : 'ghost'` in React).
    let mut toggle = row()
        .key("editor-visual-builder-button")
        .label(t("query.visualBuilder.title"))
        .role("button")
        .focusable(true)
        .h(26.0)
        .px(8.0)
        .center_y()
        .gap(4.0)
        .fill(if open { k.c.surface_base } else { k.c.surface_weak })
        .child(icon(&ICON_SPARKLES, 14.0).color(k.c.text_muted))
        .child(
            text(t("query.visualBuilder.title"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_base),
        );
    if let Some(on_action) = host.on_action.clone() {
        let panel_id = vm.panel_id.clone();
        let key = context_key.clone();
        toggle = toggle.on_press(move |s: &mut S| {
            on_action(s, QueryBuilderAction::Toggle {
                panel_id: panel_id.clone(),
                context_key: key.clone(),
                open,
            })
        });
    }
    root = root.child(row().key("qb-toolbar").w_full().h(32.0).gap(6.0).center_y().child(toggle));

    if !open {
        // The editor is the active slot. React keeps it mounted but CSS-hidden
        // while the builder is open; Rune models whichever slot is active.
        return Some(root.child(
            col()
                .key("qb-editor-slot")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .gap(4.0)
                .child(
                    text(vm.current_sql.clone())
                        .key("qb-editor-sql")
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        ));
    }

    let catalog = build_catalog(vm.session_schema.as_ref(), &vm.database, vm.schema.as_deref());
    let dialect = resolve_dialect(&vm.database_type, vm.dialect_override.as_deref());
    let stale = vm.needs_rebind();

    let mut panel = col()
        .key("qb-panel")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(4.0);

    // Panel header with a close (×) that maps to `closeFor('cancel')`.
    let mut close = icon(&ICON_X, 14.0)
        .color(k.c.text_muted)
        .key("qb-panel-close")
        .role("button")
        .label(t("common.close"))
        .focusable(true);
    if let Some(on_action) = host.on_action.clone() {
        close = close.on_press(move |s: &mut S| on_action(s, QueryBuilderAction::Cancel));
    }
    panel = panel.child(
        row()
            .key("qb-panel-header")
            .w_full()
            .h(30.0)
            .gap(6.0)
            .center_y()
            .child(icon(&ICON_SPARKLES, 14.0).color(k.c.text_accent))
            .child(
                text(t("query.visualBuilder.title"))
                    .font(k.ty.text_small)
                    .color(k.c.text_base),
            )
            .child(row().grow(1.0))
            .child(close),
    );

    // State relay: the context key passed to `openFor`, the key it was last
    // bound to, and a stale marker the host reads to issue `rebind_action()`.
    panel = panel.child(
        col()
            .key("qb-context-relay")
            .w_full()
            .gap(2.0)
            .child(
                text(context_key.clone())
                    .key("qb-context-key")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                text(vm.bound_context_key.clone().unwrap_or_default())
                    .key("qb-bound-context")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            ),
    );
    if stale {
        panel = panel.child(
            text(context_key.clone())
                .key("qb-context-stale")
                .font(k.ty.text_xsmall)
                .color(k.c.danger.fg),
        );
    }

    // Catalog relay: every unique table, its schema, its columns and the
    // per-table `loadTableSchema` request.
    let mut all_columns: Vec<String> = Vec::new();
    for relation in &catalog.tables {
        let mut table_row = row()
            .key(format!("qb-catalog-table-{}", relation.name))
            .w_full()
            .h(22.0)
            .gap(4.0)
            .center_y()
            .child(icon(&ICON_TABLE, 12.0).color(k.c.text_muted))
            .child(
                text(relation.name.clone())
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base),
            );
        if let Some(schema) = &relation.schema {
            table_row = table_row.child(
                text(schema.clone())
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        }
        if let Some(on_action) = host.on_action.clone() {
            let name = relation.name.clone();
            let schema = relation.schema.clone();
            table_row = table_row.child(
                row()
                    .key(format!("qb-load-schema-{}", relation.name))
                    .role("button")
                    .focusable(true)
                    .child(icon(&ICON_SPARKLES, 10.0).color(k.c.text_muted))
                    .on_press(move |s: &mut S| {
                        on_action(s, QueryBuilderAction::LoadTableSchema {
                            name: name.clone(),
                            schema: schema.clone(),
                        })
                    }),
            );
        }
        panel = panel.child(table_row);
        let columns = catalog
            .column_map
            .get(&relation.name)
            .cloned()
            .unwrap_or_default();
        all_columns.extend(columns.iter().cloned());
        panel = panel.child(
            text(columns.join(", "))
                .key(format!("qb-columns-{}", relation.name))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }

    // Host service relay: dialect family, FK prediction flag and current SQL.
    panel = panel.child(
        col()
            .key("qb-service-relay")
            .w_full()
            .gap(2.0)
            .child(
                text(dialect.clone())
                    .key("qb-dialect")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                text(vm.enable_fk_prediction.to_string())
                    .key("qb-fk-prediction")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                text(vm.current_sql.clone())
                    .key("qb-current-sql")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            ),
    );

    let db_session_id = vm.db_session_id.clone();
    let database = vm.database.clone();
    let schema = vm.schema.clone();
    let sql = vm.current_sql.clone();
    let database_type = vm.database_type.clone();
    let options = vm.sql_format_options.clone();
    let predict_schemas = catalog.tables.clone();
    panel = panel.child(
        row()
            .key("qb-service-actions")
            .w_full()
            .gap(6.0)
            .child(service_button("qb-ensure-columns", "ensureColumns", dark, host, move |_| {
                // React's ensureColumns returns early without a session/db or
                // when no column names were requested — emit nothing.
                if db_session_id.is_empty() || database.trim().is_empty() || all_columns.is_empty() {
                    return None;
                }
                Some(QueryBuilderAction::EnsureColumns {
                    db_session_id: db_session_id.clone(),
                    database: database.clone(),
                    schema: schema.clone(),
                    names: all_columns.clone(),
                })
            }))
            .child(service_button(
                "qb-predict",
                "predictRelations",
                dark,
                host,
                move |_| Some(QueryBuilderAction::PredictRelations { schemas: predict_schemas.clone() }),
            ))
            .child(service_button("qb-format", "formatSql", dark, host, move |_| {
                Some(QueryBuilderAction::FormatSql {
                    sql: sql.clone(),
                    database_type: database_type.clone(),
                    options: options.clone(),
                })
            })),
    );

    // OK / Cancel map to the adapter's `onCommit` / `onCancel` props.
    let mut ok = row()
        .key("qb-commit")
        .label(t("common.ok"))
        .role("button")
        .focusable(true)
        .h(26.0)
        .px(12.0)
        .center_y()
        .fill(k.c.text_accent)
        .child(text(t("common.ok")).font(k.ty.text_xsmall).color(k.c.text_inverse));
    if let Some(on_action) = host.on_action.clone() {
        let sql = vm.current_sql.clone();
        ok = ok.on_press(move |s: &mut S| {
            on_action(s, QueryBuilderAction::Commit {
                sql: sql.clone(),
                mode: CommitMode::Replace,
            })
        });
    }
    let mut cancel = row()
        .key("qb-cancel")
        .label(t("common.cancel"))
        .role("button")
        .focusable(true)
        .h(26.0)
        .px(12.0)
        .center_y()
        .fill(k.c.surface_weak)
        .child(
            text(t("common.cancel"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_base),
        );
    if let Some(on_action) = host.on_action.clone() {
        cancel = cancel.on_press(move |s: &mut S| on_action(s, QueryBuilderAction::Cancel));
    }
    panel = panel.child(
        row().key("qb-panel-actions").w_full().gap(8.0).child(ok).child(cancel),
    );

    if let Some(message) = &vm.toast {
        panel = panel.child(
            text(message.clone())
                .key("qb-toast")
                .font(k.ty.text_xsmall)
                .color(k.c.text_accent),
        );
    }

    Some(root.child(panel))
}
