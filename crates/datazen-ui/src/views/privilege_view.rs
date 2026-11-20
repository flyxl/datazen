//! Port of `packages/frontend/src/windows/connection/PrivilegeView.tsx`.
//!
//! React's privilege cockpit: two tree views (by user / by object), a grant
//! dialog and a GRANT/REVOKE SQL box. All heavy work (load, revoke, drop user,
//! grant, SQL execution) is requested through [`PrivilegeHostPort`] — no fake
//! IPC, promise or thread. Tree building and rendering live in submodules.
//!
//! ## Keys
//!
//! React's only test id is `privilege-view`; the rest are Rune-side handles
//! (`priv-mode-*`, `priv-sql-*`, `priv-grant-*`) named after the controls so
//! tests can observe them without viewport coordinates.
//!
//! Leftover L1 (removal stage): mount this from the panel host, feed grants and
//! command definitions from its store slice, bind the host port to the real
//! commands, and drop the per-item allows.

use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_PLUS, ICON_REFRESH};
use crate::ui::field::TextField;
use crate::ui::kit::kit;

mod grant_dialog;
mod grants;
mod model;

pub(crate) use grant_dialog::GrantDialogVm;
pub(crate) use model::{
    build_object_tree, build_user_tree, PrivilegeAction, PrivilegeGrantVm, PrivilegeHostPort,
    PrivilegeVm, ViewMode,
};

use grant_dialog::grant_dialog;
use grants::privilege_tree;

type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;

/// Build the whole privilege panel.
#[allow(dead_code)] // view entry point; consumer: the panel host (not wired).
pub(crate) fn privilege_view<S: 'static>(
    vm: &PrivilegeVm,
    grant: Option<&GrantDialogVm>,
    privilege_options: &[String],
    dark: bool,
    host: &PrivilegeHostPort<S>,
    sql_edit: Option<EditFn<S>>,
    grant_user_edit: Option<EditFn<S>>,
    grant_db_edit: Option<EditFn<S>>,
) -> El {
    let k = kit(dark);
    let mut root = col().key("privilege-view-root").w_full().h_full().gap(0.0);

    // Header: title, view-mode switch, grant and refresh actions.
    let by_user_active = vm.view_mode == ViewMode::ByUser;
    let mut by_user = row()
        .key("priv-mode-by-user")
        .label(t("privileges.byUser"))
        .h(24.0)
        .px(8.0)
        .center_y()
        .focusable(true)
        .fill(if by_user_active { k.c.text_accent } else { k.c.surface_weak })
        .child(
            text(t("privileges.byUser"))
                .font(k.ty.text_xsmall)
                .color(if by_user_active { k.c.text_inverse } else { k.c.text_muted }),
        );
    let mut by_object = row()
        .key("priv-mode-by-object")
        .label(t("privileges.byObject"))
        .h(24.0)
        .px(8.0)
        .center_y()
        .focusable(true)
        .fill(if by_user_active { k.c.surface_weak } else { k.c.text_accent })
        .child(
            text(t("privileges.byObject"))
                .font(k.ty.text_xsmall)
                .color(if by_user_active { k.c.text_muted } else { k.c.text_inverse }),
        );
    if let Some(on_action) = host.on_action.clone() {
        let by_user_action = on_action.clone();
        by_user = by_user.on_press(move |s: &mut S| by_user_action(s, PrivilegeAction::SetViewMode(ViewMode::ByUser)));
        by_object = by_object.on_press(move |s: &mut S| on_action(s, PrivilegeAction::SetViewMode(ViewMode::ByObject)));
    }
    let mut grant_button = icon(&ICON_PLUS, 14.0)
        .color(k.c.text_muted)
        .key("priv-grant-open")
        .role("button")
        .label(t("common.grantPrivileges"))
        .focusable(true);
    let mut refresh_button = icon(&ICON_REFRESH, 14.0)
        .color(k.c.text_muted)
        .key("priv-refresh")
        .role("button")
        .label(t("common.retry"))
        .focusable(true);
    if let Some(on_action) = host.on_action.clone() {
        let grant_action = on_action.clone();
        grant_button = grant_button.on_press(move |s: &mut S| grant_action(s, PrivilegeAction::OpenGrantDialog(None)));
        refresh_button = refresh_button.on_press(move |s: &mut S| on_action(s, PrivilegeAction::Load));
    }
    root = root.child(
        row()
            .key("priv-header")
            .w_full()
            .h(36.0)
            .gap(8.0)
            .center_y()
            .child(text(t("privileges.title")).font(k.ty.text_small).color(k.c.text_base))
            .child(row().grow(1.0))
            .child(row().key("priv-mode-switch").gap(2.0).child(by_user).child(by_object))
            .child(grant_button)
            .child(refresh_button),
    );

    if vm.loading {
        root = root.child(
            row()
                .key("priv-loading")
                .w_full()
                .gap(6.0)
                .child(text(t("common.loading")).font(k.ty.text_xsmall).color(k.c.text_muted)),
        );
    }
    if let Some(error) = &vm.error {
        root = root.child(
            text(error.clone())
                .key("priv-error")
                .font(k.ty.text_xsmall)
                .color(k.c.danger.fg),
        );
    }

    if !vm.loading && vm.error.is_none() {
        let nodes = if vm.view_mode == ViewMode::ByUser {
            build_user_tree(&vm.grants)
        } else {
            build_object_tree(&vm.grants)
        };
        root = root.child(privilege_tree(vm, dark, host, nodes));
    }

    // SQL box: the executed statement and the panel database pin are separate
    // host inputs, matching React's queryCommands.executeQuery call shape.
    let mut sql_field = TextField::new("priv-sql-input", &vm.sql)
        .height(56.0)
        .pad_x(10.0)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base);
    if let Some(accessor) = sql_edit.clone() {
        sql_field = sql_field.on_edit(rune::edit_access(move |s: &mut S| accessor(s)));
    }
    let mut execute = Button::new(t("query.execute"))
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Small)
        .disabled(vm.running || vm.sql.text.trim().is_empty());
    if let Some(on_action) = host.on_action.clone() {
        let (sql, database) = (vm.sql.text.clone(), vm.database.clone());
        execute = execute.on_press(move |s: &mut S| on_action(s, PrivilegeAction::ExecuteSql {
            sql: sql.clone(), database: database.clone(),
        }));
    }
    root = root.child(
        col()
            .key("priv-sql-box")
            .w_full()
            .gap(6.0)
            .child(text(t("privileges.sqlHint")).font(k.ty.text_xsmall).color(k.c.text_muted))
            .child(sql_field.build(dark))
            .child(
                row()
                    .w_full()
                    .gap(8.0)
                    .child(execute.build(k).key("priv-sql-execute"))
                    .child(
                        text(vm.run_message.clone().unwrap_or_default())
                            .key("priv-run-message")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    ),
            ),
    );

    if vm.show_grant {
        if let Some(dialog) = grant {
            root = root.child(grant_dialog(
                dialog, privilege_options, &vm.unique_grantees(), dark, host,
                grant_user_edit, grant_db_edit,
            ));
        }
    }
    root
}
