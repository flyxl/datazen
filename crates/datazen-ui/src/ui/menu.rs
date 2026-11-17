//! Menu and context-menu wrappers over `rune-kit`.
//!
//! Thin builders that assemble `rune_kit::Menu` and `rune_kit::MenuItem` so
//! callers get DataZen's entry point without reimplementing popup positioning,
//! the keyboard model, or close logic.
//!
//! # Key convention
//! The root `key` is caller-provided and must be unique within a frame. `rune-kit`
//! derives element keys from it (`{key}-menu`, `{key}-trigger`, `{key}-point`,
//! `{key}-item{i}`, `sep{i}`, `label{i}`, submenu surfaces as `{key}-item{i}-sub`,
//! and dismiss scrim `{key}-scrim`). Colliding root keys cause runtime renames
//! (e.g. `*-menu#dupN`) and should be avoided by callers.
//!
//! # Long menus
//! When the menu is taller than the available viewport, `rune-kit`'s anchored/scroll
//! fitting may not fully clamp an `at(...)`-anchored menu to the window bounds in all
//! cases. Callers should split very long menus (more items than fit the visible window)
//! into submenus.

use std::rc::Rc;

use rune::{El, Icon, PointerPress};
use rune_kit::Kit;

pub(crate) use rune_kit::Placement;
pub(crate) use rune_kit::State;

pub(crate) enum Entry<S: 'static> {
    Item(Item<S>),
    Separator,
    Label(String),
}

type SelectFn<S> = Box<dyn Fn(&mut S) + 'static>;

/// A menu item or submenu trigger.
pub(crate) struct Item<S: 'static> {
    label: String,
    icon: Option<&'static Icon>,
    lead: Option<El>,
    shortcut: Option<String>,
    badge: Option<String>,
    description: Option<String>,
    danger: bool,
    checked: Option<bool>,
    submenu: Option<(Vec<Entry<S>>, bool)>,
    state: Option<State>,
    disabled: bool,
    on_select: Option<SelectFn<S>>,
}

impl<S: 'static> Item<S> {
    pub(crate) fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            lead: None,
            shortcut: None,
            badge: None,
            description: None,
            danger: false,
            checked: None,
            submenu: None,
            state: None,
            disabled: false,
            on_select: None,
        }
    }

    pub(crate) fn icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub(crate) fn lead(mut self, lead: El) -> Self {
        self.lead = Some(lead);
        self
    }

    pub(crate) fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub(crate) fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub(crate) fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub(crate) fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }

    pub(crate) fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Make this item a submenu trigger with nested entries.
    pub(crate) fn submenu(mut self, entries: Vec<Entry<S>>, open: bool) -> Self {
        self.submenu = Some((entries, open));
        self
    }

    pub(crate) fn state(mut self, state: State) -> Self {
        self.state = Some(state);
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn on_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select = Some(Box::new(f));
        self
    }

    fn into_kit(self, close: Option<Rc<dyn Fn(&mut S)>>) -> rune_kit::MenuItem {
        let Item {
            label,
            icon,
            lead,
            shortcut,
            badge,
            description,
            danger,
            checked,
            submenu,
            state,
            disabled,
            on_select,
        } = self;

        let is_leaf = submenu.is_none();
        let mut mi = rune_kit::MenuItem::new(label);

        if let Some(icon) = icon {
            mi = mi.icon(icon);
        }
        if let Some(lead) = lead {
            mi = mi.lead(lead);
        }
        if let Some(shortcut) = shortcut {
            mi = mi.shortcut(shortcut);
        }
        if let Some(badge) = badge {
            mi = mi.badge(badge);
        }
        if let Some(description) = description {
            mi = mi.description(description);
        }
        if danger {
            mi = mi.danger(true);
        }
        if let Some(checked) = checked {
            mi = mi.checked(checked);
        }
        if let Some((entries, open)) = submenu {
            let kit_entries = entries
                .into_iter()
                .map(|entry| entry.into_kit(close.clone()))
                .collect();
            mi = mi.submenu(kit_entries, open);
        }
        if let Some(state) = state {
            mi = mi.state(state);
        }
        mi = mi.disabled(disabled);

        if let Some(f) = on_select {
            if is_leaf {
                if let Some(close) = close {
                    mi = mi.on_select(move |s: &mut S| {
                        f(s);
                        close(s);
                    });
                } else {
                    mi = mi.on_select(f);
                }
            } else {
                mi = mi.on_select(f);
            }
        }

        mi
    }
}

impl<S: 'static> Entry<S> {
    pub(crate) fn item(item: Item<S>) -> Self {
        Entry::Item(item)
    }

    pub(crate) fn separator() -> Self {
        Entry::Separator
    }

    pub(crate) fn group_label(label: impl Into<String>) -> Self {
        Entry::Label(label.into())
    }

    fn into_kit(self, close: Option<Rc<dyn Fn(&mut S)>>) -> rune_kit::MenuEntry {
        match self {
            Entry::Item(item) => rune_kit::MenuEntry::item(item.into_kit(close)),
            Entry::Separator => rune_kit::MenuEntry::Separator,
            Entry::Label(label) => rune_kit::MenuEntry::Label(label),
        }
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
    on_close: Option<SelectFn<S>>,
    close_on_select: bool,
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
            close_on_select: true,
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

    /// Append items in order. Equivalent to calling `item` repeatedly.
    pub(crate) fn items(mut self, items: impl IntoIterator<Item = Item<S>>) -> Self {
        for item in items {
            self.entries.push(Entry::Item(item));
        }
        self
    }

    pub(crate) fn separator(mut self) -> Self {
        self.entries.push(Entry::Separator);
        self
    }

    pub(crate) fn group_label(mut self, label: impl Into<String>) -> Self {
        self.entries.push(Entry::Label(label.into()));
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

    /// Called when the menu closes via Escape or an outside press, and (by default)
    /// after a leaf item is selected.
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Box::new(f));
        self
    }

    /// When `true` (default), selecting a leaf item runs its `on_select` then the
    /// close callback once. Set to `false` to keep the menu open after selection.
    /// Escape and outside presses still close through `on_close`.
    pub(crate) fn close_on_select(mut self, close: bool) -> Self {
        self.close_on_select = close;
        self
    }

    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let Menu {
            key,
            trigger,
            entries,
            open,
            min_width,
            at,
            placement,
            on_close,
            close_on_select,
            context,
        } = self;

        let close_rc = on_close.map(|f| Rc::new(f) as Rc<dyn Fn(&mut S)>);
        let close_for_items = if close_on_select { close_rc.clone() } else { None };

        let mut menu = if context {
            let point = at.map(|(x, y)| PointerPress { x, y });
            let trigger = trigger.unwrap_or_else(|| rune::row());
            let mut m = rune_kit::context_menu(key, trigger, point);
            if open && point.is_none() {
                m = m.open(true);
            }
            m
        } else {
            let mut m = rune_kit::Menu::new(key);
            if let Some(trigger) = trigger {
                m = m.trigger(trigger);
            }
            m = m.open(open);
            if let Some((x, y)) = at {
                m = m.at(x, y);
            }
            m
        };

        if let Some(w) = min_width {
            menu = menu.min_width(w);
        }
        if let Some(p) = placement {
            menu = menu.placement(p);
        }
        // Always wire Esc/outside dismissal to the close callback when provided.
        if let Some(close) = close_rc {
            menu = menu.on_close(move |s: &mut S| close(s));
        }

        for entry in entries {
            menu = match entry.into_kit(close_for_items.clone()) {
                rune_kit::MenuEntry::Item(item) => menu.item(*item),
                rune_kit::MenuEntry::Separator => menu.separator(),
                rune_kit::MenuEntry::Label(label) => menu.group_label(label),
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
