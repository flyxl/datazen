//! Menu and context-menu wrappers over `rune-kit`.
//!
//! Thin builders that assemble `rune_kit::Menu` and `rune_kit::MenuItem` so
//! callers get DataZen's entry point without reimplementing popup positioning,
//! the keyboard model, or close logic.

use rune::{El, PointerPress};
use rune_kit::Kit;

pub(crate) use rune_kit::Placement;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryKind {
    Separator,
    Label,
}

enum Entry<S: 'static> {
    Item(Item<S>),
    Structural(EntryKind, String),
}

pub(crate) struct Item<S: 'static> {
    label: String,
    disabled: bool,
    on_select: Option<Box<dyn Fn(&mut S) + 'static>>,
}

impl<S: 'static> Item<S> {
    pub(crate) fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            disabled: false,
            on_select: None,
        }
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn on_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select = Some(Box::new(f));
        self
    }
}

pub(crate) struct Menu<S: 'static> {
    key: String,
    trigger: Option<El>,
    entries: Vec<Entry<S>>,
    open: bool,
    min_width: Option<f32>,
    at: Option<(f32, f32)>,
    placement: Option<Placement>,
    on_close: Option<Box<dyn Fn(&mut S) + 'static>>,
    context: bool,
}

impl<S: 'static> Menu<S> {
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            trigger: None,
            entries: Vec::new(),
            open: false,
            min_width: None,
            at: None,
            placement: None,
            on_close: None,
            context: false,
        }
    }

    pub(crate) fn trigger(mut self, trigger: El) -> Self {
        self.trigger = Some(trigger);
        self
    }

    pub(crate) fn item(mut self, item: Item<S>) -> Self {
        self.entries.push(Entry::Item(item));
        self
    }

    pub(crate) fn separator(mut self) -> Self {
        self.entries.push(Entry::Structural(EntryKind::Separator, String::new()));
        self
    }

    pub(crate) fn group_label(mut self, label: impl Into<String>) -> Self {
        self.entries.push(Entry::Structural(EntryKind::Label, label.into()));
        self
    }

    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub(crate) fn min_width(mut self, w: f32) -> Self {
        self.min_width = Some(w);
        self
    }

    /// Anchor the menu at a window point (e.g. a secondary press position).
    pub(crate) fn at(mut self, x: f32, y: f32) -> Self {
        self.at = Some((x, y));
        self
    }

    pub(crate) fn placement(mut self, placement: Placement) -> Self {
        self.placement = Some(placement);
        self
    }

    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Box::new(f));
        self
    }

    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut menu = if self.context {
            let point = self.at.map(|(x, y)| PointerPress { x, y });
            let trigger = self.trigger.unwrap_or_else(|| rune::row());
            let mut m = rune_kit::context_menu(self.key, trigger, point);
            if self.open && point.is_none() {
                m = m.open(true);
            }
            m
        } else {
            let mut m = rune_kit::Menu::new(self.key);
            if let Some(trigger) = self.trigger {
                m = m.trigger(trigger);
            }
            m = m.open(self.open);
            if let Some((x, y)) = self.at {
                m = m.at(x, y);
            }
            m
        };

        if let Some(w) = self.min_width {
            menu = menu.min_width(w);
        }
        if let Some(p) = self.placement {
            menu = menu.placement(p);
        }
        if let Some(close) = self.on_close {
            menu = menu.on_close(close);
        }

        for entry in self.entries {
            menu = match entry {
                Entry::Item(item) => {
                    let mut mi = rune_kit::MenuItem::new(item.label).disabled(item.disabled);
                    if let Some(f) = item.on_select {
                        mi = mi.on_select(f);
                    }
                    menu.item(mi)
                }
                Entry::Structural(EntryKind::Separator, _) => menu.separator(),
                Entry::Structural(EntryKind::Label, label) => menu.group_label(label),
            };
        }

        menu.build(k, layer)
    }
}

/// A menu opened at a secondary-press position. When `point` is `None` the
/// menu stays closed and only the trigger is mounted.
pub(crate) fn context_menu<S: 'static>(
    key: impl Into<String>,
    trigger: El,
    point: Option<PointerPress>,
) -> Menu<S> {
    let mut menu = Menu::new(key).trigger(trigger).open(point.is_some());
    menu.context = true;
    if let Some(p) = point {
        menu = menu.at(p.x, p.y);
    }
    menu
}
