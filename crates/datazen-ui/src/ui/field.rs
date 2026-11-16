//! Text fields: the component that owns a field's height, its centred line and
//! the chrome around it.
//!
//! It follows the shape `rune-kit` uses for its components (`Button`, `TextInput`):
//! a builder whose methods set the look and the bindings, and whose `build`
//! paints the element. Rune lays a field's line out from the top of its content
//! box and its `center_y` only aligns children, so a fixed-height field built by
//! hand leaves the text and the caret pinned to the top; this component hands the
//! surplus height back as symmetric padding instead, which centres the line while
//! keeping the whole field clickable (a field squeezed to its line height would
//! leave only 15 points to hit).

use rune::tok::type_;
use rune::{text_input, Color, El, TextEdit, TextStyle};

/// Vertical padding that centres one line of `style` inside a `height`-tall field.
///
/// Exposed for the geometry test; fields themselves go through [`TextField`].
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
///     .build()
/// ```
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
    acts: Vec<Box<dyn FnOnce(El) -> El>>,
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
            acts: Vec::new(),
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
        self.acts.push(Box::new(move |e: El| e.on_edit(f)));
        self
    }

    pub(crate) fn on_submit<S: 'static>(mut self, f: impl Fn(&mut S, &mut rune::Tx) + 'static) -> Self {
        self.acts.push(Box::new(move |e: El| e.on_submit(f)));
        self
    }

    pub(crate) fn build(self) -> El {
        // Rune focuses a text box only when it can reach the `TextEdit` it
        // edits (`can_focus` asks the input for its accessor), and that comes
        // from `on_edit`: a field without it never takes a caret.
        debug_assert!(self.bound, "a text field without on_edit cannot take focus");
        let mut e = text_input(self.edit, self.style)
            .key(self.key)
            .h(self.height)
            .py(centered_pad(self.style, self.height))
            .single_line();
        match self.width {
            Some(width) => e = e.w(width),
            None => e = e.w_full(),
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
        if self.pad_x > 0.0 {
            e = e.px(self.pad_x);
        }
        if let Some(radius) = self.radius {
            e = e.radius(radius);
        }
        if let Some(fill) = self.fill {
            e = e.fill(fill);
        }
        if let Some((width, color)) = self.border {
            e = e.border(width, color);
        }
        if let Some((text, ink)) = self.placeholder {
            e = e.placeholder(text, ink);
        }
        if let Some(ink) = self.ink {
            e = e.color(ink);
        }
        if self.secure {
            e = e.secure();
        }
        if let Some(label) = self.label {
            e = e.label(label);
        }
        for act in self.acts {
            e = act(e);
        }
        e
    }
}
