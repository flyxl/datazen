//! The modal dialog base: scrim, centered panel, header (title + close), a
//! scrolling body and a footer slot pinned to the panel's bottom edge.
//!
//! `rune_kit::Dialog` owns the scrim, the panel and the modal policy, but it
//! builds its parts with private keys (`close`, `header`, `body`,
//! `<key>-dialog`), has no footer slot and no scrollable body, so a wrapper
//! cannot keep DataZen's key contract (`connection-dialog-title`,
//! `import-dialog-close`, ...) on top of it. This base therefore mirrors
//! `rune-kit`'s build on the same tokens and controls — palette, type,
//! elevation, `IconButton` — and adds the missing slots under one naming rule:
//!
//! | part   | key                 |
//! |--------|---------------------|
//! | panel  | `<key>`             |
//! | header | `<key>-header`      |
//! | title  | `<key>-title`       |
//! | close  | `<key>-close`       |
//! | body   | `<key>-body`        |
//! | footer | `<key>-footer`      |
//! | scrim  | `<key>-scrim`       |
//! | layer  | `<key>-layer`       |
//!
//! Escape and the focus trap stay Rune's: [`El::modal`] puts the panel on the
//! modal stack, Escape runs its dismiss closure (key repeat is ignored), and
//! closing hands focus back to where it was before the dialog opened
//! (`runtime/input/modal_focus.rs`). A dialog without `on_close` mounts
//! nothing, exactly as `rune_kit::Dialog` does.
//!
//! ## Reopening starts at the top
//!
//! Rune keeps a scroll viewport's offset per element id, and a dialog that
//! unmounts and mounts again at the same path gets its old id back. A body the
//! user scrolled before Escape would therefore reopen at that offset, which
//! React's fresh mount never did. [`Dialog::open_epoch`] stamps the body's
//! wrapper with a new key on each open, so the body is a new element (and a new
//! scroll viewport, offset 0) while the documented `<key>-body` key stays the
//! same for lookups and tests.
//!
//! ## Enter does not activate controls (evaluated, not enabled here)
//!
//! Rune presses a focused pressable on Return only when the app turns on
//! `Cx::web_focus()` (`runtime/input/focus.rs`): without it `takes_return` is
//! false and only Rune's own `El::default_button` reacts to Enter. DataZen
//! never calls it, so today Enter does nothing on a focused button, Space
//! presses it, and a dialog's Tab order starts at its first focusable control
//! (the close button). `web_focus()` is app-wide input semantics — every
//! pressable becomes a Tab stop and takes focus on click — so this track
//! records the gap instead of switching it on for the whole port; it belongs to
//! a keyboard-navigation task with its own full-suite pass. A trial on this
//! branch (setting it in `app::view`, then reverted) passed 211 of 213 tests:
//! only the Tab-travel assumptions moved — five tabs then land on
//! `rail-workspace` instead of `connection-new`, and a dialog opens focused on
//! its first control (`connection-name-input`) instead of its close button.

use rune::{col, row, text, Color, El, ScrollSpec, TextStyle};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Kit};

/// Panel corner radius (React's `rounded-xl`).
const PANEL_RADIUS: f32 = 12.0;
/// Height of the standard panel when the caller picks no size (kit's 368pt
/// default, for short confirmation dialogs).
const PANEL_HEIGHT: f32 = 368.0;
/// A footer's default height: a 32pt button inside React's `py-3`.
const FOOTER_HEIGHT: f32 = 56.0;
/// Space between the centered panel and the window edge (React's `p-4`).
const LAYER_PAD: f32 = 16.0;
/// The scrim's opacity: React's `bg-black/50` (`packages/ui/src/Dialog.tsx`)
/// and the pre-port `scrim()` in `views/modals.rs` (`oklcha(0.0, 0.0, 256.0,
/// 0.5)`). The kit palette's scrim is 30% (`rune_kit::palette`), which dims the
/// page less than the React dialog it replaces.
const SCRIM_ALPHA: f32 = 0.5;

/// A dialog panel plus the scrim under it, pushed into the app's layer.
pub(crate) struct Dialog {
    key: String,
    title: String,
    description: Option<String>,
    open: bool,
    open_epoch: u64,
    width: f32,
    fit: bool,
    fill: bool,
    height: Option<f32>,
    max_height: Option<f32>,
    body_max_height: Option<f32>,
    click_outside: bool,
    header_pad: (f32, f32),
    body_pad: (f32, f32),
    footer_pad: (f32, f32),
    footer_height: f32,
    sidebar: Option<El>,
    body: Vec<El>,
    footer: Vec<El>,
    /// The header close control's handler.
    dismiss: Option<Box<dyn FnOnce(El) -> El>>,
    /// The modal policy: Escape and (when enabled) a press outside the panel.
    close: Option<Box<dyn FnOnce(El) -> El>>,
}

impl Dialog {
    pub(crate) fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            description: None,
            open: false,
            open_epoch: 0,
            width: 576.0,
            fit: false,
            fill: false,
            height: None,
            max_height: None,
            body_max_height: None,
            click_outside: false,
            header_pad: (20.0, 16.0),
            body_pad: (20.0, 16.0),
            footer_pad: (20.0, 12.0),
            footer_height: FOOTER_HEIGHT,
            sidebar: None,
            body: Vec::new(),
            footer: Vec::new(),
            dismiss: None,
            close: None,
        }
    }

    /// Whether the dialog currently belongs on the layer. A closed dialog
    /// pushes nothing.
    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// The open generation: the caller bumps it every time the dialog opens
    /// (next to the `*_dialog_open` flag). Each value gives the body a new
    /// element identity, so a dialog that unmounted after the user scrolled it
    /// reopens at the top instead of at the remembered offset (see the module
    /// docs). While the dialog stays open the value does not change, so a view
    /// may rebuild freely without losing the offset.
    pub(crate) fn open_epoch(mut self, epoch: u64) -> Self {
        self.open_epoch = epoch;
        self
    }

    /// Part of the base's surface for the dialogs after T0-05 (kit's own
    /// `Dialog` has the same builder), so it is not used yet.
    #[allow(dead_code)]
    pub(crate) fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// The panel's maximum width; its layer pads 16pt on every side.
    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Sizes the panel to its header, body and footer instead of the standard
    /// 368pt height, for short dialogs such as a confirmation. The body is not
    /// grown: give it [`Dialog::body_max_height`] when its content can outgrow
    /// the window.
    pub(crate) fn fit(mut self) -> Self {
        self.fit = true;
        self
    }

    /// Fills the layer's height (the window less its 16pt padding), capped by
    /// [`Dialog::max_height`]. The body grows into what the header and footer
    /// leave, and scrolls, so a tall form never pushes the actions out.
    pub(crate) fn fill_height(mut self) -> Self {
        self.fill = true;
        self
    }

    /// A fixed panel height, for a dialog that must not jump as its content
    /// changes.
    /// Part of the base's surface for the dialogs after T0-05; unused today.
    #[allow(dead_code)]
    pub(crate) fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub(crate) fn max_height(mut self, max_height: f32) -> Self {
        self.max_height = Some(max_height);
        self
    }

    /// Caps a [`Dialog::fit`] body, so tall content scrolls instead of growing
    /// the panel past the window.
    pub(crate) fn body_max_height(mut self, height: f32) -> Self {
        self.body_max_height = Some(height);
        self
    }

    /// Whether a press outside the panel dismisses it. Off by default: the
    /// scrim only dims, as the React dialogs' backdrop does.
    /// Part of the base's surface for the dialogs after T0-05; unused today.
    #[allow(dead_code)]
    pub(crate) fn click_outside(mut self, close: bool) -> Self {
        self.click_outside = close;
        self
    }

    /// Header padding, `(x, y)`.
    pub(crate) fn header_pad(mut self, pad: (f32, f32)) -> Self {
        self.header_pad = pad;
        self
    }

    /// Body padding, `(x, y)`.
    pub(crate) fn body_pad(mut self, pad: (f32, f32)) -> Self {
        self.body_pad = pad;
        self
    }

    /// Footer padding, `(x, y)`; the footer's buttons are right aligned.
    pub(crate) fn footer_pad(mut self, pad: (f32, f32)) -> Self {
        self.footer_pad = pad;
        self
    }

    pub(crate) fn footer_height(mut self, height: f32) -> Self {
        self.footer_height = height;
        self
    }

    /// A column left of the body and footer, spanning the panel below the
    /// header (React's `NewConnectionDialog` type picker).
    pub(crate) fn sidebar(mut self, sidebar: El) -> Self {
        self.sidebar = Some(sidebar);
        self
    }

    /// A body child. The body scrolls when the panel bounds its height.
    pub(crate) fn child(mut self, child: El) -> Self {
        self.body.push(child);
        self
    }

    /// A footer child (one button per call); buttons sit right aligned.
    pub(crate) fn footer(mut self, child: El) -> Self {
        self.footer.push(child);
        self
    }

    /// The dismissal policy: the header close control presses it, and so does
    /// Escape and, when [`Dialog::click_outside`] is on, a press on the scrim.
    /// A dialog without it mounts nothing, as `rune_kit::Dialog` does.
    pub(crate) fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        let button = f.clone();
        self.dismiss = Some(Box::new(move |e| e.on_press(button)));
        self.close = Some(Box::new(move |e| e.modal(true, f)));
        self
    }

    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) {
        if !self.open {
            return;
        }
        let Some(close) = self.close else {
            return;
        };

        let mut dismiss = IconButton::new(k.glyph.close)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .label("Close")
            .build(k)
            .key(format!("{}-close", self.key));
        if let Some(handle) = self.dismiss {
            dismiss = handle(dismiss);
        }
        dismiss = dismiss.size(24.0, 24.0).mt(-2.0).mr(-2.0);

        let title_font = match self.description {
            Some(_) => TextStyle {
                size: 15.0,
                tracking: -0.13,
                ..k.ty.ui_medium
            }
            .lh(20.0),
            None => k.ty.ui_medium.lh(20.0),
        };
        let mut title = col()
            .grow(1.0)
            .min_w(0.0)
            .gap(2.0)
            .child(
                text(self.title.clone())
                    .key(format!("{}-title", self.key))
                    .font(title_font)
                    .color(k.c.text_base)
                    // React's `truncate`: one line with an ellipsis instead of
                    // a hard cut at the panel's edge.
                    .truncate(),
            );
        if let Some(description) = self.description.clone() {
            title = title.child(
                text(description)
                    .font(k.ty.ui.lh(18.0))
                    .color(k.c.text_muted),
            );
        }

        let header = row()
            .key(format!("{}-header", self.key))
            .w_full()
            .px(self.header_pad.0)
            .py(self.header_pad.1)
            .items_center()
            .border(1.0, k.c.border_muted)
            .child(title)
            .child(dismiss);

        // A panel with a height of its own bounds the body, which grows into
        // the leftover and scrolls. A fitted panel is as tall as its content,
        // so the body scrolls only when the caller caps it; nothing there may
        // grow, or the panel would measure no height for it.
        let definite = !(self.fit && self.height.is_none());
        let mut body = col()
            .key(format!("{}-body", self.key))
            .w_full()
            .px(self.body_pad.0)
            .py(self.body_pad.1)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .children(self.body);
        if definite {
            body = body.grow(1.0).min_h(0.0);
        } else if let Some(max) = self.body_max_height {
            body = body.max_h(max);
        }

        // The epoch is part of this wrapper's identity but not of the
        // documented body key: each open is a new body to Rune (and so a new
        // scroll viewport), while lookups still find `<key>-body`.
        let mut content = col()
            .key(format!("{}-content-{}", self.key, self.open_epoch))
            .w_full()
            .min_w(0.0)
            .child(body);
        if definite {
            content = content.grow(1.0).min_h(0.0);
        }
        if !self.footer.is_empty() {
            content = content.child(
                row()
                    .key(format!("{}-footer", self.key))
                    .w_full()
                    .h(self.footer_height)
                    .px(self.footer_pad.0)
                    .py(self.footer_pad.1)
                    .center_y()
                    .justify_end()
                    .gap(8.0)
                    .border(1.0, k.c.border_muted)
                    .children(self.footer),
            );
        }

        let mut panel = col()
            .key(self.key.clone())
            .role("dialog")
            .label(self.title.clone())
            .w_full()
            .max_w(self.width)
            .radius(PANEL_RADIUS)
            .fill(k.c.bg_base)
            .border(1.0, k.c.border_base)
            .child(header);
        panel = match self.sidebar {
            Some(sidebar) => panel.child(
                row()
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .child(sidebar)
                    .child(content),
            ),
            None => panel.child(content),
        };
        panel = match (self.fit, self.height) {
            (true, _) => panel,
            (false, Some(height)) => panel.h(height),
            (false, None) if self.fill => panel.h_full(),
            (false, None) => panel.h(PANEL_HEIGHT),
        };
        if let Some(max) = self.max_height {
            panel = panel.max_h(max);
        }
        // React's `shadow-xl` on the panel; `rune_kit::Dialog` paints
        // `k.elevation.overlay` the same way, and `Kit::elevate` is the shared
        // helper the popovers and menus can reuse.
        let panel = k.elevate(panel, &k.elevation.overlay);
        let panel = close(panel).modal_click_outside(self.click_outside);

        layer.push(
            rune::el()
                .key(format!("{}-scrim", self.key))
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .fill(Color::BLACK.alpha(SCRIM_ALPHA))
                .a11y_hidden(),
        );
        layer.push(
            rune::el()
                .key(format!("{}-layer", self.key))
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .padding(LAYER_PAD)
                .center()
                .child(panel),
        );
    }
}
