//! Text fields: the component that owns a field's height, its centred line and
//! the chrome around it.
//!
//! `TextField` is the app's stable builder over `rune-kit::TextInput`: callers
//! keep their builder chains, and `build` delegates focus, hit-testing and the
//! single-line editor to the kit while applying the app's chrome overrides on
//! top. Rune lays a field's line out from the top of its content box, so a
//! fixed-height field centres its text by structure rather than a top offset;
//! the kit's outer row (`role("textbox")`, `h`, `center_y`) owns that, and
//! clicking anywhere in the box — including the vertical padding — routes focus
//! to the inner editor (probed in `tests::field_kit`).

use rune::tok::type_;
use rune::{Color, El, TextEdit, TextStyle};

use super::kit::kit;

/// Vertical padding that would centre one line of `style` inside a
/// `height`-tall box.
///
/// Kept for geometry tests. The live field centres through the kit's outer
/// row rather than by adding this padding to the editor itself.
pub(crate) fn centered_pad(style: TextStyle, height: f32) -> f32 {
    ((height - style.line_px()) / 2.0).max(0.0)
}

/// A single-line text field.
///
/// ```ignore
/// TextField::new("connection-name-input", &state.connection_name)
///     .height(36.0)
///     .pad_x(12.0)
///     .radius(8.0)
///     .fill(theme.card)
///     .border(1.0, theme.card_border)
///     .on_edit(|state: &mut AppState| &mut state.connection_name)
///     .build(dark)
/// ```
///
/// The kit supplies the default shell (fill, border, focus ring, hover); any
/// chrome a caller sets (`fill`/`border`/`radius`/`pad_x`/`ink`) is applied on
/// top of the built element so the caller's intent wins.
pub(crate) struct TextField<'a> {
    key: String,
    edit: &'a TextEdit,
    style: TextStyle,
    height: f32,
    width: Option<f32>,
    grow: bool,
    min_width: Option<f32>,
    shrink: bool,
    pad_x: f32,
    radius: Option<f32>,
    fill: Option<Color>,
    border: Option<(f32, Color)>,
    placeholder: Option<(String, Color)>,
    ink: Option<Color>,
    secure: bool,
    label: Option<String>,
    bound: bool,
    has_on_submit: bool,
    acts: Vec<Box<dyn FnOnce(El) -> El>>,
    on_edit_act: Option<Box<dyn FnOnce(rune_kit::TextInput) -> rune_kit::TextInput>>,
    on_submit_act: Option<Box<dyn FnOnce(rune_kit::TextInput) -> rune_kit::TextInput>>,
}

impl<'a> TextField<'a> {
    /// A field over `edit`, 28 points tall, set in the label type, filling the parent.
    pub(crate) fn new(key: impl Into<String>, edit: &'a TextEdit) -> TextField<'a> {
        TextField {
            key: key.into(),
            edit,
            style: type_::LABEL,
            height: 28.0,
            width: None,
            grow: false,
            min_width: None,
            shrink: false,
            pad_x: 0.0,
            radius: None,
            fill: None,
            border: None,
            placeholder: None,
            ink: None,
            secure: false,
            label: None,
            bound: false,
            has_on_submit: false,
            acts: Vec::new(),
            on_edit_act: None,
            on_submit_act: None,
        }
    }

    /// The text's type; the height keeps its line in the middle of it.
    pub(crate) fn text_style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// The box's height in points (default 28).
    pub(crate) fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// A fixed width in points; without it the field fills its parent.
    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Fill the parent's width (the default; spelled out where it matters).
    pub(crate) fn w_full(mut self) -> Self {
        self.width = None;
        self
    }

    /// Take the rest of the row, so a field among buttons shrinks with the window.
    pub(crate) fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// Let the field shrink below its width instead of pushing its row wider.
    pub(crate) fn min_width(mut self, width: f32) -> Self {
        self.min_width = Some(width);
        self
    }

    /// Allow the field to shrink when the row is tight.
    pub(crate) fn shrink(mut self) -> Self {
        self.shrink = true;
        self
    }

    /// Horizontal padding inside the box, for a field that draws its own border (default 0).
    pub(crate) fn pad_x(mut self, pad: f32) -> Self {
        self.pad_x = pad;
        self
    }

    pub(crate) fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    pub(crate) fn fill(mut self, fill: Color) -> Self {
        self.fill = Some(fill);
        self
    }

    pub(crate) fn border(mut self, width: f32, color: Color) -> Self {
        self.border = Some((width, color));
        self
    }

    /// Placeholder text and the colour it is drawn in, until the field has a value.
    pub(crate) fn placeholder(mut self, text: impl Into<String>, ink: Color) -> Self {
        self.placeholder = Some((text.into(), ink));
        self
    }

    /// The typed text's colour, in place of the runtime's default.
    pub(crate) fn ink(mut self, ink: Color) -> Self {
        self.ink = Some(ink);
        self
    }

    /// Protect the value like a password field.
    pub(crate) fn secure(mut self) -> Self {
        self.secure = true;
        self
    }

    /// The accessible name, when no visible label names the field.
    pub(crate) fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub(crate) fn on_edit<S: 'static>(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.bound = true;
        self.on_edit_act = Some(Box::new(move |e| e.on_edit(f)));
        self
    }

    pub(crate) fn on_submit<S: 'static>(mut self, f: impl Fn(&mut S, &mut rune::Tx) + 'static) -> Self {
        self.has_on_submit = true;
        self.on_submit_act = Some(Box::new(move |e| e.on_submit(f)));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        // Rune focuses a text box only when it can reach the `TextEdit` it
        // edits (`can_focus` asks the input for its accessor), and that comes
        // from `on_edit`: a field without it never takes a caret.
        debug_assert!(self.bound, "a text field without on_edit cannot take focus");
        let k = kit(dark);

        // A field embedded inside a caller-drawn container (nav search row,
        // home filter row) sets no chrome of its own; render the kit field
        // plain so it doesn't paint a second shell. Explicit chrome (fill /
        // border / radius) or an explicit transparent fill means the caller
        // wants to own the surface — plain keeps the kit's default shell out
        // of the way and we apply the requested chrome after.
        let embedded = self.border.is_none()
            && self.fill.as_ref().map_or(true, |c| c.a == 0.0);

        let mut ti = rune_kit::TextInput::new(self.edit)
            .text_style(self.style)
            .height(self.height)
            .width(self.width);
        if embedded {
            ti = ti.plain();
        }
        if let Some((text, _)) = &self.placeholder {
            ti = ti.placeholder(text.clone());
        }
        if self.secure {
            ti = ti.secure();
        }
        if let Some(label) = self.label {
            ti = ti.label(label);
        }
        if let Some(act) = self.on_edit_act {
            ti = act(ti);
        }
        if let Some(act) = self.on_submit_act {
            ti = act(ti);
        }
        let mut e = ti.build(k).key(self.key);

        // Kit gives a default shell; callers' explicit chrome wins. For a
        // plain/embedded field there is no shell to fight, so only the
        // properties the caller actually set are applied.
        if let Some(fill) = self.fill {
            e = e.fill(fill);
        }
        if let Some((width, color)) = self.border {
            e = e.border(width, color);
        }
        if let Some(radius) = self.radius {
            e = e.radius(radius);
        }
        if self.pad_x > 0.0 {
            e = e.px(self.pad_x);
        }
        if self.grow {
            e = e.grow(1.0).min_w(0.0);
        }
        if let Some(min) = self.min_width {
            e = e.min_w(min);
        }
        if self.shrink {
            e = e.shrink();
        }
        for act in self.acts {
            e = act(e);
        }
        e
    }
}
