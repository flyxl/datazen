//! Port of `packages/frontend/src/components/connection/ConnectionImportFields.tsx`.
//!
//! The shared body of the connection-import flow: the format hint (file flow),
//! the app-flow data-path field with its browse-folder / browse-file buttons,
//! the chosen-file row with its change button, the password field (optional for
//! plain-text sources) and the inline error banner. The `dialog` and `inline`
//! variants only change spacing, which is kept as a gap/font flag.
//!
//! ## Structural boundary (later tasks)
//!
//! React's `ConnectionImportState` (`useConnectionImport`) and the two `lib`
//! helpers are separate tasks and are not imported here:
//!
//! - `ConnectionImportState` → [`ImportVm`], a cloneable projection the owner
//!   keeps in its `S`. Text inputs bind through
//!   [`ConnectionImportFields::on_vm`]; button presses go through
//!   [`ConnectionImportFields::on_action`]. No fake IPC command is invented —
//!   the host delivers results by applying changes to its own state.
//! - `importFileDisplayName` / `importFilePasswordPolicy` → local pure helpers
//!   [`import_file_display_name`] / [`import_file_password_policy`]. Removal
//!   stage: when `lib/importConnectionFile.ts` is ported, import them and delete
//!   the local copies.
//! - `Input` → [`TextField`]; `Button` → kit [`Button`].
//!
//! React `data-testid` values are kept verbatim (`import-data-path`,
//! `import-selected-file`, `connection-import-error`). The password field uses
//! the existing Rune key `import-password-input` so a later view binding can
//! reuse it. All colours/types/radii come from `kit(dark)`.

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// React `importFileDisplayName`: the last `/` or `\` segment of a path.
#[allow(dead_code)] // reusable pure helper; consumed by build and the tests
pub(crate) fn import_file_display_name(path: &str) -> &str {
    for (i, c) in path.char_indices().rev() {
        if c == '/' || c == '\\' {
            return &path[i + 1..];
        }
    }
    path
}

/// React `importFilePasswordPolicy`: encrypted container extensions need a
/// password, everything else leaves it optional.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable VM surface; consumed by the tests
pub(crate) enum PasswordPolicy {
    Required,
    Optional,
}

#[allow(dead_code)] // reusable pure helper; consumed by build and the tests
impl PasswordPolicy {
    pub(crate) fn from_path(path: &str) -> Self {
        let base = import_file_display_name(path);
        let ext = match base.rfind('.') {
            Some(i) => base[i + 1..].to_ascii_lowercase(),
            None => String::new(),
        };
        if ext == "datazenconnection"
            || ext == "datazenconnections"
            || ext == "tableplusconnection"
        {
            PasswordPolicy::Required
        } else {
            PasswordPolicy::Optional
        }
    }
}

/// Which browse button was pressed (React `browsePath('folder' | 'file')`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // public VM surface; wired by later callers/tests
pub(crate) enum BrowseKind {
    Folder,
    File,
}

/// The view model: a cloneable projection of `ConnectionImportState` plus the
/// `variant` prop. The owner keeps the real values in its `S`.
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable VM surface; wired by later callers/tests
pub(crate) struct ImportVm {
    pub password: TextEdit,
    pub data_path: TextEdit,
    pub path_found: bool,
    /// React `source !== 'file'` (app import vs the two-step file flow).
    pub app_import: bool,
    /// `submitting || detecting`: disables the data-path field and browse buttons.
    pub busy: bool,
    /// Disables the change-file button and the password field.
    pub submitting: bool,
    pub local_error: String,
    pub selected_file: Option<String>,
    /// `importFilePasswordPolicy(selectedFile) === 'required'`.
    pub password_required: bool,
    /// React `variant === 'inline'`; only spacing changes.
    pub inline: bool,
}

#[allow(dead_code)] // initial-state convenience; wired by later callers/tests
impl Default for ImportVm {
    fn default() -> Self {
        ImportVm {
            password: TextEdit::new(String::new()),
            data_path: TextEdit::new(String::new()),
            path_found: false,
            app_import: true,
            busy: false,
            submitting: false,
            local_error: String::new(),
            selected_file: None,
            password_required: false,
            inline: false,
        }
    }
}

/// Discrete interactions the widget asks the host to apply to `S`. Text input
/// is routed through the [`ConnectionImportFields::on_vm`] accessor.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // public VM surface; wired by later callers/tests
pub(crate) enum ImportAction {
    BrowsePath(BrowseKind),
    PickImportFile,
}

type VmFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ImportVm + 'static>;
type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;
type ActionFn<S> = Rc<dyn Fn(&mut S, ImportAction) + 'static>;

fn field_edit<S: 'static>(
    vm: Option<&VmFn<S>>,
    pick: for<'a> fn(&'a mut ImportVm) -> &'a mut TextEdit,
) -> Option<EditFn<S>> {
    vm.map(|vm| {
        let vm = vm.clone();
        Rc::new(rune::edit_access(move |s: &mut S| pick(vm(s)))) as EditFn<S>
    })
}

fn press<S: 'static>(f: &Option<ActionFn<S>>, a: ImportAction) -> impl Fn(&mut S) {
    let f = f.clone();
    move |s: &mut S| {
        if let Some(cb) = &f {
            cb(s, a.clone());
        }
    }
}

/// The secondary buttons React renders with `variant="secondary"`. kit's
/// `Neutral` is the matching filled-secondary treatment.
fn secondary_button<S: 'static>(
    key: &str,
    label: String,
    disabled: bool,
    on_action: &Option<ActionFn<S>>,
    a: ImportAction,
    k: &rune_kit::Kit,
) -> El {
    let mut button = Button::new(label)
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Small)
        .disabled(disabled);
    if !disabled {
        button = button.on_press(press(on_action, a));
    }
    button.build(k).key(key)
}

/// Builder for the ported component.
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct ConnectionImportFields<S: 'static> {
    vm: ImportVm,
    on_vm: Option<VmFn<S>>,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl<S: 'static> ConnectionImportFields<S> {
    pub(crate) fn new(vm: ImportVm) -> Self {
        ConnectionImportFields {
            vm,
            on_vm: None,
            on_action: None,
        }
    }

    /// Accessor for the mutable VM in `S`. Without it the two password/path
    /// fields are inert (honest stand-ins, never focusable dead controls).
    pub(crate) fn on_vm(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut ImportVm + 'static,
    ) -> Self {
        self.on_vm = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_action(mut self, f: impl Fn(&mut S, ImportAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    /// Renders the import body. `dark` selects the kit palette.
    pub(crate) fn build(self, dark: bool) -> El {
        let ConnectionImportFields { vm, on_vm, on_action } = self;
        let k = kit(dark);
        // React: `inline ? 'space-y-2.5' : 'space-y-2'` and a 12.5px/12px label.
        // kit has no 12.5pt type token, so both label sizes map to `text_xsmall`;
        // only the field gap differs observably.
        let field_gap = if vm.inline { 10.0 } else { 8.0 };
        let label_font = k.ty.text_xsmall;
        let data_edit = field_edit(on_vm.as_ref(), |vm| &mut vm.data_path);
        let password_edit = field_edit(on_vm.as_ref(), |vm| &mut vm.password);

        // React line 32: `space-y-4` on the outer container.
        let mut root = col()
            .key("connection-import-fields")
            .w_full()
            .min_w(0.0)
            .gap(16.0);

        // React lines 33-35: the file-flow format hint (app flow hides it).
        if !vm.app_import {
            root = root.child(
                text(t("connShare.importFormatsHint"))
                    .key("import-formats-hint")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .min_w(0.0)
                    .wrap(),
            );
        }

        // React lines 37-67: the app-flow data path block.
        if vm.app_import {
            let hint = if vm.path_found {
                t("connShare.dataPathFoundHint")
            } else {
                t("connShare.dataPathMissingHint")
            };
            let mut block = col().key("import-data-path-block").w_full().gap(field_gap);
            block = block.child(
                text(hint)
                    .key("import-data-path-hint")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .min_w(0.0)
                    .wrap(),
            );
            block = block.child(
                text(t("connShare.dataPath"))
                    .key("import-data-path-label")
                    .font(label_font)
                    .color(k.c.text_muted),
            );

            let mut path_field = TextField::new("import-data-path", &vm.data_path)
                .height(32.0)
                .pad_x(10.0)
                .radius(k.radius.sm)
                .fill(k.c.surface_base)
                .border(1.0, k.c.border_base)
                .placeholder(t("connShare.dataPathPlaceholder"));
            // Use the component's own disabled state: it skips the `on_edit`
            // binding, so a busy field genuinely cannot take a caret (an
            // outer-only `El::disabled` would leave the kit's inner editor live).
            if vm.busy {
                path_field = path_field.disabled(true);
            } else {
                match data_edit.as_ref() {
                    Some(edit) => {
                        let edit = edit.clone();
                        path_field = path_field.on_edit(rune::edit_access(move |s: &mut S| edit(s)));
                    }
                    None => path_field = path_field.disabled(true),
                }
            }
            block = block.child(path_field.build(dark));

            let buttons = row()
                .key("import-browse-row")
                .w_full()
                .gap(8.0)
                .flex_wrap()
                .child(secondary_button(
                    "import-browse-folder",
                    t("connShare.browseFolder").to_owned(),
                    vm.busy,
                    &on_action,
                    ImportAction::BrowsePath(BrowseKind::Folder),
                    k,
                ))
                .child(secondary_button(
                    "import-browse-file",
                    t("connShare.browseFile").to_owned(),
                    vm.busy,
                    &on_action,
                    ImportAction::BrowsePath(BrowseKind::File),
                    k,
                ));
            root = root.child(block).child(buttons);
        }

        // React lines 69-89: the chosen-file row (file flow only).
        if !vm.app_import {
            if let Some(path) = &vm.selected_file {
                let mut block = col()
                    .key("import-selected-file-block")
                    .w_full()
                    .gap(field_gap);
                block = block.child(
                    text(t("connShare.selectedImportFile"))
                        .key("import-selected-file-label")
                        .font(label_font)
                        .color(k.c.text_muted),
                );
                block = block.child(
                    row()
                        .key("import-selected-file-row")
                        .w_full()
                        .min_w(0.0)
                        .items_center()
                        .gap(8.0)
                        .flex_wrap()
                        .child(
                            text(import_file_display_name(path))
                                .key("import-selected-file")
                                .help(path.clone())
                                .font(k.ty.text_small)
                                .color(k.c.text_base)
                                .grow(1.0)
                                .shrink()
                                .min_w(0.0),
                        )
                        .child(secondary_button(
                            "import-change-file",
                            t("connShare.changeImportFile").to_owned(),
                            vm.submitting,
                            &on_action,
                            ImportAction::PickImportFile,
                            k,
                        )),
                );
                root = root.child(block);
            }
        }

        // React lines 91-110: the password field (app flow or a chosen file).
        if vm.app_import || vm.selected_file.is_some() {
            let mut label = row()
                .key("import-password-label")
                .w_full()
                .items_center()
                .gap(4.0)
                .child(
                    text(t("connShare.password"))
                        .font(label_font)
                        .color(k.c.text_muted),
                );
            if !vm.password_required {
                label = label.child(
                    text(format!("({})", t("connShare.passwordOptional")))
                        .key("import-password-optional")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_faint),
                );
            }
            let mut pf = TextField::new("import-password-input", &vm.password)
                .height(32.0)
                .pad_x(10.0)
                .radius(k.radius.sm)
                .fill(k.c.surface_base)
                .border(1.0, k.c.border_base)
                .secure()
                .placeholder(t("connShare.passwordImportPlaceholder"));
            if vm.submitting {
                pf = pf.disabled(true);
            } else {
                match password_edit.as_ref() {
                    Some(edit) => {
                        let edit = edit.clone();
                        pf = pf.on_edit(rune::edit_access(move |s: &mut S| edit(s)));
                    }
                    None => pf = pf.disabled(true),
                }
            }
            root = root.child(
                col()
                    .key("import-password-block")
                    .w_full()
                    .gap(field_gap)
                    .child(label)
                    .child(pf.build(dark)),
            );
        }

        // React lines 112-119: the inline error banner.
        if !vm.local_error.is_empty() {
            root = root.child(
                col()
                    .key("connection-import-error")
                    .role("alert")
                    .w_full()
                    .min_w(0.0)
                    .radius(k.radius.md)
                    .fill(k.alpha(k.c.danger.fg, 0.10))
                    .border(1.0, k.alpha(k.c.danger.fg, 0.20))
                    .padding(8.0)
                    .child(
                        text(vm.local_error.clone())
                            .key("connection-import-error-text")
                            .font(k.ty.text_xsmall)
                            .color(k.c.danger.fg)
                            .min_w(0.0)
                            .wrap(),
                    ),
            );
        }

        root
    }
}

// Leftover L1 (removal stage: when `views/modals.rs` / the onboarding inline
// import step mount this port): keep `ImportVm` in the owning `AppState`, bind
// `on_vm` to that slice, and translate `ImportAction` into host adapter calls
// (the import commands are a later task). No fake command is invented here.
