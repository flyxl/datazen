//! The two overlays of `IndexesView`: create-index and delete confirmation.
//!
//! Both are appended to the views layer as absolutely positioned scrim + layer
//! pairs (the same shape as `ui/dialog.rs`), so they cover the table without
//! consuming layout space. Every control emits an [`IndexesAction`] through the
//! injected host port; the SQL strings are pure helpers from the parent module.

use rune::{col, el, icon as rune_icon, row, text, El, ScrollSpec};

use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_PLUS, ICON_TRASH};
use crate::ui::field::TextField;
use crate::ui::kit::kit;

use super::{
    build_create_index_sql, build_drop_index_sql, button, emit, suggested_index_name,
    supported_methods, IndexesAction, IndexesHostPort, IndexesIdentity, IndexesViewState,
    IndexesVmFn,
};

/// A small check box, shared by the column list and the unique option.
fn check_box(checked: bool, key: &str, dark: bool) -> El {
    let k = kit(dark);
    let mut box_el = row()
        .key(key.to_owned())
        .size(14.0, 14.0)
        .center()
        .radius(3.0)
        .border(1.0, k.c.border_base);
    if checked {
        box_el = box_el
            .fill(k.c.bg_accent)
            .child(rune_icon(&ICON_CHECK, 10.0).color(k.c.text_inverse));
    }
    box_el
}

/// React `CreateIndexDialog`.
#[allow(dead_code)] // dialog entry; remove when the host mounts the view.
pub fn build_create<S: 'static>(
    identity: &IndexesIdentity,
    state: &IndexesViewState,
    host: &IndexesHostPort<S>,
    vm_edit: Option<IndexesVmFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    layer.push(
        el()
            .key("idx-create-scrim")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(k.c.scrim)
            .a11y_hidden(),
    );

    // The name input is only editable when the host supplies a VM accessor;
    // without one it is a bound, disabled display (React's value is still read
    // for SQL generation).
    let mut field = TextField::new("idx-name-input", &state.index_name)
        .height(30.0)
        .text_style(k.ty.text_small)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .radius(6.0)
        .pad_x(8.0);
    if let Some(vm_edit) = vm_edit {
        let table = identity.table_name.clone();
        field = field.on_edit(rune::edit_access(move |s: &mut S| {
            let vm = vm_edit(s);
            // Rune binds the accessor on mount too; only a real divergence from
            // the auto name counts as React's `nameTouched` user edit.
            let auto = suggested_index_name(&table, &vm.selected_cols);
            if vm.index_name.text != auto {
                vm.name_touched = true;
            }
            &mut vm.index_name
        }));
    } else {
        field = field.disabled(true);
    }
    let field_el = field.build(dark);

    let mut columns = col()
        .key("idx-cols")
        .w_full()
        .max_h(132.0)
        .min_h(0.0)
        .gap(2.0)
        .padding(6.0)
        .radius(6.0)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .scroll(ScrollSpec { stick_bottom: false });
    for column in &state.columns {
        let checked = state.selected_cols.iter().any(|c| c == &column.name);
        let order = state
            .selected_cols
            .iter()
            .position(|c| c == &column.name)
            .map(|at| at + 1);
        let mut option = row()
            .key(format!("idx-col-toggle-{}", column.name))
            .w_full()
            .h(24.0)
            .gap(8.0)
            .center_y()
            .px(4.0)
            .radius(4.0)
            .child(check_box(checked, &format!("idx-col-box-{}", column.name), dark))
            .child(
                text(column.name.clone())
                    .key(format!("idx-col-name-{}", column.name))
                    .font(k.ty.mono)
                    .color(k.c.text_base),
            )
            .child(
                text(column.data_type.clone())
                    .key(format!("idx-col-type-{}", column.name))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        if let Some(order) = order {
            option = option.child(
                text(format!("#{order}"))
                    .key(format!("idx-col-order-{}", column.name))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent),
            );
        }
        if let Some(press) = emit(
            &host.on_action,
            IndexesAction::ToggleColumn(column.name.clone()),
        ) {
            option = option.on_press(move |s: &mut S| press(s));
        }
        columns = columns.child(option);
    }

    let mut unique = row()
        .key("idx-unique-toggle")
        .gap(6.0)
        .center_y()
        .child(check_box(state.is_unique, "idx-unique-box", dark))
        .child(
            text(t("indexes.unique"))
                .font(k.ty.text_small)
                .color(k.c.text_base),
        );
    if let Some(press) = emit(&host.on_action, IndexesAction::SetUnique(!state.is_unique)) {
        unique = unique.on_press(move |s: &mut S| press(s));
    }

    let mut methods = row()
        .key("idx-method-select")
        .gap(4.0)
        .center_y()
        .child(
            text(t("indexes.indexType"))
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        );
    for method in supported_methods(&identity.database_type) {
        let selected = method == state.index_type;
        let mut option = row()
            .key(format!("idx-method-{}", method.as_str()))
            .h(22.0)
            .px(6.0)
            .center_y()
            .radius(4.0)
            .fill(if selected {
                k.alpha(k.c.text_accent, 0.12)
            } else {
                k.c.surface_raised
            })
            .child(
                text(method.label())
                    .font(k.ty.text_xsmall)
                    .color(if selected {
                        k.c.text_accent
                    } else {
                        k.c.text_muted
                    }),
            );
        if let Some(press) = emit(&host.on_action, IndexesAction::SetMethod(method)) {
            option = option.on_press(move |s: &mut S| press(s));
        }
        methods = methods.child(option);
    }

    let mut card = col()
        .key(super::CREATE_KEY)
        .role("dialog")
        .label(t("common.newIndex"))
        .w_full()
        .max_w(480.0)
        .max_h(560.0)
        .radius(10.0)
        .fill(k.c.bg_base)
        .border(1.0, k.c.border_base)
        .padding(20.0)
        .gap(12.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(
            text(t("common.newIndex"))
                .key("idx-create-title")
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(
            col()
                .key("idx-name-row")
                .w_full()
                .gap(4.0)
                .child(
                    text(t("indexes.indexName"))
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                )
                .child(field_el),
        )
        .child(
            col()
                .key("idx-cols-row")
                .w_full()
                .gap(4.0)
                .child(
                    text(t("indexes.selectColumns"))
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                )
                .child(columns),
        )
        .child(row().key("idx-options").w_full().gap(16.0).center_y().child(unique).child(methods));

    if !state.selected_cols.is_empty() {
        let sql = build_create_index_sql(
            &identity.database_type,
            &identity.table_name,
            &state.effective_name(&identity.table_name),
            &state.selected_cols,
            state.is_unique,
            state.index_type,
        );
        card = card.child(
            col()
                .key("idx-sql-preview")
                .w_full()
                .gap(4.0)
                .padding(8.0)
                .radius(6.0)
                .fill(k.c.surface_base)
                .border(1.0, k.c.border_base)
                .child(
                    text(t("common.sqlPreview"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                )
                .child(
                    text(sql)
                        .key("idx-sql-text")
                        .font(k.ty.mono)
                        .color(k.c.success.fg),
                ),
        );
    }

    let create_press = if state.selected_cols.is_empty() || state.submitting {
        None
    } else {
        let sql = build_create_index_sql(
            &identity.database_type,
            &identity.table_name,
            &state.effective_name(&identity.table_name),
            &state.selected_cols,
            state.is_unique,
            state.index_type,
        );
        emit(&host.on_action, IndexesAction::CreateIndex { sql })
    };
    card = card.child(
        row()
            .key("idx-create-actions")
            .w_full()
            .justify_end()
            .gap(8.0)
            .child(button::<S>(
                "idx-create-cancel",
                t("common.cancel").to_owned(),
                None,
                emit(&host.on_action, IndexesAction::CloseCreate),
                false,
                dark,
            ))
            .child(button::<S>(
                "idx-create-submit",
                t("indexes.createIndex").to_owned(),
                Some(&ICON_PLUS),
                create_press,
                false,
                dark,
            )),
    );

    // React's dialog is modal; Rune needs the panel on the modal stack so its
    // controls receive input above the table behind it (and Escape dismisses).
    let dismiss = emit(&host.on_action, IndexesAction::CloseCreate);
    let card = card.modal::<S>(true, move |s: &mut S| {
        if let Some(dismiss) = &dismiss {
            dismiss(s);
        }
    });
    layer.push(
        el()
            .key("idx-create-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .padding(16.0)
            .center()
            .child(card),
    );
}

/// React `DeleteConfirmDialog`.
#[allow(dead_code)] // dialog entry; remove when the host mounts the view.
pub fn build_delete<S: 'static>(
    identity: &IndexesIdentity,
    state: &IndexesViewState,
    host: &IndexesHostPort<S>,
    name: &str,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    layer.push(
        el()
            .key("idx-delete-scrim")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(k.c.scrim)
            .a11y_hidden(),
    );

    let confirm_press = if identity.safe_mode || state.submitting {
        None
    } else {
        let sql = build_drop_index_sql(&identity.database_type, name, &identity.table_name);
        emit(&host.on_action, IndexesAction::DropIndex { sql })
    };
    let message = t("indexes.confirmDeleteMsg").replace("{name}", name);
    let card = col()
        .key(super::DELETE_KEY)
        .role("dialog")
        .label(t("indexes.confirmDeleteTitle"))
        .w_full()
        .max_w(400.0)
        .radius(10.0)
        .fill(k.c.bg_base)
        .border(1.0, k.c.border_base)
        .padding(20.0)
        .gap(12.0)
        .child(
            row()
                .key("idx-delete-heading")
                .gap(8.0)
                .center_y()
                .child(rune_icon(&ICON_TRASH, 18.0).color(k.c.danger.fg))
                .child(
                    text(t("indexes.confirmDeleteTitle"))
                        .key("idx-delete-title")
                        .font(k.ty.ui_medium)
                        .color(k.c.danger.fg),
                ),
        )
        .child(
            text(message)
                .key("idx-delete-msg")
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        )
        .child(
            row()
                .key("idx-delete-actions")
                .w_full()
                .justify_end()
                .gap(8.0)
                .child(button::<S>(
                    "idx-delete-cancel",
                    t("common.cancel").to_owned(),
                    None,
                    emit(&host.on_action, IndexesAction::CancelDelete),
                    false,
                    dark,
                ))
                .child(button::<S>(
                    "idx-delete-confirm",
                    t("common.delete").to_owned(),
                    Some(&ICON_TRASH),
                    confirm_press,
                    true,
                    dark,
                )),
        );
    let dismiss = emit(&host.on_action, IndexesAction::CancelDelete);
    let card = card.modal::<S>(true, move |s: &mut S| {
        if let Some(dismiss) = &dismiss {
            dismiss(s);
        }
    });
    layer.push(
        el()
            .key("idx-delete-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .padding(16.0)
            .center()
            .child(card),
    );
}
