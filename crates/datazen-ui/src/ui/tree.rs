//! The DataZen tree: a virtualised outline over a [`TreeModel`] the app keeps in its state.
//!
//! `rune_kit::Tree` cannot carry DataZen's key contract: its view hardcodes the row container
//! (`rows`), its viewport (`viewport`) and every row's key (`<node id>`), so a wrapper cannot
//! name `<key>-row-<i>` for the row at visible position `i`. The model, however, is exactly
//! what DataZen needs: it owns the nodes, the open state, the selection, the keyboard cursor
//! and the visible-row projection, and it is independent of any view. This base therefore
//! wraps `TreeModel` (one focus stop, kit keyboard semantics, virtual rows) and draws the rows
//! itself on [`TreeStyle`] metrics, adding DataZen's positional keys.
//!
//! Kept from the kit: the model's keyboard (Up/Down, Right opens or enters the first child,
//! Left closes or goes to the parent, Home/End, Page Up/Page Down, Return/Space opens, typing
//! jumps), `multi_select` with Shift to extend and Cmd-A, `reveal`, filtering, the tree row
//! metrics (`row_height` 24, `indent` 14, `padding` 6, indent guides), the selected fill and
//! `virtual_rows`, so a 500-node tree mounts only the window it shows.
//!
//! Not carried: the kit's `TreeItem` extras (change counts, badges, dots, trailing elements),
//! drag and drop, and the `Tree`'s own on-select/on-open plumbing. Rows here show an optional
//! icon and a label; richer rows are a later base.
//!
//! | part         | key                          |
//! |--------------|------------------------------|
//! | container    | `<key>`                      |
//! | viewport     | `<key>-viewport`             |
//! | row list     | `<key>-rows`                 |
//! | row          | `<key>-row-<i>`              |
//! | indent guide | `<key>-row-<i>-guide-<level>`|
//! | chevron      | `<key>-row-<i>-chevron`      |
//! | icon         | `<key>-row-<i>-icon`         |
//! | label        | `<key>-row-<i>-label`        |
//! | empty state  | `<key>-empty`                |
//!
//! `<i>` is the row's visible position, so it changes when nodes expand, collapse or filter:
//! a handler that outlives a frame must find its node by id (the model's cursor and selection
//! are kept by id), never by index. The container's key is the one given to
//! [`DataTree::new`]; give the model the same key with [`TreeModel::key`] so the app's own
//! `reveal` and this view agree.
//!
//! Keyboard: the tree is one focus stop (its rows never join the tab order). Up and Down move
//! the cursor and select, Right opens a folder or steps into its first child, Left closes or
//! returns to the parent, Home/End go to the ends, Page Up/Page Down move by the viewport's
//! page, Return/Space open a leaf through `on_open` and toggle a folder. A pointer press
//! selects and focuses a row, and on a folder it also toggles it (the kit's file-tree
//! behaviour); a double press opens a leaf.

// The views that call this land in later migration tasks; `tests/tree.rs` is the only caller
// today, so the lib target would otherwise warn about every item.
#![allow(dead_code)]

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::rc::Rc;

use rune::{col, el, icon, row, text, El, GrowthSpec, Icon, Key, KeyEvent, ScrollSpec, Tx};
use rune_kit::{TreeKey, TreeModel, TreeRow, TreeStyle};

use super::kit::kit;

/// One `on_select` / `on_open` handler: the model already moved before it runs.
type Handler<S> = Rc<dyn Fn(&mut S, &str)>;

thread_local! {
    /// The viewport height each tree key last measured. `TreeModel` keeps its own viewport for
    /// Page Up/Page Down, but that field is private to the kit, so the base records the
    /// measurement here (one UI thread per process, one entry per tree on screen).
    static VIEWPORTS: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
}

/// The viewport height `key` last laid out at, 0 when it has not been laid out yet.
fn viewport_height(key: &str) -> f32 {
    VIEWPORTS.with(|m| m.borrow().get(key).copied().unwrap_or(0.0))
}

/// A tree over a [`TreeModel`] in the app's state `S`, drawing the rows with DataZen's keys.
pub(crate) struct DataTree<S, T> {
    key: String,
    get: fn(&S) -> &TreeModel<T>,
    get_mut: fn(&mut S) -> &mut TreeModel<T>,
    label: fn(&T) -> &str,
    icon: Option<fn(&T) -> Option<&'static Icon>>,
    on_select: Option<Handler<S>>,
    on_open: Option<Handler<S>>,
    style: Option<TreeStyle>,
    name: Option<String>,
    empty_text: Option<String>,
    height: Option<f32>,
    grow: bool,
}

impl<S: 'static, T: 'static> DataTree<S, T> {
    /// `get` and `get_mut` find the model in the state; `label` names a node for its row (and
    /// for typing to jump). `key` names the tree; the model should carry the same key.
    pub(crate) fn new(
        key: impl Into<String>,
        get: fn(&S) -> &TreeModel<T>,
        get_mut: fn(&mut S) -> &mut TreeModel<T>,
        label: fn(&T) -> &str,
    ) -> DataTree<S, T> {
        DataTree {
            key: key.into(),
            get,
            get_mut,
            label,
            icon: None,
            on_select: None,
            on_open: None,
            style: None,
            name: None,
            empty_text: None,
            height: None,
            grow: false,
        }
    }

    /// The glyph before the label (a folder or file icon); rows without one keep the lane.
    pub(crate) fn icon(mut self, f: fn(&T) -> Option<&'static Icon>) -> DataTree<S, T> {
        self.icon = Some(f);
        self
    }

    /// The cursor moved to `id`, by key or pointer.
    pub(crate) fn on_select(mut self, f: impl Fn(&mut S, &str) + 'static) -> DataTree<S, T> {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// A leaf was opened: Return/Space or a double press.
    pub(crate) fn on_open(mut self, f: impl Fn(&mut S, &str) + 'static) -> DataTree<S, T> {
        self.on_open = Some(Rc::new(f));
        self
    }

    pub(crate) fn style(mut self, style: TreeStyle) -> DataTree<S, T> {
        self.style = Some(style);
        self
    }

    /// The tree's accessibility label ("Files").
    pub(crate) fn name(mut self, name: impl Into<String>) -> DataTree<S, T> {
        self.name = Some(name.into());
        self
    }

    /// The text an empty model shows (the table's "no rows" string by default).
    pub(crate) fn empty_text(mut self, text: impl Into<String>) -> DataTree<S, T> {
        self.empty_text = Some(text.into());
        self
    }

    /// A fixed height; by default the tree fills the height its parent gives it.
    pub(crate) fn height(mut self, height: f32) -> DataTree<S, T> {
        self.height = Some(height);
        self
    }

    /// Fill the parent's height. Without either this or [`Self::height`] the tree hugs (and a
    /// virtual viewport with no height shows nothing).
    pub(crate) fn grow(mut self) -> DataTree<S, T> {
        self.grow = true;
        self
    }

    /// Builds the tree. `s` is read for the model; handlers write it back.
    pub(crate) fn build(self, s: &S, dark: bool) -> El {
        let DataTree {
            key,
            get,
            get_mut,
            label,
            icon,
            on_select,
            on_open,
            style,
            name,
            empty_text,
            height,
            grow,
        } = self;
        let k = kit(dark);
        let style = style.unwrap_or_else(|| TreeStyle::from_kit(k));
        let count = get(s).visible_len();
        let empty = empty_text.unwrap_or_else(|| crate::i18n::t("dataTable.empty").to_owned());
        let row_height = style.row_height;

        let rows_key = key.clone();
        let row_select = on_select.clone();
        let row_open = on_open.clone();
        let build_row = move |s: &S, i: usize| -> El {
            let Some((data, r)) = get(s).row(i) else {
                return el();
            };
            let id = r.id.to_owned();
            let openable = r.expanded.is_some();
            let mut e = tree_row(k, &style, &rows_key, i, data, &r, label, icon);
            let press = id.clone();
            let select = row_select.clone();
            e = e.on_press_tx(move |s: &mut S, tx: &mut Tx| {
                let plain = get_mut(s).click(&press, tx.mods());
                if openable && plain {
                    get_mut(s).toggle(&press);
                }
                if let Some(f) = &select {
                    f(s, &press);
                }
            });
            if !openable {
                if let Some(f) = row_open.clone() {
                    let open = id.clone();
                    e = e.on_double_press_tx(move |s: &mut S, _tx: &mut Tx| f(s, &open));
                }
            }
            e
        };

        let rows_key = key.clone();
        let rows = col()
            .key(format!("{key}-rows"))
            .w_full()
            .growth(GrowthSpec::default())
            .virtual_rows(
                count,
                row_height,
                move |_s: &S, i| Cow::Owned(format!("{rows_key}-row-{i}")),
                build_row,
            );

        let measured_key = key.clone();
        let viewport = col()
            .key(format!("{key}-viewport"))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            // A scroll inside a page reports every frame; only a new height changes what the
            // window reads (the kit tree's own layout hook).
            .on_layout_tx(move |_s: &mut S, r: [f32; 4], tx: &mut Tx| {
                let before = viewport_height(&measured_key);
                if before == r[3] {
                    tx.live_only();
                } else {
                    VIEWPORTS.with(|m| m.borrow_mut().insert(measured_key.clone(), r[3]));
                }
            })
            .child(if count == 0 {
                el().key(format!("{key}-empty"))
                    .role("status")
                    .label(empty.clone())
                    .w_full()
                    .h(40.0)
                    .px(style.padding)
                    .center_y()
                    .child(
                        text(empty)
                            .font(k.ty.ui)
                            .color(k.c.text_muted)
                            .truncate()
                            .min_w(0.0),
                    )
            } else {
                rows
            });

        let key_handler = key.clone();
        let page_key = key.clone();
        let select = on_select.clone();
        let open = on_open.clone();
        let mut root = col()
            .key(key.clone())
            .role("tree")
            .focusable(true)
            .w_full()
            .clip_soft(0.0);
        if let Some(name) = name {
            root = root.label(name);
        }
        root = match height {
            Some(height) => root.h(height),
            None => {
                if grow {
                    root.grow(1.0).min_h(0.0)
                } else {
                    root.h(row_height * count.max(1) as f32)
                }
            }
        };
        root.on_key(move |s: &mut S, ev: &KeyEvent, tx: &mut Tx| -> bool {
            if get_mut(s).visible_len() == 0 {
                return false;
            }
            let before = get_mut(s).cursor().map(str::to_owned);
            // The model's page height is private to the kit's own view, so page here by
            // repeating the arrow key across the measured viewport.
            let out = {
                let m = get_mut(s);
                match ev.key {
                    Key::PageUp | Key::PageDown => {
                        let measured = viewport_height(&page_key);
                        let page = ((measured / row_height).floor() as usize)
                            .saturating_sub(1)
                            .max(1);
                        let key = if matches!(ev.key, Key::PageDown) {
                            Key::Down
                        } else {
                            Key::Up
                        };
                        let mut last = TreeKey::Handled;
                        for _ in 0..page {
                            last = m.handle_key(
                                &KeyEvent {
                                    key: key.clone(),
                                    mods: ev.mods,
                                },
                                label,
                                tx,
                            );
                        }
                        last
                    }
                    _ => m.handle_key(ev, label, tx),
                }
            };
            match out {
                TreeKey::Ignored => false,
                TreeKey::Handled => {
                    let after = {
                        let m = get_mut(s);
                        m.cursor().map(|id| (id.to_owned(), m.visible_row(id)))
                    };
                    if let Some((id, Some(row))) = after {
                        if before.as_deref() != Some(id.as_str()) {
                            tx.scroll_into_view_in(
                                key_handler.clone(),
                                format!("{key_handler}-row-{row}"),
                            );
                            if let Some(f) = &select {
                                f(s, &id);
                            }
                        }
                    }
                    true
                }
                TreeKey::Open(id) => {
                    if let Some(f) = &open {
                        f(s, &id);
                    }
                    true
                }
            }
        })
        .child(viewport)
    }
}

/// One row: indent guides, the chevron lane, the optional icon and the label.
fn tree_row<T>(
    k: &rune_kit::Kit,
    style: &TreeStyle,
    key: &str,
    i: usize,
    data: &T,
    r: &TreeRow<'_>,
    label: fn(&T) -> &str,
    icon_fn: Option<fn(&T) -> Option<&'static Icon>>,
) -> El {
    let row_key = format!("{key}-row-{i}");
    let pad = style.padding;
    let name = label(data);
    let mut e = row()
        .key(row_key.clone())
        .role("treeitem")
        .label(name)
        .level(r.depth + 1)
        .w_full()
        .h(style.row_height)
        .pl(pad + r.depth as f32 * style.indent)
        .pr(pad)
        .gap(4.0)
        .center_y()
        .radius(k.radius.sm);
    if let Some(open) = r.expanded {
        e = e.expanded(open);
    }
    e = if r.selected {
        e.fill(style.selected)
    } else {
        e.hover(k.c.hover)
    };
    if style.guides {
        for level in 0..r.depth {
            let x = pad + level as f32 * style.indent + 8.0 - 0.5;
            e = e.child(
                el().key(format!("{row_key}-guide-{level}"))
                    .pin(Some(x), Some(0.0), None, Some(0.0))
                    .w(1.0)
                    .fill(style.guide)
                    .a11y_hidden(),
            );
        }
    }
    let mut lane = el()
        .key(format!("{row_key}-chevron"))
        .size(16.0, 16.0)
        .center()
        .a11y_hidden();
    if let Some(open) = r.expanded {
        lane = lane.child(
            icon(k.glyph.chevron_down, 16.0)
                .key(format!("{row_key}-glyph"))
                .color(k.c.icon_muted)
                .icon_rotation(if open { 0.0 } else { -FRAC_PI_2 })
                .mirror_in_rtl(),
        );
    }
    e = e.child(lane);
    if let Some(glyph) = icon_fn.and_then(|f| f(data)) {
        e = e.child(
            icon(glyph, 16.0)
                .key(format!("{row_key}-icon"))
                .color(k.c.icon_muted)
                .a11y_hidden(),
        );
    }
    e.child(
        text(name)
            .key(format!("{row_key}-label"))
            .font(k.ty.ui)
            .color(k.c.text_base)
            .grow(1.0)
            .shrink()
            .min_w(0.0)
            .truncate()
            .help_when_truncated(name),
    )
}
