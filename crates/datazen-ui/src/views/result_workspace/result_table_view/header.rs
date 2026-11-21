//! The result grid's status bar, find bar and message banners (React lines
//! 222–267, 461–532). All copy comes from exact locale keys.

use rune::{el, icon, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize};

use super::{emit, ActionFn, ResultTableAction, ResultTableVm};
use crate::i18n::t;
use crate::icons::{ICON_SEARCH, ICON_X};
use crate::ui::kit::kit;

/// React `statusBar`: row/column counts, elapsed time, SQL, truncation notice
/// and the find toggle. The `tid('result-workspace-table')` root is the view.
pub(super) fn status_bar<S: 'static>(vm: &ResultTableVm, on_action: &Option<ActionFn<S>>, dark: bool) -> El {
    let k = kit(dark);
    let mut bar = row()
        .key("result-status-bar")
        .w_full()
        .h(28.0)
        .shrink()
        .gap(12.0)
        .center_y()
        .px(12.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(text(format!("{} {}", vm.rows.len(), t("common.rows"))).color(k.c.text_muted))
        .child(text("|").color(k.c.border_base))
        .child(text(format!("{} {}", vm.columns.len(), t("common.columns"))).color(k.c.text_muted))
        .child(text("|").color(k.c.border_base))
        .child(text(format!("{} ms", vm.execution_time_ms)).color(k.c.text_muted));
    if !vm.sql.is_empty() {
        bar = bar
            .child(text("|").color(k.c.border_base))
            .child(text(vm.sql.clone()).color(k.c.text_muted).truncate().max_w(400.0));
    }
    if vm.truncated {
        bar = bar.child(
            row()
                .key("result-truncated")
                .gap(4.0)
                .center_y()
                .child(text(t("query.resultTruncated").replace("{limit}", &vm.query_result_limit.to_string())).color(k.c.warning.fg)),
        );
    }
    bar.child(
        row()
            .grow(1.0)
            .min_w(0.0),
    )
    .child(
        Button::new(t("resultFind.openLabel"))
            .variant(if vm.find_open { ButtonVariant::Neutral } else { ButtonVariant::Ghost })
            .control_size(ControlSize::Small)
            .icon(&ICON_SEARCH)
            .on_press(emit(on_action, ResultTableAction::ToggleFind))
            .build(k)
            .key("result-find-toggle")
            .label(t("resultFind.openLabel"))
            .help(t("resultFind.open")),
    )
}

/// React `ResultFindBar` (a keyed, honest stand-in for the separate port).
pub(super) fn find_bar<S: 'static>(vm: &ResultTableVm, on_action: &Option<ActionFn<S>>, matched: usize, dark: bool) -> El {
    let k = kit(dark);
    let reason = if vm.safe_mode {
        t("resultFind.safeMode")
    } else {
        t("resultFind.notWritable")
    };
    let mut bar = row()
        .key("result-find-bar")
        .w_full()
        .h(28.0)
        .shrink()
        .gap(8.0)
        .center_y()
        .px(12.0)
        .py(4.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(
            text(format!("{matched} / {}", vm.rows.len()))
                .key("result-find-count")
                .color(k.c.text_muted),
        );
    if !super::can_edit(vm) {
        bar = bar.child(text(reason).key("result-find-reason").color(k.c.text_muted));
    }
    bar.child(
        icon(&ICON_X, 14.0)
            .key("result-find-close")
            .role("button")
            .focusable(true)
            .label(t("common.close"))
            .on_press(emit(on_action, ResultTableAction::CloseFind)),
    )
}

pub(super) fn pending_bar<S: 'static>(vm: &ResultTableVm, on_action: &Option<ActionFn<S>>, dark: bool) -> El {
    let k = kit(dark);
    let count = vm.pending_edits.len();
    let mut bar = row()
        .key("pending-changes-bar")
        .w_full()
        .h(32.0)
        .shrink()
        .gap(8.0)
        .center_y()
        .px(12.0)
        .py(6.0)
        .fill(k.c.warning.bg)
        .child(
            text(format!("{count}"))
                .key("pending-changes-count")
                .font(TextStyle::new(12.0, 600.0))
                .color(k.c.warning.fg),
        );
    for (key, label, action) in [
        ("pending-preview", "resultFind.open", ResultTableAction::PreviewEdits),
        ("pending-rollback", "common.cancel", ResultTableAction::RollbackEdits),
        ("pending-commit", "common.save", ResultTableAction::CommitEdits),
    ] {
        bar = bar.child(
            Button::new(t(label))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .on_press(emit(on_action, action))
                .build(k)
                .key(key),
        );
    }
    bar
}

pub(super) fn read_only_banner(message: String, dark: bool) -> El {
    let k = kit(dark);
    row()
        .key("result-readonly-reason")
        .w_full()
        .px(12.0)
        .py(6.0)
        .child(text(message).color(k.c.text_muted))
}

pub(super) fn notice(message: String, dark: bool) -> El {
    let k = kit(dark);
    row()
        .key("result-edit-notice")
        .w_full()
        .px(12.0)
        .py(6.0)
        .child(text(message).color(k.c.success.fg))
}

pub(super) fn error_banner(message: String, hint: Option<String>, dark: bool) -> El {
    let k = kit(dark);
    let mut el = row()
        .key("result-edit-error")
        .role("alert")
        .w_full()
        .px(12.0)
        .py(6.0)
        .child(text(message).color(k.c.danger.fg));
    if let Some(hint) = hint {
        el = el.child(text(hint).color(k.c.text_muted));
    }
    el
}

/// A zero-size marker carrying a React `data-*` attribute value.
pub(super) fn data_marker(attribute: &str, value: &str) -> El {
    el().key(format!("result-data-{attribute}-{value}")).a11y_hidden().label(value.to_owned())
}
