//! The DataZen tooltip and hover card over `rune-kit` overlays.
//!
//! The trigger stays in place and the floating content is pushed into `layer` while `open`,
//! anchored to the trigger (the same two-part build as `ui/select.rs`). Mount the layer at the
//! root with `rune_kit::root` so the overlay paints above the page.
//!
//! Tooltip and hover card are state-driven surfaces: the app owns `open` and flips it on
//! hover/focus/press; this wrapper does not add implicit keyboard handlers.

#![allow(dead_code)]

use rune::{col, row, El};
use rune_kit::{popover, Kit, Placement, Tooltip as KitTooltip, TooltipAppearance};

/// A small hint anchored to a trigger.
pub(crate) struct Tooltip {
    key: String,
    trigger: Option<El>,
    text: Option<String>,
    label: Option<String>,
    children: Vec<El>,
    appearance: TooltipAppearance,
    placement: Placement,
    open: bool,
}

impl Tooltip {
    pub(crate) fn new(key: impl Into<String>) -> Tooltip {
        Tooltip {
            key: key.into(),
            trigger: None,
            text: None,
            label: None,
            children: Vec::new(),
            appearance: TooltipAppearance::Compact,
            placement: Placement::Bottom,
            open: false,
        }
    }

    pub(crate) fn trigger(mut self, trigger: El) -> Tooltip {
        self.trigger = Some(trigger);
        self
    }

    /// Plain text content in the kit's font and ink for the chosen appearance.
    pub(crate) fn text(mut self, text: impl Into<String>) -> Tooltip {
        self.text = Some(text.into());
        self
    }

    /// Plain text content in the style's font and ink, whichever style builds the tooltip.
    pub(crate) fn label(mut self, label: impl Into<String>) -> Tooltip {
        self.label = Some(label.into());
        self
    }

    /// Any extra content (keybinds, spans).
    pub(crate) fn child(mut self, child: El) -> Tooltip {
        self.children.push(child);
        self
    }

    pub(crate) fn appearance(mut self, appearance: TooltipAppearance) -> Tooltip {
        self.appearance = appearance;
        self
    }

    pub(crate) fn placement(mut self, placement: Placement) -> Tooltip {
        self.placement = placement;
        self
    }

    pub(crate) fn open(mut self, open: bool) -> Tooltip {
        self.open = open;
        self
    }

    /// Builds the trigger, pushing the tooltip into `layer` while `open`.
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut kit = KitTooltip::new(&self.key)
            .appearance(self.appearance)
            .placement(self.placement)
            .open(self.open);
        if let Some(trigger) = self.trigger {
            kit = kit.trigger(trigger);
        }
        if let Some(text) = self.text {
            kit = kit.text(k, text);
        }
        if let Some(label) = self.label {
            kit = kit.label(label);
        }
        for child in self.children {
            kit = kit.child(child);
        }
        kit.build(k, layer)
    }
}

/// A rich preview card anchored to a trigger.
pub(crate) struct HoverCard {
    key: String,
    trigger: Option<El>,
    body: Vec<El>,
    placement: Placement,
    open: bool,
}

impl HoverCard {
    pub(crate) fn new(key: impl Into<String>) -> HoverCard {
        HoverCard {
            key: key.into(),
            trigger: None,
            body: Vec::new(),
            placement: Placement::Bottom,
            open: false,
        }
    }

    pub(crate) fn trigger(mut self, trigger: El) -> HoverCard {
        self.trigger = Some(trigger);
        self
    }

    pub(crate) fn child(mut self, child: El) -> HoverCard {
        self.body.push(child);
        self
    }

    pub(crate) fn placement(mut self, placement: Placement) -> HoverCard {
        self.placement = placement;
        self
    }

    pub(crate) fn open(mut self, open: bool) -> HoverCard {
        self.open = open;
        self
    }

    /// Builds the trigger, pushing the card into `layer` while `open`.
    ///
    /// Implemented with `rune_kit::popover` primitives so the anchor sizes to the trigger
    /// (not a full-width row) and top/bottom placements fit within the window.
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let anchor_key = format!("{}-trigger", self.key);
        let trigger = row().key(anchor_key.clone()).children(self.trigger);
        if !self.open {
            return trigger;
        }
        let content = col()
            .key(format!("{}-card", self.key))
            .min_w(200.0)
            .max_w(320.0)
            .child(col().padding(4.0).children(self.body));
        layer.push(popover::anchored(
            k,
            popover::raised(k, content, k.radius.lg),
            &anchor_key,
            self.placement,
            4.0,
        ));
        trigger
    }
}