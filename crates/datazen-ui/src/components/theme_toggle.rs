//! The theme toggle, ported from React
//! `packages/frontend/src/components/ThemeToggle.tsx` (T282).
//!
//! The chrome's theme picker: a square icon button (the current mode's
//! icon, a tooltip naming it) opening a three-option popover — light,
//! dark, system — the current option washed, each option press updating
//! the settings theme mode and closing.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `useSettingsStore((s) => s.settings.theme)` | the `mode` build input (the host reads the store) |
//! | `updateSettings({ theme: { ...theme, mode } })` (53) | [`ThemeToggle::on_select`] — the host's settings write (the pack id and the rest of the preference survive the host's spread) |
//! | `useState(open)` (19) | the `open` build input (the host's cell, the one-open-slot trade) |
//! | the outside-`mousedown` effect (22-29) | the scrim + `modal` dismissal (presses outside and Escape close through [`ThemeToggle::on_close`]) — the backup window's dropdown precedent |
//! | `OPTIONS` (9-13) | [`OPTIONS`] — the three modes with their pack words and icon ids |
//! | `OPTIONS.find(...) ?? OPTIONS[2]` (31) | [`current_option`] — the current mode's option, system for an unknown mode |
//! | the trigger's `title` (40) | the help channel (`theme.tooltip` with `{current}` replaced) |
//! | `<ThemedIcon id={…} />` (42, 63) | the T262 port over the host's shared resolver (the `resolver` prop); `None` uses the default resolver, whose empty maps degrade every theme icon to the `?` placeholder — the resolver's construction is T535's seam (segment 2) |
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (internal) | `theme-toggle` |
//! | the trigger (React test id) | `theme-toggle-button` |
//! | the scrim (internal) | `theme-toggle-scrim` |
//! | the popover (internal) | `theme-toggle-menu` |
//! | one option (React test id) | `theme-option-{value}` |
//!
//! ## Documented deviations from React
//!
//! * **The popover is a scrim-dismissed anchored panel**, the backup
//!   window's dropdown ruling: the scrim takes the outside press, the
//!   `modal` channel takes Escape, both through the one `on_close`
//!   seam — the same observable outcome as the document-level
//!   `mousedown` listener.
//! * **The icon resolution rides the shared resolver seam** (T535): the
//!   tests pass a resolver mapping the three theme ids to lucide
//!   names (sun/moon/monitor — all in the registry); the default
//!   resolver's placeholder is the un-resolved honest render.
//! * **`z-50`/`shadow-lg`** are the kit's raised popover look
//!   ([`rune_kit::popover::raised`]) — the same chrome every anchored
//!   surface here uses.

use std::rc::Rc;

use rune::{col, row, text, Color, El};
use rune_kit::{popover, scrim, Button, ButtonVariant, ControlSize, Kit, Placement};

use crate::components::themed_icon::{
    default_resolver, IconResolver, ResolvedIcon, ThemedIcon,
};
use crate::i18n::t;
use crate::views::appearance_section::ThemeMode;

/// React `OPTIONS` (9-13): the three modes with their pack words and
/// their icon ids (the theme-pack semantic ids).
pub struct ThemeOption {
    pub value: ThemeMode,
    /// The option's pack word (`theme.light` &c).
    pub key: &'static str,
    /// The ThemedIcon semantic id (also `theme.light` &c — the ids and
    /// the words share their spelling in React's catalog).
    pub icon_id: &'static str,
}

/// The three options, in React's order.
pub const OPTIONS: [ThemeOption; 3] = [
    ThemeOption { value: ThemeMode::Light, key: "theme.light", icon_id: "theme.light" },
    ThemeOption { value: ThemeMode::Dark, key: "theme.dark", icon_id: "theme.dark" },
    ThemeOption { value: ThemeMode::System, key: "theme.system", icon_id: "theme.system" },
];

/// React 31: `OPTIONS.find((o) => o.value === theme.mode) ?? OPTIONS[2]`
/// — the current mode's option, system for an unknown mode.
pub fn current_option(mode: ThemeMode) -> &'static ThemeOption {
    OPTIONS
        .iter()
        .find(|option| option.value == mode)
        .unwrap_or(&OPTIONS[2])
}

/// React's default `LUCIDE_MAP` entries for the three theme ids: Sun, Moon and
/// Monitor. The built-in resolver's empty map would otherwise degrade them to
/// `?` badges; the host has no theme pack loaded, so this is the honest icon
/// face for all three options.
pub fn theme_icon_resolver() -> IconResolver {
    Rc::new(|id| match id {
        "theme.light" => ResolvedIcon::Lucide("sun".to_owned()),
        "theme.dark" => ResolvedIcon::Lucide("moon".to_owned()),
        "theme.system" => ResolvedIcon::Lucide("monitor".to_owned()),
        _ => default_resolver(id),
    })
}

/// The theme toggle. `S` is the host state that owns the open cell and
/// the settings write.
pub struct ThemeToggle<S: 'static> {
    mode: ThemeMode,
    open: bool,
    resolver: Option<IconResolver>,
    on_toggle: Option<Rc<dyn Fn(&mut S)>>,
    on_select: Option<Rc<dyn Fn(&mut S, ThemeMode)>>,
    on_close: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> ThemeToggle<S> {
    /// The toggle over the current mode and the host's open cell.
    pub fn new(mode: ThemeMode, open: bool) -> Self {
        Self {
            mode,
            open,
            resolver: None,
            on_toggle: None,
            on_select: None,
            on_close: None,
        }
    }

    /// The shared icon resolver (React's `ThemedIcon` context; the
    /// resolver's construction is T535's seam). `None` uses the
    /// default resolver.
    pub fn resolver(mut self, resolver: IconResolver) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// The trigger's press: `setOpen((v) => !v)`.
    pub fn on_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }

    /// An option's press (React 52-55): the settings write and the
    /// close — the host performs both.
    pub fn on_select(mut self, f: impl Fn(&mut S, ThemeMode) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// The outside press and Escape (the scrim and the modal channel).
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// The toggle on its own kit root: the trigger in place, the
    /// popover and scrim mounted through the root's layer while open.
    pub fn build_root(self, k: &Kit) -> El {
        let mut layer = Vec::new();
        let root = self.build(k, &mut layer);
        rune_kit::root(root, layer)
    }

    /// The toggle: the trigger in place, the popover in `layer` while
    /// open.
    pub fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let current = current_option(self.mode);

        // React 40: the tooltip names the current mode.
        let tooltip = t("theme.tooltip").replacen("{current}", t(current.key), 1);

        // React 35-43: the square trigger — `inline-flex h-8 w-8
        // items-center justify-center rounded-md text-fg-muted
        // hover:bg-surface-raised hover:text-fg`. The button's visible
        // face is the icon (16pt, React 42); its accessible label stays
        // the current mode's word.
        let icon_el = self.icon(current.icon_id, 16.0).a11y_hidden();
        let mut trigger = Button::new(t(current.key))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .help(tooltip.clone())
            // React's trigger uses `text-fg-muted`; the kit hands us the
            // button's ink for exactly this. `rune::icon` starts white, so
            // leaving it untinted made the light-mode icon invisible.
            .label_with(move |_, ink| icon_el.color(ink));
        if let Some(on_toggle) = &self.on_toggle {
            let action = Rc::clone(on_toggle);
            trigger = trigger.on_press(move |s: &mut S| action(s));
        }
        let trigger = trigger.build(k).key("theme-toggle-button");

        let root = row()
            .key("theme-toggle")
            .child(trigger);

        // React 45-68: the popover while open — the scrim, then the
        // anchored panel.
        if self.open {
            let close = self
                .on_close
                .clone()
                .unwrap_or_else(|| Rc::new(|_s: &mut S| {}));
            let close_for_scrim = Rc::clone(&close);
            layer.push(scrim("theme-toggle-scrim", move |s: &mut S| {
                close_for_scrim(s)
            }));
            let close_for_modal = Rc::clone(&close);
            let mut panel = col()
                .key("theme-toggle-menu")
                .min_w(144.0) // w-36
                .padding(4.0)
                .radius(k.radius.md)
                .modal(true, move |s: &mut S| close_for_modal(s));
            for option in &OPTIONS {
                panel = panel.child(self.option_row(k, option));
            }
            layer.push(popover::anchored(
                k,
                popover::raised(k, panel, k.radius.lg),
                "theme-toggle-button",
                Placement::BottomEnd,
                4.0, // mt-1
            ));
        }
        root
    }

    /// React 47-66: one option row — `flex w-full items-center gap-2
    /// px-3 py-2 text-xs`, the current mode washed (`bg-accent/10
    /// text-accent`), the others muted with the raised hover.
    fn option_row(&self, k: &Kit, option: &ThemeOption) -> El {
        let is_current = option.value == self.mode;
        let label = t(option.key);

        let mut row = row()
            .key(format!("theme-option-{}", option.value.as_str()))
            .w_full()
            .items_center()
            .gap(8.0)
            .px(12.0)
            .py(8.0)
            .radius(k.radius.sm)
            .role("button")
            .focusable(true)
            .help(label.to_owned())
            .child(
                self.icon(option.icon_id, 14.0) // h-3.5 w-3.5
                    .key(format!("theme-option-{}-icon", option.value.as_str()))
                    // React's option icon inherits the row's ink.
                    .color(if is_current { k.c.text_accent } else { k.c.text_muted })
                    .a11y_hidden(),
            )
            .child(
                text(label)
                    .key(format!("theme-option-{}-text", option.value.as_str()))
                    .font(k.ty.text_xsmall)
                    .color(if is_current {
                        k.c.text_accent
                    } else {
                        k.c.text_muted
                    }),
            );
        if is_current {
            // React 59: `bg-accent/10`.
            row = row.fill(k.alpha(k.c.bg_accent, 0.1));
        } else {
            // React 60: `hover:bg-surface-raised` — the standard hover
            // overlay; the icon-only muted trigger's own ruling.
            row = row.hover(k.c.surface_raised);
        }
        if let Some(on_select) = &self.on_select {
            let action = Rc::clone(on_select);
            let value = option.value;
            row = row.on_press(move |s: &mut S| action(s, value));
        }
        row
    }

    /// One theme icon through the shared resolver at `size` points —
    /// the `?` placeholder honestly when nothing resolves.
    fn icon(&self, icon_id: &str, size: f32) -> El {
        let mut icon = ThemedIcon::<S>::new("theme-toggle-icon", icon_id).size_pt(size);
        if let Some(resolver) = &self.resolver {
            let resolver = Rc::clone(resolver);
            icon = icon.resolver(resolver);
        }
        icon.build(true)
    }
}

/// Keeps the `ResolvedIcon` import honest: the tests' resolver maps the
/// three theme ids to lucide names through the enum the port shares
/// with the T262 seam.
#[allow(dead_code)]
fn resolver_shape(resolver: &IconResolver) -> ResolvedIcon {
    resolver("theme.light")
}

/// Keeps the `Color` import honest: the current option's accent wash is
/// the accent background at 10% alpha, the surface fills are the kit's.
#[allow(dead_code)]
fn accent_wash(k: &Kit) -> Color {
    k.alpha(k.c.bg_accent, 0.1)
}
