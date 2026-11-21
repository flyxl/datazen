//! The icon-only toolbar of `QueryEditorSection` (React lines 384–574), plus
//! the execution-strategy dropdown that stands in for React's native menu.
//! Every control is a keyed host-intent emitter; no command runs here.

use std::rc::Rc;

use rune::{col, row, text, El};

use super::{
    emit, qb_is_open, qb_needs_rebind, qb_context_key, ActionFn, ExecutionStrategy,
    QueryEditorAction, QueryEditorSectionVm,
};
use crate::i18n::t;
use crate::icons::{
    ICON_CHECK, ICON_CLOCK, ICON_LOADER2, ICON_PLAY, ICON_PLUS, ICON_REFRESH, ICON_SEARCH,
    ICON_SPARKLES, ICON_STAR, ICON_TABLE, ICON_TERMINAL, ICON_X,
};
use crate::ui::kit::kit;
use crate::ui::toolbar::{ToolbarButton, ToolbarButtonVariant, ToolbarShell};

pub(super) fn build<S: 'static>(
    vm: &QueryEditorSectionVm,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut children: Vec<El> = Vec::new();
    if vm.has_context_selectors {
        children.push(
            row()
                .key("query-context-selectors")
                .role("group")
                .w(180.0)
                .min_h(24.0)
                .shrink(),
        );
    }

    if vm.running {
        let cancel = button(
            "query-execution-cancel",
            t("common.cancel"),
            &ICON_X,
            ToolbarButtonVariant::Danger,
            false,
            on_action,
            QueryEditorAction::Cancel,
            dark,
        );
        children.push(
            row()
                .key("query-execution-status")
                .gap(6.0)
                .center_y()
                .child(text(t("query.running")).color(k.c.text_muted))
                .child(cancel),
        );
    } else {
        children.push(button(
            "editor-execute-button",
            t("query.execute"),
            &ICON_PLAY,
            ToolbarButtonVariant::Run,
            false,
            on_action,
            QueryEditorAction::Execute,
            dark,
        ));
    }

    children.push(strategy_button(vm, on_action, dark));
    children.push(button(
        "editor-save-button",
        t("common.save"),
        &ICON_STAR,
        ToolbarButtonVariant::Ghost,
        vm.running || vm.sql.trim().is_empty(),
        on_action,
        QueryEditorAction::OpenAddFavorite(vm.sql.clone()),
        dark,
    ));
    children.push(button(
        "editor-nl2sql-button",
        t("nl2sql.title"),
        &ICON_SPARKLES,
        if vm.nl2sql_visible { ToolbarButtonVariant::Secondary } else { ToolbarButtonVariant::Ghost },
        false,
        on_action,
        QueryEditorAction::ToggleNl2sql,
        dark,
    ));
    children.push(divider("query-toolbar-divider-main"));

    children.push(button(
        "editor-format-button",
        t("query.format"),
        &ICON_TERMINAL,
        ToolbarButtonVariant::Ghost,
        vm.running || vm.sql.trim().is_empty(),
        on_action,
        QueryEditorAction::Format,
        dark,
    ));
    if vm.supports_explain {
        children.push(button(
            "editor-explain-button",
            t("explain.title"),
            &ICON_SEARCH,
            ToolbarButtonVariant::Ghost,
            vm.running || vm.sql.trim().is_empty(),
            on_action,
            QueryEditorAction::Explain,
            dark,
        ));
    }
    children.push(button(
        "editor-snippets-button",
        t("query.snippets"),
        &ICON_TABLE,
        ToolbarButtonVariant::Ghost,
        vm.running,
        on_action,
        QueryEditorAction::Format,
        dark,
    ));
    children.push(button(
        "editor-refresh-completion-button",
        t("query.refreshCompletion"),
        &ICON_REFRESH,
        ToolbarButtonVariant::Ghost,
        vm.running,
        on_action,
        QueryEditorAction::RefreshCompletion,
        dark,
    ));

    if vm.query_builder_available {
        let builder_action = if qb_is_open(vm) {
            QueryEditorAction::CloseVisualBuilder { mode: "hide".to_owned() }
        } else {
            QueryEditorAction::OpenVisualBuilder {
                panel_id: vm.panel_id.clone(),
                context_key: qb_context_key(
                    &vm.connection_id,
                    &vm.db_session_id,
                    vm.selected_database.as_deref(),
                    vm.selected_schema.as_deref(),
                ),
            }
        };
        children.push(button(
            "editor-visual-builder-button",
            t("query.visualBuilder.title"),
            &ICON_SPARKLES,
            if qb_is_open(vm) { ToolbarButtonVariant::Secondary } else { ToolbarButtonVariant::Ghost },
            false,
            on_action,
            builder_action,
            dark,
        ));
    }

    if vm.in_transaction {
        children.push(divider("query-toolbar-divider-tx"));
        children.push(button(
            "query-commit-tx",
            t("query.commitTx"),
            &ICON_CHECK,
            ToolbarButtonVariant::Ghost,
            vm.running || vm.tx_busy,
            on_action,
            QueryEditorAction::CommitTx,
            dark,
        ));
        children.push(button(
            "query-rollback-tx",
            t("query.rollbackTx"),
            &ICON_X,
            ToolbarButtonVariant::Danger,
            vm.running || vm.tx_busy,
            on_action,
            QueryEditorAction::RollbackTx,
            dark,
        ));
        children.push(
            row()
                .key("query-in-transaction")
                .label(t("query.inTransaction"))
                .child(text(if vm.compact_toolbar { "TX".to_owned() } else { t("query.inTransaction").to_owned() })),
        );
    } else {
        children.push(button(
            "editor-begin-tx-button",
            t("query.beginTx"),
            &ICON_PLUS,
            ToolbarButtonVariant::Ghost,
            vm.running || vm.tx_busy,
            on_action,
            QueryEditorAction::BeginTx,
            dark,
        ));
    }

    children.push(row().key("query-toolbar-spacer").grow(1.0).min_w(0.0));
    if vm.safe_mode {
        children.push(
            row()
                .key("query-safe-mode")
                .label(t("settings.safeMode"))
                .child(text(if vm.compact_toolbar { "Safe".to_owned() } else { t("settings.safeMode").to_owned() })),
        );
    }
    if !vm.compact_toolbar {
        children.push(
            text(format!("Mod+Enter {}", t("query.execute")))
                .key("query-toolbar-shortcut")
                .color(k.c.text_muted),
        );
    }
    if let Some(rows) = vm.row_count {
        let copy = if vm.compact_toolbar {
            format!("{rows} {}", t("common.rows"))
        } else {
            t("query.historyRows").replace("{count}", &rows.to_string())
        };
        children.push(text(copy).key("query-row-count").color(k.c.text_muted));
    }
    if let Some(rows) = vm.affected_rows {
        let copy = if vm.compact_toolbar {
            format!("{rows} {}", t("query.affectedRows"))
        } else {
            t("query.rowsAffectedCount").replace("{count}", &rows.to_string())
        };
        children.push(text(copy).key("query-affected-rows").color(k.c.text_muted));
    }
    if let Some(ms) = vm.execution_time_ms {
        let copy = if vm.compact_toolbar {
            format!("{ms} ms")
        } else {
            format!("{} {ms} ms", t("query.totalTime"))
        };
        children.push(text(copy).key("query-execution-time").color(k.c.text_muted));
    }
    children.push(button(
        "editor-history-toggle",
        t("query.history"),
        &ICON_CLOCK,
        if vm.history_visible { ToolbarButtonVariant::Secondary } else { ToolbarButtonVariant::Ghost },
        false,
        on_action,
        QueryEditorAction::ToggleHistory,
        dark,
    ));
    children.push(button(
        "editor-favorites-toggle",
        t("query.favorites"),
        &ICON_STAR,
        if vm.favorites_visible { ToolbarButtonVariant::Secondary } else { ToolbarButtonVariant::Ghost },
        false,
        on_action,
        QueryEditorAction::ToggleFavorites,
        dark,
    ));

    let mut bar = ToolbarShell::new("query-editor-toolbar")
        .children(children)
        .build(dark)
        .h(36.0)
        .flex_wrap();
    if qb_needs_rebind(vm) {
        bar = bar.child(rebind_marker(vm));
    }
    bar
}

/// The small dropdown standing in for React's native strategy context menu.
fn strategy_button<S: 'static>(
    vm: &QueryEditorSectionVm,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let active = t(vm.execution_strategy.label_key()).to_owned();
    row().key("execution-strategy-group").center_y().child(button(
        "editor-execution-strategy-button",
        active.as_str(),
        &ICON_LOADER2,
        ToolbarButtonVariant::Ghost,
        vm.running,
        on_action,
        QueryEditorAction::ToggleExecutionStrategy,
        dark,
    ))
}

/// The dropdown lives below the 36pt toolbar so its items are never clipped by
/// the toolbar's scroll viewport.
pub(super) fn strategy_menu<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let mut menu = col().key("execution-strategy-menu").role("menu").gap(2.0);
    for strategy in ExecutionStrategy::ALL {
        menu = menu.child(
            text(t(strategy.label_key()))
                .key(format!("execution-strategy-{}", strategy.label_key()))
                .role("menuitem")
                .focusable(true)
                .on_press(emit(on_action, QueryEditorAction::SetExecutionStrategy(strategy))),
        );
    }
    menu
}

/// Observable marker for the useLayoutEffect rebind (the host reads the
/// `OpenVisualBuilder` action to perform the actual openFor call).
fn rebind_marker(vm: &QueryEditorSectionVm) -> El {
    row()
        .key("query-builder-rebind")
        .label(qb_context_key(
            &vm.connection_id,
            &vm.db_session_id,
            vm.selected_database.as_deref(),
            vm.selected_schema.as_deref(),
        ))
        .a11y_hidden()
}

fn divider(key: &str) -> El {
    row().key(key).w(1.0).h(16.0).shrink()
}

#[allow(clippy::too_many_arguments)]
fn button<S: 'static>(
    key: &str,
    label: &str,
    glyph: &'static rune::Icon,
    variant: ToolbarButtonVariant,
    disabled: bool,
    on_action: &Option<ActionFn<S>>,
    action: QueryEditorAction,
    dark: bool,
) -> El {
    let mut b = ToolbarButton::new(key, label.to_owned())
        .icon(glyph)
        .icon_only(true)
        .variant(variant)
        .disabled(disabled);
    if let Some(callback) = on_action.clone() {
        b = b.on_press(move |s: &mut S| callback(s, action.clone()));
    }
    b.build(dark).key(key)
}

// Keep Rc imported for the action alias used by this module's helpers.
#[allow(dead_code)]
type _ActionAlias<S> = Rc<dyn Fn(&mut S) + 'static>;
