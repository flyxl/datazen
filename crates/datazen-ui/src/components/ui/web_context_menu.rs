//! The web context-menu host, ported from
//! `packages/frontend/src/components/ui/WebContextMenu.tsx` (241 lines, T096).
//!
//! React renders `WebContextMenuHost` into `document.body` and reads its
//! open state from the zustand `contextMenuStore`. This port carries the
//! whole vertical in one module, because the web component is the only
//! consumer of these contracts and the store task (T157) owns the host-side
//! wiring that will reuse this model:
//!
//! * the item contract (`NativeMenuItemDef`, `NativeMenuPredefined`) from
//!   `packages/driver-sdk/src/types/menu.ts`;
//! * `normalizeNativeMenuItems` from
//!   `packages/driver-sdk/src/nativeContextMenu.ts` — drop empty submenus
//!   and leading/trailing/duplicate separators, keep disabled items;
//! * the position math (`positionRootMenu`, `positionSubmenu`) from
//!   `lib/contextMenuPosition.ts` — flip left/up so panels stay in view;
//! * the store shape (`show`/`hide`, open at the cursor) from
//!   `stores/contextMenuStore.ts`;
//! * the host itself: open state renders two absolutely positioned panels
//!   plus a dismissal surface, closed renders nothing (React `null`).
//!
//! ## Key convention
//!
//! React's `data-testid`s become the element keys, and the React list keys
//! that are not test ids become the separator keys:
//!
//! | part | key |
//! |---|---|
//! | root panel | `web-context-menu` |
//! | submenu panel | `web-context-submenu` |
//! | plain item `id` | `web-context-item-{id}` |
//! | predefined `Cut`/`Copy`/… | `web-context-pre-{item}` |
//! | submenu trigger | `web-context-submenu-trigger-{id}` |
//! | separator (with `id`) | `{id}` |
//! | separator without `id` | `sep-{index}` / `pre-sep-{index}` |
//! | dismissal surface | `web-context-scrim` (new, no React id) |
//!
//! ## Deliberate boundaries
//!
//! * **Positioning is explicit, not measured.** React measures
//!   `offsetWidth`/`offsetHeight` in layout effects; the port takes the
//!   measured sizes as inputs (`WebContextMenuState::menu_size`,
//!   `submenu_size`, `viewport`) so the flip/clamp math is pure and
//!   testable without a layout pass. The production wiring measures the
//!   same way React does — after a frame.
//! * **Submenus open on press, not hover.** React opens a submenu when the
//!   trigger is hovered or focused (`onMouseEnter`/`onFocus`). Rune has no
//!   hover callbacks; the trigger opens on press, which is the native-menu
//!   affordance the kit's own menus use. The plain-item hover that closes a
//!   submenu has the same fate: it is covered by outside presses.
//! * **The trigger rect comes from the host.** React reads
//!   `e.currentTarget.getBoundingClientRect()`; the port's
//!   [`WebContextMenuHost::on_open_submenu`] hands the host state and the
//!   trigger id to the caller, who resolves the rect (from the press point
//!   or its own layout bookkeeping) and runs
//!   [`WebContextMenuState::on_item_enter`] — the same boundary, one step
//!   over.
//! * **Predefined edit items dispatch to the host.** React's
//!   `runPredefinedEdit` calls `document.execCommand('cut')`; there is no
//!   DOM here, so the command surfaces through
//!   [`WebContextMenuHost::on_predefined`] — the same command ids React
//!   maps them onto (`cut`/`copy`/`paste`/`selectAll`/`undo`/`redo`). With
//!   no handler the menu still closes, matching React's `hide()` running
//!   before `runPredefinedEdit`.
//! * **Dismissal.** React listens for window `Escape` and outside
//!   `mousedown`; the port renders a transparent full-window press target
//!   (`web-context-scrim`) under the panels and puts the root panel on the
//!   modal stack, whose Escape dismissal runs the same hide — the kit menu
//!   and dialog precedent. The `resize`/`scroll`/`visibilitychange` closes
//!   are viewport wiring, not component behavior.
//! * **No `z-[10000]`.** Rune layers paint in order; the caller mounts
//!   this host last on its overlay list.

// The production consumer is the wiring track (T157 and the grid context
// menus); until then the component tests are the only callers, so the
// dead-code lint is silenced here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, el, icon, row, text, El};

use crate::icons::ICON_CHEVRON_RIGHT;
use crate::ui::kit::kit;

/// The predefined edit items (`packages/driver-sdk/src/types/menu.ts`).
///
/// `Separator` is a predefined kind in React's contract (it renders as a
/// separator row, see [`is_separator`]); the edit commands are the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredefinedEdit {
    Separator,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Undo,
    Redo,
}

impl PredefinedEdit {
    /// The React name of the predefined item (`item.item`).
    pub fn as_str(self) -> &'static str {
        match self {
            PredefinedEdit::Separator => "Separator",
            PredefinedEdit::Cut => "Cut",
            PredefinedEdit::Copy => "Copy",
            PredefinedEdit::Paste => "Paste",
            PredefinedEdit::SelectAll => "SelectAll",
            PredefinedEdit::Undo => "Undo",
            PredefinedEdit::Redo => "Redo",
        }
    }

    /// React's `COMMANDS` map (`lib/runPredefinedEdit.ts`): the
    /// `document.execCommand` id each predefined edit maps onto. `Separator`
    /// has no command (React returns before looking one up).
    pub fn command(self) -> Option<&'static str> {
        match self {
            PredefinedEdit::Separator => None,
            PredefinedEdit::Cut => Some("cut"),
            PredefinedEdit::Copy => Some("copy"),
            PredefinedEdit::Paste => Some("paste"),
            PredefinedEdit::SelectAll => Some("selectAll"),
            PredefinedEdit::Undo => Some("undo"),
            PredefinedEdit::Redo => Some("redo"),
        }
    }
}

/// A plain item's action (`() => void | Promise<void>` in React).
pub type NativeAction = Rc<dyn Fn()>;

impl std::fmt::Debug for NativeMenuItemDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The action is a closure; it prints as its kind's identifying
        // fields only (React's def is a plain object with a function).
        match self {
            NativeMenuItemDef::Item { id, label, shortcut, enabled, .. } => f
                .debug_struct("Item")
                .field("id", id)
                .field("label", label)
                .field("shortcut", shortcut)
                .field("enabled", enabled)
                .finish(),
            NativeMenuItemDef::Separator { id } => {
                f.debug_struct("Separator").field("id", id).finish()
            }
            NativeMenuItemDef::Predefined { item, text } => f
                .debug_struct("Predefined")
                .field("item", item)
                .field("text", text)
                .finish(),
            NativeMenuItemDef::Submenu { id, label, items } => f
                .debug_struct("Submenu")
                .field("id", id)
                .field("label", label)
                .field("items", items)
                .finish(),
        }
    }
}

/// One row of the menu (`packages/driver-sdk/src/types/menu.ts`,
/// `NativeMenuItemDef`). The four React kinds become variants; the
/// discriminator is `kind` there and the variant here. `Clone` is the
/// store clone React's items arrays get by sharing (`Rc` around the
/// action); `Debug` drops the closure, see the manual impl below.
#[derive(Clone)]
pub enum NativeMenuItemDef {
    /// `kind: 'item'` — `id`, `label`, optional `shortcut` and `enabled`,
    /// and the `action` that runs when the row is chosen.
    Item {
        id: String,
        label: String,
        shortcut: Option<String>,
        enabled: Option<bool>,
        action: NativeAction,
    },
    /// `kind: 'separator'` — optional `id`.
    Separator { id: Option<String> },
    /// `kind: 'predefined'` — an OS edit item, optionally relabelled.
    Predefined {
        item: PredefinedEdit,
        text: Option<String>,
    },
    /// `kind: 'submenu'` — optional `id`, a `label`, nested `items`.
    Submenu {
        id: Option<String>,
        label: String,
        items: Vec<NativeMenuItemDef>,
    },
}

/// React's `isSeparator` (`WebContextMenu.tsx:18`): separators are both the
/// explicit kind and the predefined `Separator`.
pub fn is_separator(item: &NativeMenuItemDef) -> bool {
    matches!(item, NativeMenuItemDef::Separator { .. })
        || matches!(
            item,
            NativeMenuItemDef::Predefined {
                item: PredefinedEdit::Separator,
                ..
            }
        )
}

impl NativeMenuItemDef {
    /// A plain item or submenu is enabled unless it says otherwise; the
    /// other kinds render no disabled state in React.
    pub fn enabled(&self) -> bool {
        match self {
            NativeMenuItemDef::Item { enabled, .. } => *enabled != Some(false),
            _ => true,
        }
    }

    /// The React item id a submenu resolves to: `item.id ?? 'sub-{index}'`
    /// (`WebContextMenu.tsx:65`, `:202`).
    pub fn submenu_id(&self, index: usize) -> Option<String> {
        match self {
            NativeMenuItemDef::Submenu { id, .. } => {
                Some(id.clone().unwrap_or_else(|| format!("sub-{index}")))
            }
            _ => None,
        }
    }

    /// React's `itemLabel` (`WebContextMenu.tsx:22`): the item or submenu
    /// `label`, or the predefined `text ?? item`. Separators have none.
    pub fn item_label(&self) -> Option<String> {
        match self {
            NativeMenuItemDef::Item { label, .. } => Some(label.clone()),
            NativeMenuItemDef::Submenu { label, .. } => Some(label.clone()),
            NativeMenuItemDef::Predefined { item, text } => {
                Some(text.clone().unwrap_or_else(|| item.as_str().to_string()))
            }
            NativeMenuItemDef::Separator { .. } => None,
        }
    }

    /// The submenu's children, for the submenu panel.
    pub fn submenu_items(&self) -> Option<&[NativeMenuItemDef]> {
        match self {
            NativeMenuItemDef::Submenu { items, .. } => Some(items),
            _ => None,
        }
    }
}

/// `normalizeNativeMenuItems` (`nativeContextMenu.ts:37`): recursively drop
/// empty submenus, then leading, trailing and duplicate separators.
/// Disabled items are kept.
pub fn normalize_native_menu_items(items: Vec<NativeMenuItemDef>) -> Vec<NativeMenuItemDef> {
    let filtered: Vec<NativeMenuItemDef> = items
        .into_iter()
        .filter_map(|mut def| {
            if let NativeMenuItemDef::Submenu { items, .. } = &mut def {
                *items = normalize_native_menu_items(std::mem::take(items));
                if items.is_empty() {
                    return None;
                }
            }
            Some(def)
        })
        .collect();

    let mut out: Vec<NativeMenuItemDef> = Vec::with_capacity(filtered.len());
    for def in filtered {
        if is_separator(&def) {
            // Leading separators are dropped; a separator right after
            // another separator is dropped too.
            if out.is_empty() || is_separator(out.last().expect("checked non-empty")) {
                continue;
            }
        }
        out.push(def);
    }
    while out.last().is_some_and(is_separator) {
        out.pop();
    }
    out
}

/// The padding the position math keeps between a panel and the viewport
/// (`CONTEXT_MENU_PAD`, `contextMenuPosition.ts:23`).
pub const CONTEXT_MENU_PAD: f32 = 8.0;

/// React's `clamp` (`contextMenuPosition.ts:25`): when `max < min` the min
/// wins (a viewport smaller than the menu), otherwise the ordinary clamp.
fn clamp(n: f32, min: f32, max: f32) -> f32 {
    if max < min {
        return min;
    }
    n.min(max).max(min)
}

/// `positionRootMenu` (`contextMenuPosition.ts:34`): place the root menu at
/// the cursor, flipping left and up when it would overflow the viewport,
/// then clamping to the padded viewport.
pub fn position_root_menu(
    cursor: (f32, f32),
    menu: (f32, f32),
    viewport: (f32, f32),
    pad: f32,
) -> (f32, f32) {
    let (cursor_x, cursor_y) = cursor;
    let (menu_w, menu_h) = menu;
    let (viewport_w, viewport_h) = viewport;

    let mut left = cursor_x;
    let mut top = cursor_y;
    if left + menu_w > viewport_w - pad {
        left = cursor_x - menu_w;
    }
    left = clamp(left, pad, viewport_w - pad - menu_w);
    if top + menu_h > viewport_h - pad {
        top = cursor_y - menu_h;
    }
    top = clamp(top, pad, viewport_h - pad - menu_h);
    (left, top)
}

/// Which side of its trigger a submenu landed on
/// (`positionSubmenu`'s `side`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmenuSide {
    Right,
    Left,
}

/// `positionSubmenu` (`contextMenuPosition.ts:60`): place a submenu beside
/// its parent item, preferring the right side and flipping only when the
/// right side clips and the left has more room; vertically aligned to the
/// item, then clamped.
pub fn position_submenu(
    item: (f32, f32, f32, f32),
    submenu: (f32, f32),
    viewport: (f32, f32),
    pad: f32,
) -> (f32, f32, SubmenuSide) {
    let (item_left, item_top, item_w, _item_h) = item;
    let (sub_w, sub_h) = submenu;
    let (viewport_w, viewport_h) = viewport;

    let item_right = item_left + item_w;
    let space_right = viewport_w - pad - item_right;
    let space_left = item_left - pad;
    let side = if space_right >= sub_w || space_right >= space_left {
        SubmenuSide::Right
    } else {
        SubmenuSide::Left
    };

    let mut left = match side {
        SubmenuSide::Right => item_right,
        SubmenuSide::Left => item_left - sub_w,
    };
    left = clamp(left, pad, viewport_w - pad - sub_w);

    let mut top = item_top;
    if top + sub_h > viewport_h - pad {
        top = viewport_h - pad - sub_h;
    }
    top = clamp(top, pad, viewport_h - pad - sub_h);
    (left, top, side)
}

/// The submenu React opens on trigger enter/focus: which def is open, its
/// rows, the trigger's rect, and where the panel is drawn
/// (`WebContextMenu.tsx:126`).
#[derive(Clone, Debug)]
pub struct OpenSubmenu {
    pub id: String,
    pub items: Vec<NativeMenuItemDef>,
    /// `(left, top, width, height)` of the trigger row.
    pub item_rect: (f32, f32, f32, f32),
    /// The panel position: `(rect.right, rect.top)` first, then refined by
    /// [`WebContextMenuState::refine_submenu`].
    pub pos: (f32, f32),
}

/// The store + host state: React's zustand `contextMenuStore` fields (`open`,
/// `x`, `y`, `items`) plus the component's own root-position and submenu
/// state, with the panel sizes the layout effects measure.
#[derive(Debug, Default)]
pub struct WebContextMenuState {
    pub open: bool,
    pub x: f32,
    pub y: f32,
    pub items: Vec<NativeMenuItemDef>,
    pub submenu: Option<OpenSubmenu>,
    /// The measured root panel size; React falls back to `180 × 40` before
    /// the first measure (`WebContextMenu.tsx:139`).
    pub menu_size: (f32, f32),
    /// The measured submenu panel size; React falls back to `160 × 40`
    /// (`WebContextMenu.tsx:153`).
    pub submenu_size: (f32, f32),
    /// The viewport the position math clamps against
    /// (`window.innerWidth/innerHeight` in React).
    pub viewport: (f32, f32),
}

impl WebContextMenuState {
    /// The store's `show` (`contextMenuStore.ts:19`): normalize, and an
    /// empty result keeps the menu closed rather than opening an empty
    /// panel; a real menu opens at the cursor.
    pub fn show(&mut self, items: Vec<NativeMenuItemDef>, pos: (f32, f32)) {
        let normalized = normalize_native_menu_items(items);
        if normalized.is_empty() {
            self.open = false;
            self.items.clear();
            return;
        }
        self.open = true;
        self.x = pos.0;
        self.y = pos.1;
        self.items = normalized;
        // React clears `sub` when the menu re-opens (the layout effect
        // sets it null whenever `open` flips).
        self.submenu = None;
    }

    /// The store's `hide` (`contextMenuStore.ts:27`): `open: false` and the
    /// items go; the submenu closes with the host (`useLayoutEffect`).
    pub fn hide(&mut self) {
        self.open = false;
        self.items.clear();
        self.submenu = None;
    }

    /// The first layout effect (`WebContextMenu.tsx:133`): where the root
    /// panel sits — the cursor, flipped and clamped against the measured
    /// menu size and the viewport.
    pub fn root_position(&self) -> (f32, f32) {
        position_root_menu(
            (self.x, self.y),
            self.menu_size,
            self.viewport,
            CONTEXT_MENU_PAD,
        )
    }

    /// The second layout effect (`WebContextMenu.tsx:150`): refine the open
    /// submenu's position against the measured submenu size, but only when
    /// the refined position actually differs (React's `setSub` guard).
    pub fn refine_submenu(&mut self) {
        let Some(sub) = self.submenu.as_mut() else {
            return;
        };
        let (left, top, _side) = position_submenu(
            sub.item_rect,
            self.submenu_size,
            self.viewport,
            CONTEXT_MENU_PAD,
        );
        if (left, top) != sub.pos {
            sub.pos = (left, top);
        }
    }

    /// React's `onItemEnter` (`WebContextMenu.tsx:196`): an empty id closes
    /// any open submenu; a known submenu def opens beside the given rect
    /// (positioned at `(rect.right, rect.top)` until refined); an unknown
    /// id closes it.
    pub fn on_item_enter(&mut self, id: &str, item_rect: (f32, f32, f32, f32)) {
        if id.is_empty() {
            self.submenu = None;
            return;
        }
        let Some(def) = self
            .items
            .iter()
            .enumerate()
            .find(|(index, def)| def.submenu_id(*index).is_some_and(|sid| sid == id))
            .map(|(_index, def)| def)
        else {
            self.submenu = None;
            return;
        };
        let Some(items) = def.submenu_items() else {
            self.submenu = None;
            return;
        };
        self.submenu = Some(OpenSubmenu {
            id: id.to_string(),
            items: items.to_vec(),
            item_rect,
            pos: (item_rect.0 + item_rect.2, item_rect.1),
        });
    }
}

/// The host (`WebContextMenuHost`): mounts the panels React portals to
/// `document.body`. Closed — including a never-shown state — mounts
/// nothing, exactly React's `null`.
///
/// `S` is the host state the callbacks mutate; `dark` is taken per mount.
pub struct WebContextMenuHost<'a, S: 'static> {
    state: &'a WebContextMenuState,
    /// The accessor into the host state, like
    /// `AdminCreateDialog::host_mut`: hide and submenu-open mutations run
    /// through it, so the component never owns the store.
    host: Option<Rc<dyn Fn(&mut S) -> &mut WebContextMenuState>>,
    /// Where the predefined edit commands are delivered — the honest
    /// equivalent of React's `document.execCommand`.
    on_predefined: Option<Rc<dyn Fn(&mut S, PredefinedEdit)>>,
    /// Where a submenu trigger's rect is resolved: the caller receives the
    /// host state and the trigger id and runs
    /// `WebContextMenuState::on_item_enter` with the rect it knows.
    on_open_submenu: Option<Rc<dyn Fn(&mut S, &str)>>,
}

impl<'a, S: 'static> WebContextMenuHost<'a, S> {
    pub fn new(state: &'a WebContextMenuState) -> Self {
        Self {
            state,
            host: None,
            on_predefined: None,
            on_open_submenu: None,
        }
    }

    /// The state accessor, so presses can hide and open submenus without
    /// the component owning the host state.
    pub fn host_mut(mut self, f: impl Fn(&mut S) -> &mut WebContextMenuState + 'static) -> Self {
        self.host = Some(Rc::new(f));
        self
    }

    /// The predefined-command channel (React's `document.execCommand`).
    pub fn on_predefined(
        mut self,
        f: impl Fn(&mut S, PredefinedEdit) + 'static,
    ) -> Self {
        self.on_predefined = Some(Rc::new(f));
        self
    }

    /// The submenu-open channel: resolve the trigger's rect and run
    /// `on_item_enter` (React reads the rect off the DOM element).
    pub fn on_open_submenu(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_open_submenu = Some(Rc::new(f));
        self
    }

    /// Mount the open menu onto the overlay list: the dismissal surface,
    /// then the root panel, then the open submenu (later siblings paint on
    /// top). Closed mounts nothing.
    pub fn mount(self, dark: bool, layer: &mut Vec<El>) {
        if !self.state.open {
            return;
        }
        let k = kit(dark);
        let host = self.host.clone();
        let escape_host = self.host.clone();

        // React's window `mousedown` listener: presses outside both panels
        // close the menu. A transparent full-window press target under the
        // panels is the Rune shape (the kit scrim).
        let scrim = {
            let outside = host.clone();
            el()
                .key("web-context-scrim")
                .role("none")
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .on_press(move |s: &mut S| {
                    if let Some(host) = &outside {
                        host(s).hide();
                    }
                })
        };

        // React's window `keydown` (`Escape`): the modal stack's dismissal.
        let root = menu_panel::<S>(
            &k,
            "web-context-menu",
            self.state.root_position(),
            &self.state.items,
            &host,
            &self.on_open_submenu,
            &self.on_predefined,
            self.state.submenu.as_ref().map(|sub| sub.id.as_str()),
        )
        .modal(true, move |s: &mut S| {
            if let Some(host) = &escape_host {
                host(s).hide();
            }
        });

        layer.push(scrim);
        layer.push(root);

        if let Some(sub) = &self.state.submenu {
            let submenu = menu_panel::<S>(
                &k,
                "web-context-submenu",
                sub.pos,
                &sub.items,
                &host,
                &self.on_open_submenu,
                &self.on_predefined,
                None,
            );
            layer.push(submenu);
        }
    }
}

/// One `MenuPanel` (`WebContextMenu.tsx:29`): a rounded, bordered panel on
/// the surface-alt fill with `py-1`, `min-w-[180px]`, `shadow-xl`, one row
/// per item. The open submenu id highlights that submenu's trigger.
fn menu_panel<S: 'static>(
    k: &rune_kit::Kit,
    key: &'static str,
    pos: (f32, f32),
    items: &[NativeMenuItemDef],
    host: &Option<Rc<dyn Fn(&mut S) -> &mut WebContextMenuState>>,
    on_open_submenu: &Option<Rc<dyn Fn(&mut S, &str)>>,
    on_predefined: &Option<Rc<dyn Fn(&mut S, PredefinedEdit)>>,
    open_sub_id: Option<&str>,
) -> El {
    let mut panel = col()
        .key(key)
        .role("menu")
        .absolute(pos.0, pos.1)
        .min_w(180.0)
        .py(4.0)
        .radius(k.radius.lg)
        .border(1.0, k.c.border_base)
        .fill(k.c.layer_01)
        .items_stretch();
    for (index, item) in items.iter().enumerate() {
        panel = panel.child(menu_row::<S>(
            k,
            item,
            index,
            host,
            on_open_submenu,
            on_predefined,
            open_sub_id,
        ));
    }
    // React's `shadow-xl`; the dialog base maps the same class onto
    // `k.elevation.overlay` (`ui/dialog.rs:394`).
    k.elevate(panel, &k.elevation.overlay)
}

/// One row of a panel: separator, submenu trigger, or item
/// (`WebContextMenu.tsx:54`).
fn menu_row<S: 'static>(
    k: &rune_kit::Kit,
    item: &NativeMenuItemDef,
    index: usize,
    host: &Option<Rc<dyn Fn(&mut S) -> &mut WebContextMenuState>>,
    on_open_submenu: &Option<Rc<dyn Fn(&mut S, &str)>>,
    on_predefined: &Option<Rc<dyn Fn(&mut S, PredefinedEdit)>>,
    open_sub_id: Option<&str>,
) -> El {
    // Separators: `my-1 h-px bg-edge`, keyed by the separator id or the
    // fallback React list keys. React's separator is a direct child of the
    // panel (which has no horizontal padding), so it spans the panel's
    // content width — no negative margins, unlike the kit menus whose list
    // has horizontal padding to cancel.
    if is_separator(item) {
        let key = match item {
            NativeMenuItemDef::Separator { id: Some(id) } => id.clone(),
            NativeMenuItemDef::Separator { id: None } => format!("sep-{index}"),
            // The predefined Separator: `pre-sep-{index}`.
            _ => format!("pre-sep-{index}"),
        };
        return el()
            .key(key)
            .role("separator")
            .h(1.0)
            .mt(4.0)
            .mb(4.0)
            .fill(k.c.border_base)
            .a11y_hidden();
    }

    // Submenu triggers: the label and a chevron. Hovering/focusing opens in
    // React; pressing opens here. The open trigger keeps the raised fill
    // React gives it (`openSubmenuId === id`).
    if let Some(trigger_id) = item.submenu_id(index) {
        let label = item.item_label().unwrap_or_default();
        let open = open_sub_id == Some(trigger_id.as_str());
        let mut trigger = row()
            .key(format!("web-context-submenu-trigger-{trigger_id}"))
            .role("menuitem")
            .label(label.clone())
            .w_full()
            .items_center()
            .justify_between()
            .gap(12.0)
            .px(12.0)
            .py(6.0);
        if open {
            trigger = trigger.fill(k.c.surface_raised);
        }
        trigger = trigger
            .child(
                text(label)
                    .key("label")
                    .font(k.ty.body)
                    .color(k.c.text_base)
                    .min_w(0.0),
            )
            .child(
                icon(&ICON_CHEVRON_RIGHT, 14.0)
                    .key("chevron")
                    .color(k.c.text_muted)
                    .a11y_hidden(),
            );
        if let Some(on_open) = on_open_submenu {
            let on_open = Rc::clone(on_open);
            let id = trigger_id.clone();
            return trigger.on_press(move |s: &mut S| on_open(s, &id));
        }
        return trigger;
    }

    // Plain items and predefined rows: label plus a shortcut (plain only),
    // disabled rows muted at half opacity and pressless.
    let enabled = item.enabled();
    let label = item.item_label().unwrap_or_default();
    let (row_key, shortcut) = match item {
        NativeMenuItemDef::Item { id, shortcut, .. } => {
            (format!("web-context-item-{id}"), shortcut.clone())
        }
        NativeMenuItemDef::Predefined { item, .. } => {
            (format!("web-context-pre-{}", item.as_str()), None)
        }
        _ => (format!("row-{index}"), None),
    };
    let mut item_row = row()
        .key(row_key)
        .role("menuitem")
        .label(label.clone())
        .w_full()
        .items_center()
        .justify_between()
        .gap(16.0)
        .px(12.0)
        .py(6.0)
        .child(
            text(label)
                .key("label")
                .font(k.ty.body)
                .color(if enabled {
                    k.c.text_base
                } else {
                    k.c.text_muted
                })
                .min_w(0.0),
        );
    if let Some(shortcut) = shortcut {
        item_row = item_row.child(
            text(shortcut)
                .key("shortcut")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .a11y_hidden(),
        );
    }
    if !enabled {
        // React: `cursor-not-allowed text-fg-muted opacity-50`, and the
        // click handler returns early.
        return item_row.disabled(true).layer_opacity(0.5);
    }
    // Enabled: pressing hides the menu first, then runs the item's action —
    // React's `onClick` order (`hide(); void item.action()`).
    match item {
        NativeMenuItemDef::Item { action, .. } => {
            let action = Rc::clone(action);
            let hide = hide_fn(host);
            item_row.on_press(move |s: &mut S| {
                if let Some(hide) = &hide {
                    hide(s);
                }
                action();
            })
        }
        NativeMenuItemDef::Predefined { item, .. } => {
            let item = *item;
            let on_predefined = on_predefined.clone();
            let hide = hide_fn(host);
            item_row.on_press(move |s: &mut S| {
                if let Some(hide) = &hide {
                    hide(s);
                }
                if let Some(on_predefined) = &on_predefined {
                    on_predefined(s, item);
                }
            })
        }
        _ => item_row,
    }
}

/// The hide closure shared by presses and dismissal, through the state
/// accessor. No accessor (the unwired host) presses nothing, like the
/// bare-dialog coverage in `tests/admin_create_dialog.rs`.
fn hide_fn<S: 'static>(host: &Option<Rc<dyn Fn(&mut S) -> &mut WebContextMenuState>>) -> Option<Rc<dyn Fn(&mut S)>> {
    host.as_ref().map(|host| {
        let host = Rc::clone(host);
        Rc::new(move |s: &mut S| {
            host(s).hide();
        }) as Rc<dyn Fn(&mut S)>
    })
}
