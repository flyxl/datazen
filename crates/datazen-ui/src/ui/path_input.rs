//! Path picker input: a text field plus a ghost browse button.
//!
//! Owns the row layout, error border tinting, and the browse action wiring.
//! The edited value/state lives in the caller's [`rune::TextEdit`] and is
//! updated through `on_edit`, exactly as [`crate::ui::field::TextField`].
//!
//! A path input whose browse button cannot pick must not compile: `build` is
//! only available after [`PathInput::on_browse`] supplies the picker.
//!
//! ## Keys
//!
//! | part    | key             |
//! |---------|-----------------|
//! | wrapper | `{key}`         |
//! | input   | `{key}-input`   |
//! | browse  | `{key}-browse`  |

#![allow(dead_code)]

use std::marker::PhantomData;
use std::rc::Rc;

use rune::{row, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize, State};

use crate::icons::ICON_FOLDER_OPEN;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// Marker: the browse picker has not been provided yet.
pub(crate) struct NeedsBrowse;
/// Marker: the browse picker is set for state `S`.
pub(crate) struct HasBrowse<S: 'static>(PhantomData<S>);

type EditBind<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>;
type Browse<S> = Rc<dyn Fn(&mut S)>;

pub(crate) struct PathInput<'a, S: 'static, B> {
    key: String,
    edit: &'a TextEdit,
    placeholder: Option<String>,
    disabled: bool,
    error: bool,
    label: Option<String>,
    on_edit: Option<EditBind<S>>,
    on_browse: Option<Browse<S>>,
    _marker: PhantomData<B>,
}

impl<'a> PathInput<'a, (), NeedsBrowse> {
    /// A path input over `edit`. Call [`PathInput::on_browse`] before `build`.
    pub(crate) fn new(key: impl Into<String>, edit: &'a TextEdit) -> Self {
        Self {
            key: key.into(),
            edit,
            placeholder: None,
            disabled: false,
            error: false,
            label: None,
            on_edit: None,
            on_browse: None,
            _marker: PhantomData,
        }
    }
}

impl<'a, S: 'static, B> PathInput<'a, S, B> {
    pub(crate) fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn error(mut self, error: bool) -> Self {
        self.error = error;
        self
    }

    /// Accessible name for the browse control (and input label) when no visible
    /// label names them.
    pub(crate) fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the required browse picker. Without this, `build` is unavailable.
    pub(crate) fn on_browse<S2: 'static>(
        self,
        f: impl Fn(&mut S2) + 'static,
    ) -> PathInput<'a, S2, HasBrowse<S2>> {
        PathInput {
            key: self.key,
            edit: self.edit,
            placeholder: self.placeholder,
            disabled: self.disabled,
            error: self.error,
            label: self.label,
            // on_browse establishes the state type; bind on_edit after this.
            on_edit: None,
            on_browse: Some(Rc::new(f)),
            _marker: PhantomData,
        }
    }
}

impl<'a, S: 'static> PathInput<'a, S, HasBrowse<S>> {
    /// Bind the edited text into app state for the browse state's type.
    pub(crate) fn on_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_edit = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let c = &k.c;

        let fill = if self.error { c.danger.bg } else { c.surface_base };
        let mut field = TextField::new(format!("{}-input", self.key), self.edit)
            .height(32.0)
            .pad_x(10.0)
            .radius(6.0)
            .fill(fill)
            .grow()
            .min_width(0.0);

        let border_color = if self.error {
            c.danger.border
        } else {
            c.border_muted
        };
        field = field.border(1.0, border_color);

        if let Some(ph) = &self.placeholder {
            field = field.placeholder(ph.clone());
        }
        if let Some(label) = &self.label {
            field = field.label(label.clone());
        }

        let mut field_el = if self.disabled {
            // Render as a non-interactive display so it is unfocusable.
            let display = if self.edit.text.is_empty() {
                self.placeholder.clone().unwrap_or_default()
            } else {
                self.edit.text.clone()
            };
            let text_color = if self.edit.text.is_empty() { c.text_faint } else { c.text_base };
            let mut e = rune::row()
                .key(format!("{}-input", self.key))
                .h(32.0)
                .w_full()
                .grow(1.0)
                .min_w(0.0)
                .px(10.0)
                .radius(6.0)
                .center_y()
                .fill(fill)
                .border(1.0, border_color)
                .child(
                    rune::text(display)
                        .font(rune::tok::type_::LABEL)
                        .color(text_color),
                );
            if let Some(label) = &self.label {
                e = e.label(label.clone());
            }
            e.focusable(false).disabled(true)
        } else {
            if let Some(on_edit) = &self.on_edit {
                let on_edit = on_edit.clone();
                field = field.on_edit(move |s: &mut S| on_edit(s));
            }
            field.build(dark)
        };

        let browse_label = self
            .label
            .clone()
            .unwrap_or_else(|| "Browse".to_owned());
        let on_browse = self.on_browse.expect("browse handler set");

        let mut browse = Button::new("")
            .icon(&ICON_FOLDER_OPEN)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .help(browse_label);

        if self.disabled {
            browse = browse.state(State::Disabled).disabled(true);
        } else {
            let on_browse = on_browse.clone();
            browse = browse.on_press_tx(move |s: &mut S, _tx: &mut rune::Tx| {
                on_browse(s);
            });
        }

        let mut browse_el = browse.build(&k).key(format!("{}-browse", self.key));
        if self.disabled {
            browse_el = browse_el.focusable(false).disabled(true);
        }

        row()
            .key(self.key)
            .w_full()
            .gap(8.0)
            .center_y()
            .child(field_el)
            .child(browse_el)
    }
}