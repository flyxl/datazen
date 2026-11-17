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
use rune::{text_input, Color, El, TextEdit, TextStyle};

use super::kit::kit;

/// Vertical padding that would centre one line of `style` inside a
/// `height`-tall box.
///
/// The kit-delegated field centres through its outer row; the legacy
/// (custom-ink) path uses this padding directly.
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
/// chrome a caller sets (`fill`/`border`/`radius`/`pad_x`) is applied on top
/// of the built element so the caller's intent wins. Placeholder colour and
/// the default text colour are decided by the kit; `.ink(...)` opts into the
/// direct-build path so the requested text colour reaches the editor.
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
    placeholder: Option<String>,
    ink: Option<Color>,
    secure: bool,
    label: Option<String>,
    bound: bool,
    on_edit_act: Option<Box<dyn FnOnce(rune_kit::TextInput) -> rune_kit::TextInput>>,
    on_submit_act: Option<Box<dyn FnOnce(rune_kit::TextInput) -> rune_kit::TextInput>>,
    on_key_act: Option<Box<dyn FnOnce(rune_kit::TextInput) -> rune_kit::TextInput>>,
    on_edit_el: Option<Box<dyn FnOnce(El) -> El>>,
    on_submit_el: Option<Box<dyn FnOnce(El) -> El>>,
    on_key_el: Option<Box<dyn FnOnce(El) -> El>>,
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
            on_edit_act: None,
            on_submit_act: None,
            on_key_act: None,
            on_edit_el: None,
            on_submit_el: None,
            on_key_el: None,
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

    /// Placeholder text shown until the field has a value. The colour is
    /// determined by the kit's theme tokens.
    pub(crate) fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    /// The typed text's colour, in place of the kit/theme default. Setting
    /// this opts into the direct-build path so the colour reaches the editor.
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
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
    ) -> Self {
        self.bound = true;
        let f1 = f.clone();
        self.on_edit_act = Some(Box::new(move |e| e.on_edit(f1)));
        self.on_edit_el = Some(Box::new(move |e: El| e.on_edit(f)));
        self
    }

    pub(crate) fn on_submit<S: 'static>(mut self, f: impl Fn(&mut S, &mut rune::Tx) + Clone + 'static) -> Self {
        let f1 = f.clone();
        self.on_submit_act = Some(Box::new(move |e| e.on_submit(f1)));
        self.on_submit_el = Some(Box::new(move |e: El| e.on_submit(f)));
        self
    }

    /// A key handler that runs while the field holds focus. Returning `true`
    /// consumes the key before Rune's focus traversal; this is how a field
    /// commits on `Tab` and moves focus itself.
    pub(crate) fn on_key<S: 'static>(
        mut self,
        f: impl Fn(&mut S, &rune::KeyEvent, &mut rune::Tx) -> bool + Clone + 'static,
    ) -> Self {
        let f1 = f.clone();
        self.on_key_act = Some(Box::new(move |e| e.on_key(f1)));
        self.on_key_el = Some(Box::new(move |e: El| e.on_key(f)));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        // Rune focuses a text box only when it can reach the `TextEdit` it
        // edits (`can_focus` asks the input for its accessor), and that comes
        // from `on_edit`: a field without it never takes a caret.
        debug_assert!(self.bound, "a text field without on_edit cannot take focus");

        // A caller-specified ink colour must reach the editor text; kit's
        // TextInput owns the inner element colour, so use the direct
        // text_input path in that case (it still fills the full box height).
        if self.ink.is_some() {
            return self.build_direct(dark);
        }

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
        if let Some(text) = &self.placeholder {
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
        if let Some(act) = self.on_key_act {
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
        e
    }

    /// Direct `text_input` build used when the caller needs control the kit
    /// cannot express (custom text/placeholder ink). Keeps the whole box as
    /// the hit target and exposes an inner "input" element so audits/tests can
    /// inspect the editor, mirroring the kit's structure.
    fn build_direct(self, dark: bool) -> El {
        let mut input = text_input(self.edit, self.style)
            .key("input")
            .grow(1.0)
            .shrink()
            .min_w(0.0)
            .single_line();
        if let Some(text) = self.placeholder {
            // No caller-specified placeholder colour in the kit-delegated
            // contract; use a theme-aware hint colour.
            let hint = if dark { Color::WHITE.alpha(0.45) } else { Color::BLACK.alpha(0.45) };
            input = input.placeholder(text, hint);
        }
        if let Some(ink) = self.ink {
            input = input.color(ink);
        }
        if self.secure {
            input = input.secure();
        }
        if let Some(label) = self.label {
            input = input.label(label);
        }
        if let Some(act) = self.on_edit_el {
            input = act(input);
        }
        if let Some(act) = self.on_submit_el {
            input = act(input);
        }
        if let Some(act) = self.on_key_el {
            input = act(input);
        }

        // Wrap in a full-height row that centres the editor line, so the whole
        // box remains a hit target (matching the kit's outer structure).
        use rune::row;
        let mut e = row()
            .key(self.key)
            .h(self.height)
            .center_y()
            .child(input);
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
        e
    }
}
