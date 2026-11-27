//! T195: the `components/MenuBar.tsx` port — the web-based menu bar for
//! Windows/Linux (where native menus are unavailable), rendered in the
//! title bar and mirroring the macOS native menu structure (Settings lives
//! under File because Windows has no app menu).
//!
//! # What maps where
//!
//! | React | here |
//! |---|---|
//! | `MenuItem` / `Menu` | [`MenuBarItem`] / [`MenuBarMenu`] (the label / shortcut / checked / separator / children model) |
//! | `useMenus()` (theme + product gates + i18n) | [`build_menus`] — pure over the theme and the static feature table |
//! | `PRODUCT_FEATURES` / `isProductFeatureEnabled` | [`PRODUCT_FEATURES`] / [`product_feature_enabled`] (the static const table) |
//! | `handleMenuAction(id)` | [`route_menu_action`] — the pure router; the side effects (windowManager opens, cross-window emit, fullscreen toggle, `openLogDir`, `openPath`) are the host's seams |
//! | `MenuBar()` (the bar + open state) | [`MenuBar`] — the component over a host-owned open-menu id |
//! | `MenuButton` (trigger + portal dropdown) | the [`crate::ui::menu::Menu`] wrapper (T0-07): the trigger is the bar button, the dropdown anchors below-left with the trigger's measured 2pt gutter and 200pt min width |
//! | `MenuEntry` (separator / submenu / leaf with checked + shortcut) | the wrapper's separator / submenu / checked / shortcut entries |
//! | `navigateMenu` (Left/Right wrap-around) | the host's Left/Right handling over [`next_menu_id`] (the pure wrap arithmetic) |
//! | `isMac` early return | the host's decision to mount the bar at all (the port renders what it is given) |
//!
//! # State ownership
//!
//! The open-menu id is the host's `S` (one [`MenuBarState`] field); the
//! bar component is a pure render + event router in the family's style.
//! Hover-to-switch (React's `onHover` with `openMenu` set) and
//! click-outside/Escape closing are the wrapper's own dismissal layer —
//! the host writes the id the hover reports.
//!
//! # Deliberate deviations from the React source
//!
//! * **Hover-to-switch is scrim-blocked (documented gap).** React's
//!   `onMouseEnter` switch (`if (openMenu) setOpenMenu(menu.id)`) rides the
//!   DOM, where the open dropdown's outside-press listener does not block
//!   hover. The kit dropdown's dismiss scrim is a window-wide hit target
//!   (it must be, to close on outside press), so once a dropdown is open
//!   the pointer over the bar hits the scrim, not the bar button — the
//!   hover switch cannot fire. The Left/Right keyboard navigation and the
//!   click-to-open/close toggle are the working switches; the `on_hover`
//!   channel stays wired for hosts that mount the bar with no dropdown
//!   open (its React guard makes it inert then anyway).
//! * **The portal is the kit's anchored overlay** — `createPortal` +
//!   `document.body` + `position: fixed` is exactly the problem the T0-07
//!   wrapper (the kit `Menu` at a trigger, gutter 2pt, min width 200pt)
//!   already solves; React's measured `rect.left / rect.bottom + 2`
//!   positioning is the wrapper's anchor.
//! * **DOM focus walking is the kit's keyboard model** — React's
//!   `querySelector('[role^="menuitem"]').focus()` family (first item on
//!   open, Up/Down/Home/End cycling, Escape returning focus to the
//!   trigger) is the kit menu's own behaviour; nothing here re-walks
//!   buttons.
//! * **`aria-haspopup` / `aria-controls` / `aria-expanded`** ride the
//!   wrapper's trigger (its module docs list what the kit writes); the
//!   `id={menu-${id}-popup}` React id is the wrapper key
//!   (`menu-bar-{id}` derives `-menu` from it).
//! * **`▸`** (the submenu arrow span) is the kit submenu's own glyph;
//!   `tabIndex={menuIndex === 0 ? 0 : -1}` (the bar's roving tabindex) is
//!   the runtime's focus-group behaviour — the bar row is a focus group
//!   and Left/Right move it, which is also React's own `navigateMenu`.
//! * **The theme items' labels** are the composed
//!   `t('menu.theme'): t('menu.themeLight')` strings, verbatim; the
//!   `menuitemradio` role and the `✓` are the wrapper's `checked` entry
//!   (the accent check the kit paints).

use std::rc::Rc;

use rune::{row, text, El, Tx};
use rune_kit::Placement;

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::menu::Menu;

/// The `ThemeMode` slice the menu table discriminates on (the settings
/// tree keeps its own full mirror; the host maps its state to this — the
/// components layer stays free of `views::` imports).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuBarTheme {
    #[default]
    System,
    Light,
    Dark,
}

impl MenuBarTheme {
    /// The wire spelling (React's `'light' | 'dark' | 'system'`).
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// React `PRODUCT_FEATURES` — the static product-level feature gates.
pub const PRODUCT_FEATURES: &[(&str, bool)] =
    &[("schemaDiff", true), ("dataSync", true), ("dataTransfer", true)];

/// React `isProductFeatureEnabled(key)` — the static table lookup.
pub fn product_feature_enabled(key: &str) -> bool {
    PRODUCT_FEATURES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, enabled)| *enabled)
        .unwrap_or(false)
}

/// React `MenuItem`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuBarItem {
    pub id: String,
    pub label: String,
    pub shortcut: Option<String>,
    /// `None` = no check slot at all; `Some(checked)` = the radio row.
    pub checked: Option<bool>,
    pub separator: bool,
    pub children: Vec<MenuBarItem>,
}

impl MenuBarItem {
    fn item(id: &str, label: String) -> Self {
        MenuBarItem {
            id: id.to_owned(),
            label,
            ..Self::default()
        }
    }

    fn separator(id: &str) -> Self {
        MenuBarItem {
            id: id.to_owned(),
            separator: true,
            ..Self::default()
        }
    }

    fn shortcut(mut self, shortcut: &str) -> Self {
        self.shortcut = Some(shortcut.to_owned());
        self
    }

    fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    fn children(mut self, children: Vec<MenuBarItem>) -> Self {
        self.children = children;
        self
    }
}

/// React `Menu`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuBarMenu {
    pub id: String,
    pub label: String,
    pub items: Vec<MenuBarItem>,
}

/// React `useMenus()` — the pure menu table over the theme and the
/// product feature gates. The label strings are `t()` lookups composed
/// exactly as React composes them.
pub fn build_menus(theme: MenuBarTheme) -> Vec<MenuBarMenu> {
    // ── the Tools items, in React's gated order ──
    let mut tools: Vec<MenuBarItem> = Vec::new();
    if product_feature_enabled("schemaDiff") {
        tools.push(MenuBarItem::item("schema-diff", t("common.schemaDiff").to_owned()));
        tools.push(MenuBarItem::separator("sep-3"));
    }
    if product_feature_enabled("dataSync") {
        tools.push(MenuBarItem::item("data-sync", t("common.dataSync").to_owned()));
        tools.push(MenuBarItem::separator("sep-3b"));
    }
    if product_feature_enabled("dataTransfer") {
        tools.push(MenuBarItem::item(
            "data-transfer",
            t("common.dataTransfer").to_owned(),
        ));
        tools.push(MenuBarItem::separator("sep-3a"));
    }
    tools.extend([
        MenuBarItem::item("workflow", t("menu.workflow").to_owned()),
        MenuBarItem::item("dashboard", t("menu.dashboard").to_owned()),
        MenuBarItem::separator("sep-3b"),
        MenuBarItem::item("backup", t("common.backupDatabase").to_owned()),
        MenuBarItem::item("restore", t("common.restoreDatabase").to_owned()),
        MenuBarItem::separator("sep-4"),
        MenuBarItem::item("view-logs", t("common.viewLogs").to_owned()),
    ]);

    vec![
        MenuBarMenu {
            id: "file".to_owned(),
            label: t("menu.file").to_owned(),
            items: vec![
                MenuBarItem::item(
                    "new-connection",
                    t("common.newConnection").to_owned(),
                )
                .shortcut("Ctrl+N"),
                MenuBarItem::separator("sep-f1"),
                MenuBarItem::item("export-config", t("common.exportAppData").to_owned()),
                MenuBarItem::item("import-config", t("common.importAppData").to_owned()),
                MenuBarItem::item(
                    "export-connections",
                    t("common.exportConnections").to_owned(),
                ),
                MenuBarItem::item(
                    "import-connections",
                    t("common.importConnections").to_owned(),
                )
                .children(vec![
                    MenuBarItem::item(
                        "import-connections-dbx",
                        t("menu.importFromDbx").to_owned(),
                    ),
                    MenuBarItem::item(
                        "import-connections-navicat",
                        t("menu.importFromNavicat").to_owned(),
                    ),
                    MenuBarItem::item(
                        "import-connections-datagrip",
                        t("menu.importFromDataGrip").to_owned(),
                    ),
                    MenuBarItem::item(
                        "import-connections-dbeaver",
                        t("menu.importFromDBeaver").to_owned(),
                    ),
                    MenuBarItem::item(
                        "import-connections-tableplus",
                        t("menu.importFromTablePlus").to_owned(),
                    ),
                    MenuBarItem::separator("sep-import"),
                    MenuBarItem::item(
                        "import-connections-file",
                        t("menu.importFromFile").to_owned(),
                    ),
                ]),
                MenuBarItem::separator("sep-f2"),
                MenuBarItem::item("open-settings", t("menu.settings").to_owned())
                    .shortcut("Ctrl+,"),
            ],
        },
        MenuBarMenu {
            id: "view".to_owned(),
            label: t("menu.view").to_owned(),
            items: vec![
                MenuBarItem::item(
                    "theme-light",
                    format!("{}: {}", t("menu.theme"), t("menu.themeLight")),
                )
                .checked(theme == MenuBarTheme::Light),
                MenuBarItem::item(
                    "theme-dark",
                    format!("{}: {}", t("menu.theme"), t("menu.themeDark")),
                )
                .checked(theme == MenuBarTheme::Dark),
                MenuBarItem::item(
                    "theme-system",
                    format!("{}: {}", t("menu.theme"), t("menu.themeSystem")),
                )
                .checked(theme == MenuBarTheme::System),
                MenuBarItem::separator("sep-2"),
                MenuBarItem::item("fullscreen", t("menu.fullscreen").to_owned()),
            ],
        },
        MenuBarMenu {
            id: "tools".to_owned(),
            label: t("menu.tools").to_owned(),
            items: tools,
        },
        MenuBarMenu {
            id: "help".to_owned(),
            label: t("menu.help").to_owned(),
            items: vec![
                MenuBarItem::item("help-docs", t("menu.documentation").to_owned()),
                MenuBarItem::separator("sep-h1"),
                MenuBarItem::item("help-report", t("menu.reportIssue").to_owned()),
            ],
        },
    ]
}

/// React `handleMenuAction(id)`'s routing, pure: what the action *is*; the
/// host performs it (the windowManager opens, the cross-window emit, the
/// fullscreen toggle, `openLogDir`, `openPath`, and the default
/// `emitMenuAction` are all host seams — none of them is state).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuBarAction {
    /// `theme-*`: the cross-window theme change (the host emits it).
    SetTheme(MenuBarTheme),
    OpenSchemaDiff,
    OpenDataSync,
    OpenDataTransfer,
    /// `backup` / `restore`: the backup window, in its mode.
    OpenBackup { restore: bool },
    /// `view-logs`: the log directory.
    OpenLogDir,
    /// `fullscreen`: the platform toggle.
    ToggleFullscreen,
    /// `help-docs`: the docs window.
    OpenDocs,
    /// `help-report`: the issue tracker URL.
    OpenReportIssue,
    /// The default: `emitMenuAction(id)` — the shell's menu events.
    EmitMenuAction(String),
}

/// Route one item id to its action, exactly React's `handleMenuAction`
/// order: the theme prefix first, then the migration openers map, then the
/// named switch, else the default emit.
pub fn route_menu_action(id: &str) -> MenuBarAction {
    if let Some(theme) = id.strip_prefix("theme-") {
        return match theme {
            "light" => MenuBarAction::SetTheme(MenuBarTheme::Light),
            "dark" => MenuBarAction::SetTheme(MenuBarTheme::Dark),
            _ => MenuBarAction::SetTheme(MenuBarTheme::System),
        };
    }
    match id {
        "schema-diff" => MenuBarAction::OpenSchemaDiff,
        "data-sync" => MenuBarAction::OpenDataSync,
        "data-transfer" => MenuBarAction::OpenDataTransfer,
        "backup" => MenuBarAction::OpenBackup { restore: false },
        "restore" => MenuBarAction::OpenBackup { restore: true },
        "view-logs" => MenuBarAction::OpenLogDir,
        "fullscreen" => MenuBarAction::ToggleFullscreen,
        "help-docs" => MenuBarAction::OpenDocs,
        "help-report" => MenuBarAction::OpenReportIssue,
        _ => MenuBarAction::EmitMenuAction(id.to_owned()),
    }
}

/// React `navigateMenu`'s wrap arithmetic: the next menu id Left/Right of
/// `current`, wrapping around the bar.
pub fn next_menu_id(menus: &[MenuBarMenu], current: &str, forward: bool) -> Option<String> {
    let index = menus.iter().position(|menu| menu.id == current)?;
    let delta = if forward { 1 } else { menus.len() as isize - 1 };
    let next = (index as isize + delta).rem_euclid(menus.len() as isize) as usize;
    Some(menus[next].id.clone())
}

/// The bar's open-menu state — React's `useState<string | null>` on the
/// host's `S`.
#[derive(Clone, Debug, Default)]
pub struct MenuBarState {
    /// The open menu's id; `None` = none open.
    pub open_menu: Option<String>,
}

type ActionFn<S> = Rc<dyn Fn(&mut S)>;
type SelectFn<S> = Rc<dyn Fn(&mut S, &str, &mut Tx)>;
type OpenFn<S> = Rc<dyn Fn(&mut S, &str)>;
type HoverFn<S> = Rc<dyn Fn(&mut S, &str)>;
type NavigateFn<S> = Rc<dyn Fn(&mut S, &str, bool)>;

/// The menu bar. The host owns the menus (built from the settings theme)
/// and the open-menu id; the component renders the bar and routes the
/// actions.
pub struct MenuBar<'a, S: 'static> {
    menus: &'a [MenuBarMenu],
    open_menu: Option<&'a str>,
    on_open: Option<OpenFn<S>>,
    on_select: Option<SelectFn<S>>,
    on_close: Option<ActionFn<S>>,
    on_hover: Option<HoverFn<S>>,
    on_navigate: Option<NavigateFn<S>>,
}

impl<'a, S: 'static> MenuBar<'a, S> {
    /// The bar over the host's menus and open id.
    pub fn new(menus: &'a [MenuBarMenu], open_menu: Option<&'a str>) -> Self {
        MenuBar {
            menus,
            open_menu,
            on_open: None,
            on_select: None,
            on_close: None,
            on_hover: None,
            on_navigate: None,
        }
    }

    /// A bar button was pressed while its menu was closed: open it (the
    /// host writes the open-menu id).
    pub fn on_open(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_open = Some(Rc::new(f));
        self
    }

    /// One leaf item was selected: run the host's action (React runs
    /// `handleMenuAction` then closes — the kit menu's close_on_select
    /// owns the close half).
    pub fn on_select(self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_action(move |s, _, _| f(s))
    }

    /// Deliver the selected leaf's real action id, including submenu leaves.
    pub fn on_select_action(mut self, f: impl Fn(&mut S, &str, &mut Tx) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// The open menu closed (Escape / outside press / a selection).
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// Hover over a closed bar button while a menu is open: switch to the
    /// hovered menu (React's `onHover`).
    pub fn on_hover(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_hover = Some(Rc::new(f));
        self
    }

    /// Left/Right over the bar buttons (React's `navigateMenu`): the host
    /// writes the next id (the wrapper's focus group moves the focus).
    pub fn on_navigate(
        mut self,
        f: impl Fn(&mut S, &str, bool) + 'static,
    ) -> Self {
        self.on_navigate = Some(Rc::new(f));
        self
    }

    /// Builds the bar. The open menu's dropdown renders into `layer`
    /// through the T0-07 wrapper; the returned element is the bar row.
    pub fn build(self, _s: &S, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let mut bar = row()
            .role("menubar")
            .key("menu-bar")
            .items_center()
            .gap(2.0)
            .label(t("menu.appName"));

        for menu in self.menus {
            let open = self.open_menu == Some(menu.id.as_str());
            let key = format!("menu-bar-{}", menu.id);
            let menu_id = menu.id.clone();
            let on_open = self.on_open.clone();
            let on_close = self.on_close.clone();

            // The trigger: React's `rounded px-2 py-0.5 text-xs` button.
            let mut button = row()
                .role("menuitem")
                .key(format!("{key}-trigger"))
                .radius(k.radius.sm)
                .px(8.0)
                .py(2.0)
                .focusable(true)
                .label(menu.label.clone())
                .expanded(open)
                .hover(k.c.surface_raised)
                .on_press(move |s: &mut S| {
                    if open {
                        if let Some(close) = on_close.clone() {
                            close(s);
                        }
                    } else if let Some(open) = on_open.clone() {
                        open(s, &menu_id);
                    }
                });

            // The hover switch: only while another menu is open (React's
            // `if (openMenu)` guard) — a plain hover does nothing.
            if let Some(hover) = self.on_hover.clone() {
                let any_open = self.open_menu.is_some();
                let menu_id = menu.id.clone();
                button = button.on_hover(move |s: &mut S| {
                    if any_open {
                        hover(s, &menu_id);
                    }
                });
            }

            // Keyboard entry and adjacent-trigger navigation reuse the
            // actual menu model; the popup still owns vertical focus walking.
            let navigate = self.on_navigate.clone();
            let on_open = self.on_open.clone();
            let menu_id = menu.id.clone();
            let forwards = next_menu_id(self.menus, &menu_id, true);
            let backwards = next_menu_id(self.menus, &menu_id, false);
            button = button.on_key(move |s: &mut S, ev: &rune::KeyEvent, tx: &mut Tx| {
                match ev.key {
                    rune::Key::Down if !open => {
                        if let Some(on_open) = &on_open {
                            on_open(s, &menu_id);
                            return true;
                        }
                    }
                    rune::Key::Right | rune::Key::Left => {
                        let forward = matches!(ev.key, rune::Key::Right);
                        if let Some(navigate) = &navigate {
                            navigate(s, &menu_id, forward);
                            if let Some(next) = if forward { &forwards } else { &backwards } {
                                tx.focus(format!("menu-bar-{next}-trigger"), true);
                            }
                            return true;
                        }
                    }
                    _ => {}
                }
                false
            });

            let trigger_text = text(menu.label.clone())
                .font(k.ty.text_xsmall)
                .color(if open {
                    k.c.text_base
                } else {
                    k.c.text_muted
                });
            button = button.child(trigger_text);
            // React's open trigger paints `bg-surface-raised text-fg`.
            let button = button.fill(if open {
                k.c.surface_raised
            } else {
                rune::Color::TRANSPARENT
            });

            // The dropdown itself: the T0-07 wrapper anchored under the
            // trigger, only while open.
            let mut dropdown = Menu::<S>::new(key)
                .trigger(button)
                .open(open)
                .placement(Placement::BottomStart)
                .gutter(2.0)
                .min_width(200.0);
            if let Some(close) = self.on_close.clone() {
                dropdown = dropdown.on_close(move |s: &mut S| close(s));
            }
            for item in &menu.items {
                if item.separator {
                    dropdown = dropdown.separator();
                } else {
                    dropdown = dropdown.item(self.entry(item));
                }
            }
            bar = bar.child(dropdown.build(&k, layer));
        }
        bar
    }

    /// One `MenuEntry` → one wrapper item (separator / submenu / leaf with
    /// the checked radio and the shortcut).
    fn entry(&self, item: &MenuBarItem) -> crate::ui::menu::Item<S> {
        let mut entry = crate::ui::menu::Item::<S>::new(item.label.clone());
        if let Some(shortcut) = &item.shortcut {
            entry = entry.shortcut(shortcut.clone());
        }
        if let Some(checked) = item.checked {
            // React's `menuitemradio` + the accent `✓`: the wrapper's
            // checked entry (unchecked rows keep the slot, exactly the
            // `w-4 text-center` empty span).
            entry = entry.checked(checked);
        }
        if !item.children.is_empty() {
            let children: Vec<crate::ui::menu::Entry<S>> = item
                .children
                .iter()
                .map(|child| self.subentry(child))
                .collect();
            entry = entry.submenu(children, false);
            return entry;
        }
        if let Some(select) = self.on_select.clone() {
            let id = item.id.clone();
            entry = entry.on_select_tx(move |s: &mut S, tx| select(s, &id, tx));
        }
        entry
    }

    /// A submenu child (no deeper nesting in React's table).
    fn subentry(&self, item: &MenuBarItem) -> crate::ui::menu::Entry<S> {
        if item.separator {
            return crate::ui::menu::Entry::Separator;
        }
        let mut entry = crate::ui::menu::Item::<S>::new(item.label.clone());
        if let Some(shortcut) = &item.shortcut {
            entry = entry.shortcut(shortcut.clone());
        }
        if let Some(checked) = item.checked {
            entry = entry.checked(checked);
        }
        if let Some(select) = self.on_select.clone() {
            let id = item.id.clone();
            entry = entry.on_select_tx(move |s: &mut S, tx| select(s, &id, tx));
        }
        crate::ui::menu::Entry::Item(entry)
    }
}
