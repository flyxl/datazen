//! Create-database dialog, ported from
//! `packages/frontend/src/windows/connection/CreateDatabaseDialog.tsx` and its
//! shared `components/admin/AdminCreateDialog.tsx`.
//!
//! React's two create dialogs are thin wrappers that pin a command, title,
//! name label, placeholder and optional field labels. The Rune port keeps that
//! seam in [`AdminCreateSpec`] and shares the form state machine
//! ([`AdminCreateState`]) with the schema sibling. All async work is an
//! [`AdminCreateAction`] the host executes through its own port — no fake IPC.
//!
//! ## Local stand-ins (removal stage)
//!
//! `AdminSchemaFields` needs a driver command definition, which this view layer
//! has no port for; with `definition` undefined React renders no extra fields,
//! so this port renders none. Remove when that component is ported and thread
//! the definition in. `fieldLabels` keys are documented in the sibling port.
//!
//! ## Keys
//!
//! Preserves T223's slot keys: `content-view-create-database-dialog` with
//! `-body`, `-context`, `-cancel`, `-confirm` (plus the Dialog base's `-title`,
//! `-close`). Added: `-name`, `-name-label`, `-error`.

use std::rc::Rc;

use rune::{col, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// Which create dialog a shared action belongs to.
#[allow(dead_code)] // shared by both create dialogs; remove when the admin component lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CreateDialogKind {
    Database,
    Schema,
}

/// Host calls the form emits. `Execute` carries only the trimmed name; the host
/// owns `dbSessionId`, `command` and the optional target database.
#[allow(dead_code)] // host port; remove when the panel-store view track wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AdminCreateAction {
    Close,
    Execute { kind: CreateDialogKind, name: String },
    Created { kind: CreateDialogKind, name: String },
}

/// The three `useState` values of `AdminCreateDialog`.
#[allow(dead_code)] // shared form state; remove when the admin component lands.
#[derive(Clone, Debug, Default)]
pub(crate) struct AdminCreateState {
    pub name: TextEdit,
    pub running: bool,
    pub error: String,
}

impl AdminCreateState {
    /// React `resetForm`.
    #[allow(dead_code)] // consumer: the dialog host result feed (not landed); remove when it wires.
    pub(crate) fn reset(&mut self) {
        self.name = TextEdit::new(String::new());
        self.error.clear();
    }

    /// React `handleCreate` prologue: an empty name emits no command at all.
    pub(crate) fn begin(&mut self) -> Option<String> {
        let name = self.name.text.trim().to_owned();
        if name.is_empty() {
            return None;
        }
        self.running = true;
        self.error.clear();
        Some(name)
    }

    /// Resolve a host answer. Success resets the form and emits the post-success
    /// side effect plus close, in React's order; failure only sets the error.
    #[allow(dead_code)] // consumer: the dialog host result feed (not landed); remove when it wires.
    pub(crate) fn apply_result(
        &mut self,
        result: Result<(), String>,
        kind: CreateDialogKind,
    ) -> Vec<AdminCreateAction> {
        self.running = false;
        match result {
            Ok(()) => {
                let name = self.name.text.trim().to_owned();
                self.reset();
                vec![
                    AdminCreateAction::Created { kind, name: name.clone() },
                    AdminCreateAction::Close,
                ]
            }
            Err(message) => {
                self.error = message;
                Vec::new()
            }
        }
    }
}

/// The exact wrapper values from React `CreateDatabaseDialog`.
#[allow(dead_code)] // wrapper config; consumed by the slot composition.
pub(crate) const DB_SPEC: AdminCreateSpec = AdminCreateSpec {
    key: "content-view-create-database-dialog",
    title_key: "common.createDatabase",
    name_label_key: "createDb.name",
    placeholder: "my_database",
    kind: CreateDialogKind::Database,
};

/// The wrapper's configuration — the only difference between the two dialogs.
#[allow(dead_code)] // shared config; remove when the admin component lands.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AdminCreateSpec {
    pub key: &'static str,
    pub title_key: &'static str,
    pub name_label_key: &'static str,
    pub placeholder: &'static str,
    pub kind: CreateDialogKind,
}

type ActionFn<S> = Rc<dyn Fn(&mut S, AdminCreateAction) + 'static>;
type SubmitFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// Render the shared dialog shell. Both the editable (host-state) and snapshot
/// (composition) entry points converge here, so keys and callbacks stay 1:1.
fn render<S: 'static>(
    state: &AdminCreateState,
    spec: AdminCreateSpec,
    field_el: El,
    context: Option<&str>,
    submit: SubmitFn<S>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let close = {
        let on_action = on_action.clone();
        move |s: &mut S| {
            if let Some(callback) = &on_action {
                callback(s, AdminCreateAction::Close);
            }
        }
    };

    let mut dialog = Dialog::new(spec.key, t(spec.title_key))
        .open(true)
        .fit()
        .width(480.0)
        .body_max_height(280.0)
        .child(
            col()
                .key(format!("{}-body", spec.key))
                .w_full()
                .gap(12.0)
                .child(
                    text(t(spec.name_label_key))
                        .key(format!("{}-name-label", spec.key))
                        .wrap(),
                )
                .child(field_el)
                .child(
                    text(context.unwrap_or_else(|| t(spec.title_key)))
                        .key(format!("{}-context", spec.key))
                        .wrap(),
                ),
        );
    if !state.error.is_empty() {
        dialog = dialog.child(
            CopyableError::new(format!("{}-error", spec.key), state.error.clone()).build::<S>(k),
        );
    }
    dialog = dialog.footer(
        Button::new(t("common.cancel"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::ExtraLarge)
            .on_press(close.clone())
            .build(k)
            .key(format!("{}-cancel", spec.key)),
    );
    dialog = dialog.footer(
        Button::new(t(spec.title_key))
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::ExtraLarge)
            .disabled(state.name.text.trim().is_empty() || state.running)
            .on_press(move |s: &mut S| submit(s))
            .build(k)
            .key(format!("{}-confirm", spec.key)),
    );
    dialog.on_close::<S>(close).build(k, layer);
}

/// Editable entry point: the host owns the form state in `S`.
#[allow(dead_code)] // consumer: standalone dialog tests and the eventual host wiring.
pub(crate) fn build<S: 'static>(
    state: &AdminCreateState,
    spec: AdminCreateSpec,
    state_edit: impl Fn(&mut S) -> &mut AdminCreateState + 'static,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let state_edit: Rc<dyn Fn(&mut S) -> &mut AdminCreateState> = Rc::new(state_edit);
    let submit = {
        let state_edit = state_edit.clone();
        let on_action = on_action.clone();
        let kind = spec.kind;
        move |s: &mut S| {
            let name = {
                let form = state_edit(s);
                form.begin()
            };
            if let Some(name) = name {
                if let Some(callback) = &on_action {
                    callback(s, AdminCreateAction::Execute { kind, name });
                }
            }
        }
    };
    let field_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit> = {
        let state_edit = state_edit.clone();
        Rc::new(move |s: &mut S| &mut state_edit(s).name)
    };
    let mut field = TextField::new(format!("{}-name", spec.key), &state.name)
        .height(36.0)
        .placeholder(t(spec.placeholder))
        .label(t(spec.name_label_key))
        .on_edit(rune::edit_access(move |s: &mut S| field_edit(s)));
    let enter = submit.clone();
    field = field.on_submit(move |s: &mut S, _tx: &mut rune::Tx| enter(s));
    let field_el = field.build(dark);
    render::<S>(state, spec, field_el, None, Rc::new(submit), on_action, dark, layer);
}

/// Snapshot entry point for the T223 dialog slot: the host there has no form
/// state slice, so the field is a bound display of the supplied snapshot and
/// confirm still routes to the same action. Remove when the host owns the form.
#[allow(dead_code)] // consumer: content_view_dialogs slot; remove when the host wires real form state.
pub(crate) fn build_snapshot<S: 'static>(
    state: &AdminCreateState,
    spec: AdminCreateSpec,
    context: Option<&str>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let submit = {
        let on_action = on_action.clone();
        let kind = spec.kind;
        let name = state.name.text.trim().to_owned();
        move |s: &mut S| {
            if let Some(callback) = &on_action {
                callback(s, AdminCreateAction::Execute { kind, name: name.clone() });
            }
        }
    };
    // Without an edit accessor the base renders a selectable, unfocusable field
    // showing the host's current value — honest for a slot that owns no state.
    let field_el = TextField::new(format!("{}-name", spec.key), &state.name)
        .height(36.0)
        .placeholder(t(spec.placeholder))
        .label(t(spec.name_label_key))
        .disabled(true)
        .build(dark);
    render::<S>(state, spec, field_el, context, Rc::new(submit), on_action, dark, layer);
}
