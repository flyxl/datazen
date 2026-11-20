//! `NavigatorDialogs`, ported 1:1 from
//! `packages/frontend/src/windows/connection/navigator/NavigatorDialogs.tsx`.
//!
//! React is a pure host for five overlays: new group, rename group, the object
//! filter dialog and five confirm dialogs passed in as children. The Rune port
//! keeps the seam with a host-owned [`NavigatorDialogsVm`] and emits
//! [`NavigatorDialogsAction`]s through an injected callback. The object-filter
//! child reuses the existing `ObjectFilterDialog` port. No command, promise or
//! thread is invented here; the host performs add / rename / save + refresh.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts these dialogs yet. The navigator host will own the
//! text edits and map each action onto its store/commands; until then every
//! reusable item carries a per-item `#[allow(dead_code)]` with this removal
//! stage. The five confirm dialogs are host-supplied [`El`]s rendered verbatim
//! (React's `ReactNode` children), so their keys stay the host's responsibility.
//!
//! ## Keys
//!
//! `navigator-new-group-dialog` / `navigator-rename-group-dialog` with `-input`,
//! `-cancel`, `-ok`; the reused object-filter dialog keeps `object-filter-*`.

use std::rc::Rc;

use rune::{El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::components::connection::object_filter_dialog::{
    ObjectFilterAction, ObjectFilterDialog, ObjectFilterPrefs, ObjectFilterVm,
};
use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

const NEW_GROUP: &str = "navigator-new-group-dialog";
const RENAME_GROUP: &str = "navigator-rename-group-dialog";

/// Host-owned projection of the dialog props. `object_filter.open` stands in
/// for React's `objectFilterConn != null` gate.
#[allow(dead_code)] // VM surface; owned by the navigator host.
#[derive(Clone, Debug, Default)]
pub(crate) struct NavigatorDialogsVm {
    pub(crate) new_group_open: bool,
    pub(crate) new_group_name: TextEdit,
    pub(crate) renaming_group: Option<String>,
    pub(crate) rename_value: TextEdit,
    pub(crate) object_filter: ObjectFilterVm,
}

/// The intents a host must apply. Text is already trimmed, as in React.
#[allow(dead_code)] // host port; remove when the navigator host lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NavigatorDialogsAction {
    CloseNewGroup,
    AddGroup(String),
    CloseRename,
    RenameGroup { old: String, new: String },
    CloseObjectFilter,
    /// React's `onSave`: `saveConnection(config)` then `refreshConnection(id)`.
    SaveObjectFilter(ObjectFilterPrefs),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, NavigatorDialogsAction) + 'static>;
type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>;
type FilterFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ObjectFilterVm>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: NavigatorDialogsAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

fn emit_many<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    actions: Vec<NavigatorDialogsAction>,
) -> impl Fn(&mut S) + Clone {
    footer_actions_helper(on_action, actions)
}

fn footer_actions_helper<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    actions: Vec<NavigatorDialogsAction>,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            for action in &actions {
                callback(s, action.clone());
            }
        }
    }
}

fn footer<S: 'static>(
    k: &rune_kit::Kit,
    key: &str,
    label: &str,
    on_action: &Option<ActionFn<S>>,
    action: NavigatorDialogsAction,
    primary: bool,
) -> El {
    footer_actions(k, key, label, on_action, vec![action], primary)
}

/// React's OK handlers perform two state changes (add/rename, then close), so
/// the button emits both in order onto the host port.
fn footer_actions<S: 'static>(
    k: &rune_kit::Kit,
    key: &str,
    label: &str,
    on_action: &Option<ActionFn<S>>,
    actions: Vec<NavigatorDialogsAction>,
    primary: bool,
) -> El {
    let callback = on_action.clone();
    Button::new(label)
        .variant(if primary { ButtonVariant::Contrast } else { ButtonVariant::Ghost })
        .control_size(ControlSize::Regular)
        .on_press(move |s: &mut S| {
            if let Some(callback) = &callback {
                for action in &actions {
                    callback(s, action.clone());
                }
            }
        })
        .build(k)
        .key(key)
}

/// Compose every mounted dialog into `layer`. Closed dialogs mount nothing.
#[allow(dead_code)] // consumer: the connection navigator host (not landed).
pub(crate) fn build<S: 'static>(
    vm: &NavigatorDialogsVm,
    new_group_edit: Option<EditFn<S>>,
    rename_edit: Option<EditFn<S>>,
    filter_vm: Option<FilterFn<S>>,
    on_action: Option<ActionFn<S>>,
    confirm_dialogs: &[El],
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);

    if vm.new_group_open {
        let name = vm.new_group_name.text.trim().to_owned();
        let mut field = TextField::new(format!("{NEW_GROUP}-input"), &vm.new_group_name)
            .height(32.0)
            .grow()
            .pad_x(8.0)
            .placeholder(t("main.groupNamePlaceholder"));
        if let Some(edit) = new_group_edit.clone() {
            let accessor = edit.clone();
            field = field.on_edit(move |s: &mut S| accessor(s));
        } else {
            field = field.disabled(true);
        }
        let submit_actions: Vec<NavigatorDialogsAction> = add_group_action(&name)
            .into_iter()
            .chain(std::iter::once(NavigatorDialogsAction::CloseNewGroup))
            .collect();
        let submit = emit_many(&on_action, submit_actions.clone());
        field = field.on_submit(move |s: &mut S, _tx: &mut rune::Tx| submit(s));
        Dialog::new(NEW_GROUP, t("common.newGroup"))
            .open(true)
            .fit()
            .width(360.0)
            .child(field.build(dark))
            .footer(footer(
                k,
                &format!("{NEW_GROUP}-cancel"),
                t("common.cancel"),
                &on_action,
                NavigatorDialogsAction::CloseNewGroup,
                false,
            ))
            .footer(footer_actions(
                k,
                &format!("{NEW_GROUP}-ok"),
                t("common.ok"),
                &on_action,
                add_group_action(&name)
                    .into_iter()
                    .chain(std::iter::once(NavigatorDialogsAction::CloseNewGroup))
                    .collect(),
                true,
            ))
            .on_close::<S>(emit(&on_action, NavigatorDialogsAction::CloseNewGroup))
            .build(k, layer);
    }

    if let Some(old) = vm.renaming_group.as_ref() {
        let new = vm.rename_value.text.trim().to_owned();
        let old = old.clone();
        let mut field = TextField::new(format!("{RENAME_GROUP}-input"), &vm.rename_value)
            .height(32.0)
            .grow()
            .pad_x(8.0)
            .placeholder(t("main.groupNamePlaceholder"));
        if let Some(edit) = rename_edit.clone() {
            let accessor = edit.clone();
            field = field.on_edit(move |s: &mut S| accessor(s));
        } else {
            field = field.disabled(true);
        }
        let submit_actions: Vec<NavigatorDialogsAction> = rename_group_action(&old, &new)
            .into_iter()
            .chain(std::iter::once(NavigatorDialogsAction::CloseRename))
            .collect();
        let submit = emit_many(&on_action, submit_actions.clone());
        field = field.on_submit(move |s: &mut S, _tx: &mut rune::Tx| submit(s));
        Dialog::new(RENAME_GROUP, t("main.ctx.renameGroup"))
            .open(true)
            .fit()
            .width(360.0)
            .child(field.build(dark))
            .footer(footer(
                k,
                &format!("{RENAME_GROUP}-cancel"),
                t("common.cancel"),
                &on_action,
                NavigatorDialogsAction::CloseRename,
                false,
            ))
            .footer(footer_actions(
                k,
                &format!("{RENAME_GROUP}-ok"),
                t("common.ok"),
                &on_action,
                rename_group_action(&old, &new)
                    .into_iter()
                    .chain(std::iter::once(NavigatorDialogsAction::CloseRename))
                    .collect(),
                true,
            ))
            .on_close::<S>(emit(&on_action, NavigatorDialogsAction::CloseRename))
            .build(k, layer);
    }

    if vm.object_filter.open {
        let mut dialog = ObjectFilterDialog::new(&vm.object_filter);
        if let Some(vm_edit) = filter_vm {
            dialog = dialog.on_vm(move |s: &mut S| vm_edit(s));
        }
        if let Some(callback) = on_action.clone() {
            dialog = dialog.on_action(move |s: &mut S, action| match action {
                ObjectFilterAction::Close => callback(s, NavigatorDialogsAction::CloseObjectFilter),
                ObjectFilterAction::Save(prefs) => {
                    callback(s, NavigatorDialogsAction::SaveObjectFilter(prefs))
                }
                // ToggleHide edits the VM's own slice; the host applies it.
                ObjectFilterAction::ToggleHide(_) => {}
            });
        }
        dialog.build(dark, layer);
    }

    for dialog in confirm_dialogs {
        layer.push(dialog.clone());
    }
}

/// React: `if (name.trim()) void addGroup(name.trim())` — empty names only close.
fn add_group_action(name: &str) -> Option<NavigatorDialogsAction> {
    if name.is_empty() {
        None
    } else {
        Some(NavigatorDialogsAction::AddGroup(name.to_owned()))
    }
}

/// React: the gate is `renamingGroup !== null && renameValue.trim()` — a
/// rename in progress with an empty old name (`Some("")`) still fires.
fn rename_group_action(old: &str, new: &str) -> Option<NavigatorDialogsAction> {
    if new.is_empty() {
        None
    } else {
        Some(NavigatorDialogsAction::RenameGroup {
            old: old.to_owned(),
            new: new.to_owned(),
        })
    }
}
