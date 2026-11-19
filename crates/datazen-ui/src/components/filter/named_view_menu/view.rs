//! Rendering half of the `NamedViewMenu` port; see the parent module.

use std::rc::Rc;

use rune::{col, el, icon, row, text, El, Key, KeyEvent, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_DOWN, ICON_STAR, ICON_TRASH};
use crate::ui::field::TextField;
use crate::ui::kit::kit;

use super::{is_name_taken, normalize_view_name, scoped, MenuState, NamedView, NamedViewAction, NamedViewVm};

// ---- Rendering ---------------------------------------------------------------------------------

type ActionFn<S> = Rc<dyn Fn(&mut S, NamedViewAction)>;
type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>;

fn press<S: 'static>(f: &Option<ActionFn<S>>, a: NamedViewAction) -> impl Fn(&mut S) {
    let f = f.clone();
    move |s: &mut S| {
        if let Some(cb) = &f {
            cb(s, a.clone());
        }
    }
}

fn bind<S: 'static>(f: &Option<EditFn<S>>) -> Option<EditFn<S>> {
    f.clone()
}

/// Builder for the ported component. The host keeps [`NamedViewVm`] in `S`;
/// text drafts are bound through accessors so Rune can route keystrokes.
#[allow(dead_code)] // consumer: the filter table view (not landed).
pub(crate) struct NamedViewMenu<'a, S: 'static> {
    vm: &'a NamedViewVm,
    name_edit: Option<EditFn<S>>,
    rename_edit: Option<EditFn<S>>,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // consumer: the filter table view (not landed).
impl<'a, S: 'static> NamedViewMenu<'a, S> {
    pub(crate) fn new(vm: &'a NamedViewVm) -> Self {
        Self {
            vm,
            name_edit: None,
            rename_edit: None,
            on_action: None,
        }
    }

    pub(crate) fn on_name_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.name_edit = Some(Rc::new(rune::edit_access(f)));
        self
    }

    pub(crate) fn on_rename_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.rename_edit = Some(Rc::new(rune::edit_access(f)));
        self
    }

    pub(crate) fn on_action(mut self, f: impl Fn(&mut S, NamedViewAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let vm = self.vm;
        let trigger_label = t("filter.view.savedViews");
        let open = vm.state.is_open();

        let mut trigger = row()
            .key("named-view-trigger")
            .role("button")
            .h(28.0)
            .px(6.0)
            .gap(4.0)
            .center_y()
            .radius(6.0)
            .focusable(true)
            .expanded(open)
            .label(trigger_label)
            .help(trigger_label)
            .on_press(press(&self.on_action, NamedViewAction::Trigger))
            .child(icon(&ICON_STAR, 14.0).color(k.c.text_accent))
            .child(text(trigger_label).font(k.ty.text_xsmall).color(k.c.text_base))
            .child(icon(&ICON_CHEVRON_DOWN, 12.0).color(k.c.text_muted));

        let root = col()
            .key("named-view-menu")
            .w_full()
            .min_w(0.0)
            .gap(4.0)
            .child(trigger);

        if !open {
            return root;
        }

        let mut panel = col()
            .key("named-view-panel")
            .label(vm.state.as_str())
            .w(288.0)
            .min_w(0.0)
            .gap(6.0)
            .padding(8.0)
            .radius(8.0)
            .fill(k.c.surface_base)
            .border(1.0, k.c.border_base);

        match vm.state {
            MenuState::Loading => {
                panel = panel.child(
                    text(t("common.loading"))
                        .key("named-view-loading")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted)
                        .px(8.0)
                        .py(12.0),
                );
            }
            MenuState::Importing => {
                panel = panel.child(
                    text(t("filter.view.import"))
                        .key("named-view-importing")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted)
                        .px(8.0)
                        .py(12.0),
                );
            }
            MenuState::Error => {
                panel = panel.child(
                    text(vm.error.clone())
                        .key("named-view-error")
                        .font(k.ty.text_xsmall)
                        .color(k.c.danger.fg)
                        .min_w(0.0)
                        .wrap(),
                );
                panel = panel.child(kit_button(
                    k,
                    "named-view-retry",
                    t("common.retry"),
                    false,
                    &self.on_action,
                    NamedViewAction::Retry,
                ));
            }
            MenuState::Naming => {
                panel = panel.child(
                    text(t("filter.view.nameLabel"))
                        .key("named-view-name-label")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                );
                let mut name_field = TextField::new("named-view-name-input", &vm.name)
                    .height(28.0)
                    .pad_x(8.0)
                    .text_style(k.ty.text_xsmall)
                    .placeholder(t("filter.view.namePlaceholder"))
                    .disabled(self.name_edit.is_none());
                if let Some(edit) = bind(&self.name_edit) {
                    let commit = self.on_action.clone();
                    name_field = name_field.on_edit(rune::edit_access(move |s: &mut S| edit(s))).on_key(
                        move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                            if event.key == Key::Enter {
                                if let Some(cb) = &commit {
                                    cb(s, NamedViewAction::Save);
                                }
                                return true;
                            }
                            false
                        },
                    );
                }
                panel = panel.child(name_field.build(dark));
                let trimmed = normalize_view_name(&vm.name.text);
                if is_name_taken(vm, &trimmed) && !vm.name.text.trim().is_empty() {
                    panel = panel.child(
                        text(t("filter.view.nameExists"))
                            .key("named-view-name-taken")
                            .font(k.ty.text_xsmall)
                            .color(k.c.warning.fg),
                    );
                }
                if vm.dirty {
                    panel = panel.child(
                        text(t("filter.view.draftNotAppliedHint"))
                            .key("named-view-draft-hint")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .min_w(0.0)
                            .wrap(),
                    );
                    panel = panel.child(kit_button(
                        k,
                        "named-view-apply-first",
                        t("filter.view.applyAnyway"),
                        false,
                        &self.on_action,
                        NamedViewAction::ApplyFirstAndSave,
                    ));
                }
                let save_disabled = trimmed.is_empty() || is_name_taken(vm, &trimmed);
                panel = panel.child(
                    row()
                        .w_full()
                        .gap(6.0)
                        .center_y()
                        .child(kit_button(
                            k,
                            "named-view-cancel",
                            t("common.cancel"),
                            false,
                            &self.on_action,
                            NamedViewAction::CancelNaming,
                        ))
                        .child(kit_button(
                            k,
                            "named-view-save",
                            t("filter.view.save"),
                            save_disabled,
                            &self.on_action,
                            NamedViewAction::Save,
                        )),
                );
            }
            MenuState::List => {
                if !vm.notice.is_empty() {
                    panel = panel.child(
                        text(vm.notice.clone())
                            .key("named-view-notice")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .min_w(0.0)
                            .wrap(),
                    );
                }
                let views = scoped(vm);
                if views.is_empty() {
                    panel = panel.child(
                        text(t("filter.view.empty"))
                            .key("named-view-empty")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .px(8.0)
                            .py(12.0),
                    );
                } else {
                    panel = panel.child(el().key("named-view-item").a11y_hidden());
                    for (index, view) in views.iter().enumerate() {
                        panel = panel.child(view_row(
                            k,
                            dark,
                            index,
                            view,
                            vm.renaming_id.as_deref(),
                            &vm.rename_value,
                            bind(&self.rename_edit),
                            &self.on_action,
                        ));
                    }
                }
                panel = panel.child(
                    row()
                        .w_full()
                        .gap(6.0)
                        .center_y()
                        .child(kit_button(
                            k,
                            "named-view-save-as",
                            t("filter.view.saveAs"),
                            false,
                            &self.on_action,
                            NamedViewAction::SaveAs,
                        ))
                        .child(kit_button(
                            k,
                            "named-view-export",
                            t("filter.view.export"),
                            false,
                            &self.on_action,
                            NamedViewAction::Export,
                        ))
                        .child(kit_button(
                            k,
                            "named-view-import",
                            t("filter.view.import"),
                            false,
                            &self.on_action,
                            NamedViewAction::Import,
                        )),
                );
            }
            MenuState::Idle => {}
        }
        root.child(panel)
    }
}

fn kit_button<S: 'static>(
    k: &Kit,
    key: &str,
    label: &str,
    disabled: bool,
    on_action: &Option<ActionFn<S>>,
    action: NamedViewAction,
) -> El {
    let mut button = Button::new(label)
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Small)
        .disabled(disabled);
    if !disabled {
        button = button.on_press(press(on_action, action));
    }
    button.build(k).key(key)
}

#[allow(clippy::too_many_arguments)]
fn view_row<S: 'static>(
    k: &Kit,
    dark: bool,
    index: usize,
    view: &NamedView,
    renaming_id: Option<&str>,
    rename_value: &TextEdit,
    rename_edit: Option<EditFn<S>>,
    on_action: &Option<ActionFn<S>>,
) -> El {
    let row_key = format!("named-view-item-{}", view.id);
    if renaming_id == Some(view.id.as_str()) {
        let mut field = TextField::new("named-view-rename-input", rename_value)
            .height(24.0)
            .pad_x(4.0)
            .text_style(k.ty.text_xsmall)
            .disabled(rename_edit.is_none());
        if let Some(edit) = rename_edit {
            let commit = on_action.clone();
            let id = view.id.clone();
            field = field.on_edit(rune::edit_access(move |s: &mut S| edit(s))).on_key(
                move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| match event.key {
                    Key::Enter => {
                        if let Some(cb) = &commit {
                            cb(s, NamedViewAction::CommitRename(id.clone()));
                        }
                        true
                    }
                    Key::Escape => {
                        if let Some(cb) = &commit {
                            cb(s, NamedViewAction::CancelRename);
                        }
                        true
                    }
                    _ => false,
                },
            );
        }
        return row()
            .key(row_key)
            .w_full()
            .gap(4.0)
            .center_y()
            .child(field.build(dark));
    }

    let id = view.id.clone();
    let mut apply = row()
        .role("button")
        .key(format!("named-view-apply-{}", view.id))
        .focusable(true)
        .min_w(0.0)
        .grow(1.0)
        .label(view.name.clone())
        .child(text(view.name.clone()).font(k.ty.text_xsmall).color(k.c.text_base).truncate())
        .on_press(press(on_action, NamedViewAction::Apply(id.clone())));
    if view.is_default {
        apply = apply.child(
            text(t("filter.view.defaultBadge"))
                .key(format!("named-view-default-badge-{}", view.id))
                .font(k.ty.text_xsmall)
                .color(k.c.text_accent),
        );
    }
    let mut line = apply;
    if !view.is_default {
        line = line.child(kit_button(
            k,
            &format!("named-view-set-default-{}", view.id),
            t("filter.view.setDefault"),
            false,
            on_action,
            NamedViewAction::SetDefault(id.clone()),
        ));
    }
    line = line.child(kit_button(
        k,
        &format!("named-view-rename-{}", view.id),
        t("filter.view.rename"),
        false,
        on_action,
        NamedViewAction::StartRename(id.clone()),
    ));
    line = line.child(
        row()
            .role("button")
            .key(format!("named-view-delete-{}", view.id))
            .focusable(true)
            .label(t("filter.view.delete"))
            .help(t("filter.view.delete"))
            .child(icon(&ICON_TRASH, 12.0).color(k.c.danger.fg))
            .child(el().key(format!("named-view-delete-marker-{index}")).a11y_hidden())
            .on_press(press(on_action, NamedViewAction::Delete(id.clone()))),
    );
    row()
        .key(row_key)
        .w_full()
        .gap(4.0)
        .center_y()
        .child(line)
}

// Leftover L1 (removal stage): when the filter table view mounts this menu,
// keep the VM in its state slice, bind the two text accessors and translate the
// effects to the host's named-view commands. No IPC, timer or thread is created
// here. The export content is a deterministic stand-in until a Rust
// `buildNamedViewExport` lands; the confirm dialogs are effects for the host.
