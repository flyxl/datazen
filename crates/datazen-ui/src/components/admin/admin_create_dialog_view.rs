//! `AdminCreateDialog`'s view — T013, the render half of the port.
//!
//! Ported from `packages/frontend/src/components/admin/AdminCreateDialog.tsx`
//! 94–131: the `max-w-md` dialog, the name field (with Enter-to-create and
//! `autoFocus`), the optional schema fields and the copyable error, with a
//! Cancel / primary footer. The state machine it renders lives in
//! [`super::admin_create_dialog`]; the caller owns the
//! [`AdminCreateDialogState`](super::admin_create_dialog::AdminCreateDialogState)
//! and passes `&state` each frame, and presses call back through the
//! `host_mut` accessor into the state machine, exactly as the T012 host
//! wires its children.
//!
//! # Key convention
//!
//! React gives this dialog **no** `data-testid` at all — the `Dialog`,
//! `Button`, `Input` and `CopyableError` children are un-keyed — so the
//! keys below are new stable names under one `admin-create-*` prefix,
//! chosen so the `CreateDatabaseDialog` / `CreateSchemaDialog` wiring task
//! and the tests can find every part:
//!
//! | part | key |
//! |---|---|
//! | panel (the `ui::Dialog` base) | `admin-create-dialog` (+ the base's `-title` / `-close` / `-body` / `-footer` / `-scrim` / `-layer` suffixes) |
//! | name label | `admin-create-name-label` |
//! | name input | `admin-create-name-input` |
//! | schema field `F` input | `admin-create-field-{F}` |
//! | schema field `F` label | `admin-create-field-{F}-label` |
//! | error (the `CopyableError` root) | `admin-create-error` (+ the base's `-text` / `-copy` suffixes) |
//! | cancel button | `admin-create-cancel` |
//! | primary button | `admin-create-submit` |
//! | running spinner inside the primary button | `admin-create-submit-spinner` (+ `-indicator`) |
//!
//! # Deliberate boundaries
//!
//! * **Panel width.** React's `max-w-md` is 448pt; the panel is fixed at
//!   that width with a `fit()` body capped at 400pt (the T012 body cap),
//!   so a long schema list scrolls instead of pushing the footer out.
//! * **Unwired dialog mounts nothing.** Without a `host_mut` accessor
//!   there is no `on_close` to give the `Dialog` base, and a dialog
//!   without one mounts nothing — so `build` returns early, exactly as
//!   the base does.
//! * **The primary button's spinner is a real `Spinner`.** Kit's
//!   `ButtonVariant::Loading` only dims the button and drops
//!   interactivity; it renders no spinner, so while `running` the label
//!   is built with `label_with` around a `Spinner` (`size="lg"` in React)
//!   plus the title text, and the button is `disabled` — matching
//!   React's `disabled={!name.trim() || running}`.

// The production consumer is the `CreateDatabaseDialog` /
// `CreateSchemaDialog` wiring task; until then the view is exercised by
// `tests/admin_create_dialog.rs` only.
#![allow(dead_code)]

use std::rc::Rc;

use rune::TextEdit;
use rune::{col, row, text, El, Key, KeyEvent, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use super::admin_create_dialog::AdminCreateDialogState;
use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};

/// Panel max width (React's `max-w-md`).
const PANEL_WIDTH: f32 = 448.0;
/// Body cap: the scrolling region's max height (the T012 body cap — React
/// has no cap here, but a long schema list must not push the footer out).
const BODY_MAX_HEIGHT: f32 = 400.0;
/// The name / schema inputs (React's default `Input` height).
const FIELD_HEIGHT: f32 = 36.0;

pub(crate) struct AdminCreateDialog<'a, S: 'static> {
    state: &'a AdminCreateDialogState,
    host: Option<Rc<dyn Fn(&mut S) -> &mut AdminCreateDialogState>>,
}

impl<'a, S: 'static> AdminCreateDialog<'a, S> {
    pub(crate) fn new(state: &'a AdminCreateDialogState) -> Self {
        Self { state, host: None }
    }

    /// The state accessor every button and field drives, e.g.
    /// `|state| &mut state.admin_create_dialog`.
    pub(crate) fn host_mut(
        mut self,
        f: impl Fn(&mut S) -> &mut AdminCreateDialogState + 'static,
    ) -> Self {
        self.host = Some(Rc::new(f));
        self
    }

    /// Renders the dialog into `layer` (the app's overlay layer, as
    /// `modal_layer` does for dialogs). Nothing is pushed when the dialog
    /// is closed — React's `Dialog open={open}` — or when no host
    /// accessor was given (see the module docs).
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        let state = self.state;
        if !state.is_open() {
            return;
        }
        let Some(host) = &self.host else {
            return;
        };
        let k = kit(dark);
        let title = t(&state.config().title_key).to_string();

        let mut body = col().w_full().gap(16.0).child(self.name_block(dark, host));
        for field in state.visible_fields() {
            body = body.child(self.schema_field(dark, host, field));
        }
        if let Some(error) = state.error() {
            let copy_host = Rc::clone(host);
            let copyable = CopyableError::new("admin-create-error", error)
                .copy_button(true)
                .copied(state.error_copied())
                .on_copy_tx(move |state: &mut S, _tx: &mut Tx| {
                    copy_host(state).set_error_copied(true);
                });
            body = body.child(copyable.build::<S>(k));
        }

        let close_host = Rc::clone(host);
        Dialog::new("admin-create-dialog", title.clone())
            .open(true)
            .open_epoch(state.open_epoch())
            .width(PANEL_WIDTH)
            .fit()
            .body_max_height(BODY_MAX_HEIGHT)
            .on_close(move |state: &mut S| close_host(state).close())
            .child(body)
            .footer(self.cancel_button(dark, host))
            .footer(self.submit_button(dark, host, &title))
            .build(k, layer);
    }

    /// The name block (`AdminCreateDialog.tsx:104–116`): the label from
    /// `nameLabelKey` and the input with `autoFocus` and Enter-to-create.
    fn name_block(
        &self,
        dark: bool,
        host: &Rc<dyn Fn(&mut S) -> &mut AdminCreateDialogState>,
    ) -> El {
        let k = kit(dark);
        let state = self.state;
        let edit_host = Rc::clone(host);
        let submit_host = Rc::clone(host);
        let focus_host = Rc::clone(host);
        // React `autoFocus`es the name input. An `El::autofocus` on the
        // field's outer row is ignored — the modal only honours the flag
        // on the focusable element itself, and the kit `TextInput` builds
        // its inner input where no caller can set it — so the first
        // layout after an open consumes the state's one-shot request and
        // focuses the field by key instead (see the state module docs).
        let field = TextField::new("admin-create-name-input", state.name_edit_ref())
            .height(FIELD_HEIGHT)
            .placeholder(state.config().name_placeholder.clone())
            .on_edit(move |state: &mut S| edit_host(state).name_edit())
            .on_submit(move |state: &mut S, _tx: &mut Tx| {
                submit_host(state).create();
            })
            .build(dark)
            .on_layout_tx(move |state: &mut S, _rect: [f32; 4], tx: &mut Tx| {
                if focus_host(state).take_focus_name_request() {
                    tx.focus("admin-create-name-input", true);
                }
            });
        col()
            .w_full()
            .gap(4.0)
            .child(
                text(t(&state.config().name_label_key))
                    .key("admin-create-name-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(field)
    }

    /// One optional schema field (`AdminSchemaFields.tsx:20–37` as
    /// rendered through this dialog): the label — the caller's
    /// `fieldLabels` override, else `fieldLabel` — and the input with the
    /// `fieldPlaceholder` example, if the driver sent one.
    fn schema_field(
        &self,
        dark: bool,
        host: &Rc<dyn Fn(&mut S) -> &mut AdminCreateDialogState>,
        field: &super::admin_create_dialog::AdminSchemaField,
    ) -> El {
        let k = kit(dark);
        let state = self.state;
        let edit_host = Rc::clone(host);
        let field_name = field.name.clone();
        let empty = TextEdit::new(String::new());
        let edit = state.optional_edit_ref(&field.name).unwrap_or(&empty);
        let mut input = TextField::new(format!("admin-create-field-{field_name}"), edit)
            .height(FIELD_HEIGHT)
            .on_edit(move |state: &mut S| edit_host(state).optional_edit(&field_name));
        if let Some(placeholder) = field.placeholder() {
            input = input.placeholder(placeholder.to_string());
        }
        col()
            .w_full()
            .gap(4.0)
            .child(
                text(state.field_label(field))
                    .key(format!("admin-create-field-{}-label", field.name))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(input.build(dark))
    }

    /// The Cancel footer button (`AdminCreateDialog.tsx:108–110`).
    fn cancel_button(
        &self,
        dark: bool,
        host: &Rc<dyn Fn(&mut S) -> &mut AdminCreateDialogState>,
    ) -> El {
        let k = kit(dark);
        let cancel_label = t("common.cancel").to_string();
        let mut cancel = Button::new(cancel_label.clone())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .help(cancel_label)
            .build(k)
            .key("admin-create-cancel");
        let press_host = Rc::clone(host);
        let key_host = Rc::clone(host);
        cancel = cancel
            .on_press(move |state: &mut S| press_host(state).close())
            .on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                if event.key == Key::Enter {
                    key_host(state).close();
                    true
                } else {
                    false
                }
            });
        cancel
    }

    /// The primary footer button (`AdminCreateDialog.tsx:111–119`): the
    /// title text, disabled on a blank name or while running, with the
    /// large spinner in front while running.
    fn submit_button(
        &self,
        dark: bool,
        host: &Rc<dyn Fn(&mut S) -> &mut AdminCreateDialogState>,
        title: &str,
    ) -> El {
        let k = kit(dark);
        let state = self.state;
        let press_host = Rc::clone(host);
        let mut submit = Button::new(title.to_string())
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Regular)
            .help(title.to_string())
            .disabled(!state.can_create());
        if state.running() {
            let title = title.to_string();
            submit = submit.label_with(move |font, ink| {
                row()
                    .gap(4.0)
                    .child(
                        Spinner::new("admin-create-submit-spinner")
                            .size(SpinnerSize::Lg)
                            .build(dark),
                    )
                    .child(text(title).font(font).color(ink))
            });
        }
        let mut submit = submit.build(k).key("admin-create-submit");
        let key_host = Rc::clone(host);
        submit = submit
            .on_press(move |state: &mut S| press_host(state).create())
            .on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                if event.key == Key::Enter {
                    key_host(state).create();
                    true
                } else {
                    false
                }
            });
        submit
    }
}
