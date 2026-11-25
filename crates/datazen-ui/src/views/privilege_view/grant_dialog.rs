//! The Grant Privileges dialog: user/database inputs, privilege checkboxes and
//! host-injected grant/cancel actions. No IPC or async lives here.

use std::collections::BTreeSet;
use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit};
use rune_kit::Button;

use crate::i18n::t;
use crate::icons::ICON_X;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

use super::model::{PrivilegeAction, PrivilegeHostPort};

/// React `GrantDialog`'s local state.
#[allow(dead_code)] // view VM; consumer: the privilege host (not wired).
#[derive(Clone, Debug)]
pub struct GrantDialogVm {
    pub username: TextEdit,
    pub database: TextEdit,
    pub selected: BTreeSet<String>,
    pub submitting: bool,
    pub error: Option<String>,
}

impl Default for GrantDialogVm {
    fn default() -> Self {
        Self {
            username: TextEdit::new(String::new()),
            database: TextEdit::new(String::new()),
            selected: BTreeSet::new(),
            submitting: false,
            error: None,
        }
    }
}

#[allow(dead_code)] // view entry point; consumer: the privilege view (not wired).
pub fn grant_dialog<S: 'static>(
    vm: &GrantDialogVm,
    options: &[String],
    users: &[String],
    dark: bool,
    host: &PrivilegeHostPort<S>,
    username_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
    database_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
) -> El {
    let k = kit(dark);
    let mut user_field = TextField::new("priv-grant-user", &vm.username)
        .height(28.0)
        .pad_x(8.0)
        .placeholder(t("privileges.userPlaceholder"));
    if let Some(accessor) = username_edit.clone() {
        user_field = user_field.on_edit(rune::edit_access(move |s: &mut S| accessor(s)));
    }
    let mut db_field = TextField::new("priv-grant-database", &vm.database)
        .height(28.0)
        .pad_x(8.0)
        .placeholder(t("privileges.dbPlaceholder"));
    if let Some(accessor) = database_edit.clone() {
        db_field = db_field.on_edit(rune::edit_access(move |s: &mut S| accessor(s)));
    }

    let mut root = col()
        .key("priv-grant-dialog")
        .w(420.0)
        .gap(10.0)
        .padding(16.0)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .radius(8.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(text(t("common.grantPrivileges")).font(k.ty.text_small).color(k.c.text_base))
                .child({
                    let mut close = icon(&ICON_X, 14.0).color(k.c.text_muted).key("priv-grant-close");
                    if let Some(on_action) = host.on_action.clone() {
                        close = close.on_press(move |s: &mut S| on_action(s, PrivilegeAction::CloseGrantDialog));
                    }
                    close
                }),
        )
        .child(text(t("privileges.targetUser")).font(k.ty.text_xsmall).color(k.c.text_muted))
        .child(user_field.build(dark))
        .child(row().key("priv-grant-users").label(users.join(",")).a11y_hidden())
        .child(text(t("privileges.targetDatabase")).font(k.ty.text_xsmall).color(k.c.text_muted))
        .child(db_field.build(dark))
        .child(
            row()
                .w_full()
                .gap(6.0)
                .center_y()
                .child(text(t("privileges.selectPrivileges")).font(k.ty.text_xsmall).color(k.c.text_muted))
                .child(text_button("priv-grant-select-all", t("common.selectAll").to_owned(), dark, host, |_| PrivilegeAction::SelectAllPrivileges))
                .child(text_button("priv-grant-deselect-all", t("common.deselectAll").to_owned(), dark, host, |_| PrivilegeAction::DeselectAllPrivileges)),
        );

    // The privilege grid is a true checkbox list; toggles are host actions so
    // the dialog never owns the persisted selection.
    let mut grid = row().key("priv-grant-privileges").w_full().gap(6.0).flex_wrap();
    for privilege in options {
        let checked = vm.selected.contains(privilege);
        let mut checkbox = Checkbox::new(
            format!("priv-grant-option-{privilege}"),
            checked,
        )
        .label(privilege.clone());
        if let Some(on_action) = host.on_action.clone() {
            let privilege = privilege.clone();
            checkbox = checkbox.on_toggle(move |s: &mut S| {
                on_action(s, PrivilegeAction::ToggleGrantPrivilege(privilege.clone()));
            });
        }
        grid = grid.child(checkbox.build(k));
    }
    root = root.child(grid);

    if let Some(error) = &vm.error {
        root = root.child(
            text(error.clone())
                .key("priv-grant-error")
                .font(k.ty.text_xsmall)
                .color(k.c.danger.fg),
        );
    }

    let can_grant = !vm.username.text.trim().is_empty() && !vm.selected.is_empty() && !vm.submitting;
    let mut grant = Button::new(t("privileges.grant"))
        .variant(rune_kit::ButtonVariant::Contrast)
        .control_size(rune_kit::ControlSize::Small)
        .disabled(!can_grant);
    if let Some(on_action) = host.on_action.clone() {
        let (username, database, selected) =
            (vm.username.text.trim().to_owned(), vm.database.text.trim().to_owned(), vm.selected.iter().cloned().collect::<Vec<_>>());
        grant = grant.on_press(move |s: &mut S| on_action(s, PrivilegeAction::Grant {
            username: username.clone(), database: database.clone(), privileges: selected.clone(),
        }));
    }
    root.child(
        row()
            .w_full()
            .gap(8.0)
            .child(text_button("priv-grant-cancel", t("common.cancel").to_owned(), dark, host, |_| PrivilegeAction::CloseGrantDialog))
            .child(grant.build(k).key("priv-grant-submit")),
    )
}

fn text_button<S: 'static, F: Fn(&mut S) -> PrivilegeAction + 'static>(
    key: &str,
    label: String,
    dark: bool,
    host: &PrivilegeHostPort<S>,
    action: F,
) -> El {
    let k = kit(dark);
    let mut el = row()
        .key(key)
        .label(label.clone())
        .focusable(true)
        .child(text(label).font(k.ty.text_xsmall).color(k.c.text_accent));
    if let Some(on_action) = host.on_action.clone() {
        el = el.on_press(move |s: &mut S| {
            let requested = action(s);
            on_action(s, requested);
        });
    }
    el
}
