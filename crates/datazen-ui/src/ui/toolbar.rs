//! Toolbar primitives: a button and a shell row.
//!
//! `ToolbarButton` wraps `rune-kit`'s `Button`/`IconButton` (ghost/variant
//! visuals, focus, disabled opacity) instead of re-implementing button chrome.
//! `ToolbarShell` is a simple horizontal container.
//!
//! ## Key convention
//!
//! | part            | key               |
//! |-----------------|-------------------|
//! | ToolbarButton   | `<key>`           |
//! | ToolbarShell    | `<key>`           |
//! | Shell leading   | `<key>-leading`   |
//! | Shell trailing  | `<key>-trailing`  |

use rune::{row, El, Icon};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant};

/// Toolbar button variants aligned to React's ToolbarButton where meaningful.
#[allow(dead_code)] // reusable base; remove when view callers land
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum ToolbarButtonVariant {
    #[default]
    Ghost,
    Primary,
    Secondary,
    Danger,
    Run,
}

/// A toolbar action button. Icon+label or icon-only, backed by rune-kit button primitives.
#[allow(dead_code)] // reusable base; remove when view callers land
pub(crate) struct ToolbarButton<S: 'static> {
    key: String,
    label: String,
    icon: Option<&'static Icon>,
    variant: ToolbarButtonVariant,
    compact: bool,
    icon_only: bool,
    disabled: bool,
    on_press: Option<Box<dyn Fn(&mut S) + 'static>>,
}

#[allow(dead_code)] // reusable base; remove when view callers land
impl<S: 'static> ToolbarButton<S> {
    pub(crate) fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            icon: None,
            variant: ToolbarButtonVariant::Ghost,
            compact: false,
            icon_only: false,
            disabled: false,
            on_press: None,
        }
    }

    pub(crate) fn icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub(crate) fn variant(mut self, variant: ToolbarButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub(crate) fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    pub(crate) fn icon_only(mut self, icon_only: bool) -> Self {
        self.icon_only = icon_only;
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn on_press(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let tip = self.label.clone();
        let use_icon_button = self.icon.is_some() && (self.icon_only || self.compact);
        if use_icon_button {
            if let Some(i) = self.icon {
                let mut b = IconButton::new(i)
                    .variant(match self.variant {
                        ToolbarButtonVariant::Ghost | ToolbarButtonVariant::Secondary => {
                            IconButtonVariant::Ghost
                        }
                        ToolbarButtonVariant::Primary | ToolbarButtonVariant::Run => {
                            IconButtonVariant::Neutral
                        }
                        ToolbarButtonVariant::Danger => IconButtonVariant::Neutral,
                    })
                    .control_size(ControlSize::Regular)
                    .label(tip.clone())
                    .help(tip);
                if self.disabled {
                    b = b.disabled(true);
                }
                if let Some(f) = self.on_press {
                    b = b.on_press(f);
                }
                return b.build(k).key(self.key);
            }
        }
        let mut b = Button::new(self.label.clone())
            .variant(match self.variant {
                ToolbarButtonVariant::Ghost | ToolbarButtonVariant::Secondary => {
                    ButtonVariant::Ghost
                }
                ToolbarButtonVariant::Primary | ToolbarButtonVariant::Run => {
                    ButtonVariant::Contrast
                }
                ToolbarButtonVariant::Danger => ButtonVariant::Danger,
            })
            .control_size(ControlSize::Regular)
            .help(tip);
        if let Some(i) = self.icon {
            b = b.icon(i);
        }
        if self.disabled {
            b = b.disabled(true);
        }
        if let Some(f) = self.on_press {
            b = b.on_press(f);
        }
        b.build(k).key(self.key)
    }
}

/// Horizontal toolbar container with optional leading/trailing groups.
#[allow(dead_code)] // reusable base; remove when view callers land
pub(crate) struct ToolbarShell {
    key: String,
    leading: Vec<El>,
    trailing: Vec<El>,
    children: Vec<El>,
}

#[allow(dead_code)] // reusable base; remove when view callers land
impl ToolbarShell {
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            leading: Vec::new(),
            trailing: Vec::new(),
            children: Vec::new(),
        }
    }

    pub(crate) fn leading(mut self, items: Vec<El>) -> Self {
        self.leading = items;
        self
    }

    pub(crate) fn trailing(mut self, items: Vec<El>) -> Self {
        self.trailing = items;
        self
    }

    pub(crate) fn children(mut self, items: Vec<El>) -> Self {
        self.children = items;
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let mut bar = row()
            .key(self.key.clone())
            .w_full()
            .min_w(0.0)
            .items_center()
            .gap(8.0)
            .px(12.0)
            .py(4.0)
            .fill(k.c.layer_01)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .border_layout()
            .scroll(rune::ScrollSpec { stick_bottom: false });

        if !self.leading.is_empty() {
            let mut lead = row()
                .key(format!("{}-leading", self.key))
                .items_center()
                .gap(8.0)
                .shrink();
            for c in self.leading {
                lead = lead.child(c);
            }
            bar = bar.child(lead);
        }
        for c in self.children {
            bar = bar.child(c);
        }
        if !self.trailing.is_empty() {
            let mut trail = row()
                .key(format!("{}-trailing", self.key))
                .items_center()
                .gap(8.0)
                .grow(1.0)
                .min_w(0.0)
                .justify_end();
            for c in self.trailing {
                trail = trail.child(c);
            }
            bar = bar.child(trail);
        }
        bar
    }
}
