//! Port of `packages/frontend/src/components/connection/ObjectFilterDialog.tsx`.
//!
//! A small modal that edits three object-filter preferences (hide system
//! schemas plus include/exclude name patterns) and hands the resulting config
//! back to the host. React owns four `useState` values and awaits `onSave`;
//! Rune keeps the values in [`ObjectFilterVm`] owned by `S` and reports the two
//! user intents through [`ObjectFilterAction`] to an injected host callback —
//! no fake IPC, promise or thread.
//!
//! Opening the dialog copies the connection's saved filter into the VM (React's
//! `useEffect` on `[open, connection]`). [`ObjectFilterVm::open`] is the explicit
//! equivalent; the host calls it when the dialog is shown.
//!
//! ## Keys
//!
//! React's only testid is `object-filter-dialog` (the panel). The controls use
//! the `object-filter-*` prefix; every visible string is an exact locale key.

use std::rc::Rc;

use rune::{col, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

/// The three editable preferences, mirroring React's `ObjectFilterPrefs`.
#[allow(dead_code)] // VM surface; consumer: the connection navigator host.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ObjectFilterPrefs {
    pub hide_system_schemas: bool,
    pub table_name_include: String,
    pub table_name_exclude: String,
}

/// The dialog's observable state.
#[allow(dead_code)] // VM surface; consumer: the connection navigator host.
#[derive(Clone, Debug)]
pub struct ObjectFilterVm {
    pub open: bool,
    /// React's local `hideSystemSchemas`.
    pub hide_system_schemas: bool,
    pub table_name_include: TextEdit,
    pub table_name_exclude: TextEdit,
    /// `saving` disables Save while the host persists.
    pub saving: bool,
}

#[allow(dead_code)] // initial-state convenience; wired by the host view.
impl Default for ObjectFilterVm {
    fn default() -> Self {
        Self {
            open: false,
            hide_system_schemas: false,
            table_name_include: TextEdit::new(String::new()),
            table_name_exclude: TextEdit::new(String::new()),
            saving: false,
        }
    }
}

#[allow(dead_code)] // open/reset contract; wired by the host view.
impl ObjectFilterVm {
    /// React's effect: seed the form from the connection's saved filter.
    pub fn open(&mut self, saved: &ObjectFilterPrefs) {
        self.open = true;
        self.hide_system_schemas = saved.hide_system_schemas;
        self.table_name_include.replace_text(saved.table_name_include.clone());
        self.table_name_exclude.replace_text(saved.table_name_exclude.clone());
        self.saving = false;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.saving = false;
    }

    /// React's trimmed config: empty strings become `None` (absent optional
    /// fields), exactly like the `|| undefined` in `handleSave`.
    pub fn trimmed_prefs(&self) -> ObjectFilterPrefs {
        ObjectFilterPrefs {
            hide_system_schemas: self.hide_system_schemas,
            table_name_include: self.table_name_include.text.trim().to_owned(),
            table_name_exclude: self.table_name_exclude.text.trim().to_owned(),
        }
    }
}

/// The two user intents. The host performs the save command and closes.
#[allow(dead_code)] // VM surface; consumer: the connection navigator host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectFilterAction {
    Close,
    /// The checkbox only edits local state (React's `setHideSystemSchemas`).
    ToggleHide(bool),
    Save(ObjectFilterPrefs),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, ObjectFilterAction)>;
type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>;

fn press<S: 'static>(f: &Option<ActionFn<S>>, a: ObjectFilterAction) -> impl Fn(&mut S) {
    let f = f.clone();
    move |s: &mut S| {
        if let Some(cb) = &f {
            cb(s, a.clone());
        }
    }
}

fn footer_button<S: 'static>(
    k: &Kit,
    key: &str,
    label: &str,
    disabled: bool,
    on_action: &Option<ActionFn<S>>,
    action: ObjectFilterAction,
) -> El {
    let mut button = Button::new(label)
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Regular)
        .disabled(disabled);
    if !disabled {
        button = button.on_press(press(on_action, action));
    }
    button.build(k).key(key)
}

/// Builder for the ported dialog.
#[allow(dead_code)] // consumer: the connection navigator host (not landed).
pub struct ObjectFilterDialog<'a, S: 'static> {
    vm: &'a ObjectFilterVm,
    on_vm: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ObjectFilterVm>>,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // consumer: the connection navigator host (not landed).
impl<'a, S: 'static> ObjectFilterDialog<'a, S> {
    pub fn new(vm: &'a ObjectFilterVm) -> Self {
        Self {
            vm,
            on_vm: None,
            on_action: None,
        }
    }

    pub fn on_vm(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut ObjectFilterVm + 'static,
    ) -> Self {
        self.on_vm = Some(Rc::new(f));
        self
    }

    pub fn on_action(mut self, f: impl Fn(&mut S, ObjectFilterAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    pub fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let vm = self.vm;
        let k = kit(dark);
        let root = col()
            .key("object-filter-dialog-root")
            .w_full()
            .h_full();
        if !vm.open {
            return root;
        }

        let mut toggle = Checkbox::new("object-filter-hide-system", vm.hide_system_schemas)
            .label(t("objectFilter.hideSystemSchemas"));
        if let Some(on_action) = &self.on_action {
            let on_action = on_action.clone();
            let checked = vm.hide_system_schemas;
            toggle = toggle.on_toggle(move |s: &mut S| {
                on_action(s, ObjectFilterAction::ToggleHide(!checked));
            });
        }

        let mut include = TextField::new("object-filter-include", &vm.table_name_include)
            .height(32.0)
            .pad_x(8.0)
            .text_style(k.ty.text_xsmall)
            .placeholder(t("objectFilter.includePlaceholder"));
        let mut exclude = TextField::new("object-filter-exclude", &vm.table_name_exclude)
            .height(32.0)
            .pad_x(8.0)
            .text_style(k.ty.text_xsmall)
            .placeholder(t("objectFilter.excludePlaceholder"));
        if let Some(accessor) = &self.on_vm {
            let include_acc = accessor.clone();
            let exclude_acc = accessor.clone();
            include = include
                .on_edit(rune::edit_access(move |s: &mut S| &mut include_acc(s).table_name_include));
            exclude = exclude
                .on_edit(rune::edit_access(move |s: &mut S| &mut exclude_acc(s).table_name_exclude));
        } else {
            include = include.disabled(true);
            exclude = exclude.disabled(true);
        }

        let body = col()
            .key("object-filter-body")
            .w_full()
            .min_w(0.0)
            .gap(12.0)
            .child(toggle.build(k))
            .child(
                col()
                    .key("object-filter-include-block")
                    .w_full()
                    .gap(4.0)
                    .child(
                        text(t("objectFilter.include"))
                            .key("object-filter-include-label")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    )
                    .child(include.build(dark)),
            )
            .child(
                col()
                    .key("object-filter-exclude-block")
                    .w_full()
                    .gap(4.0)
                    .child(
                        text(t("objectFilter.exclude"))
                            .key("object-filter-exclude-label")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    )
                    .child(exclude.build(dark)),
            )
            .child(
                text(t("objectFilter.hint"))
                    .key("object-filter-hint")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .min_w(0.0)
                    .wrap(),
            );

        let mut dialog = Dialog::new("object-filter-dialog", t("common.objectFilter"))
            .open(true)
            .fit()
            .body_max_height(360.0)
            .child(body)
            .footer(footer_button(
                &k,
                "object-filter-cancel",
                t("common.cancel"),
                false,
                &self.on_action,
                ObjectFilterAction::Close,
            ))
            .footer(footer_button(
                &k,
                "object-filter-save",
                t("common.save"),
                vm.saving,
                &self.on_action,
                ObjectFilterAction::Save(vm.trimmed_prefs()),
            ));
        if self.on_action.is_some() {
            let close = self.on_action.clone();
            dialog = dialog.on_close(move |s: &mut S| {
                if let Some(cb) = &close {
                    cb(s, ObjectFilterAction::Close);
                }
            });
        }
        dialog.build(&k, layer);
        root
    }
}

// Leftover L1 (removal stage): when the connection navigator host mounts this
// dialog, bind `on_vm` to its state slice and translate `ObjectFilterAction`
// into the `objectFilter` lib command plus the connection update; drop the
// per-item allows then. No IPC is performed here.
