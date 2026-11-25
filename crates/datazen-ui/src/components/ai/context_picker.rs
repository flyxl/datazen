//! T217: the AI context picker — port of
//! `packages/frontend/src/components/ai/ContextPicker.tsx` (394 lines).
//!
//! The `@`-mention picker [`AiInput`](super::ai_input) opens: the root
//! view (Tables / Files categories plus the recents section), the drill-in
//! lists, and the keyboard router. The React component is controlled
//! (`query` prop, data fetched through commands, recents persisted to
//! `localStorage`); the port keeps the same split — the host owns a
//! [`ContextPickerState`] (the view, the lists, the loading flags, the
//! recents, the active row), renders through [`ContextPicker`], and
//! mutates through the callbacks.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | root | `context-picker` (React `data-testid`) |
//! | back row | `context-picker-back` (React `data-testid`) |
//! | Tables category | `context-cat-tables` (React `data-testid`) |
//! | Files category | `context-cat-files` (React `data-testid`) |
//! | recent section | `context-section-<label>` (React key `section-<label>`) |
//! | one item row | `context-item-<Kind>:<id>` (React `data-testid="context-item"` + `data-kind` / `data-id`) |
//! | loading row | `context-picker-loading` |
//! | loading spinner | `context-picker-loading-spinner` |
//! | empty row | `context-picker-empty` |
//!
//! ## Deliberate deviations from the React source
//!
//! * **The fetch effects are host wiring.** React's
//!   `databaseCommands.listTables` / `contextCommands.listFiles` (and the
//!   `loadingTables` / `loadingFiles` flags they drive) run as effects; the
//!   port's state carries the results and flags, and the host sets them as
//!   the fetches resolve. `needs_tables` / `needs_files` / the loading
//!   disjunction derive exactly as React computes them.
//! * **The recents are host-persisted.** React's `localStorage`
//!   (`datazen.contextRecent`, max 8, LRU dedupe by kind + id) is a
//!   persistence channel; the state keeps the same list shape and the
//!   host wiring persists it if it wants.
//! * **The picker is a plain block** (288pt wide, capped 240pt with a
//!   vertical scroll viewport — React's `max-h-60 overflow-y-auto`): rune
//!   has no absolute layout (the rule [`ai_input`](super::ai_input)
//!   documents for its send control), so the host layer anchors it.
//!   `position` (above / below) is carried for parity but renders no
//!   offset — the host places the block.
//! * **BUG-06's `onReady` is the public
//!   [`ContextPickerState::handle_key`].** React hands the parent textarea
//!   the picker's keydown handler so it can forward events; the port's
//!   host (the `AiInput` wiring) forwards by calling the state method, and
//!   each mounted row also installs the same handler so the picker works
//!   standalone from a focused row.
//! * **`onMouseEnter` → `setActiveIndex` has no rune surface** (hover
//!   fills only); arrows and clicks are the active-row drivers. The
//!   document-level click-away close is the host layer's job — `on_close`
//!   is the callback. `scrollIntoView` is N/A without list scroll.
//! * **`text-[10px]`** renders at the kit's 12pt `text_xsmall` (no 10pt
//!   step; the input area documents the same rule) with `text_xsmall_medium`
//!   for the `font-medium`; the `uppercase tracking-wide` transform has no
//!   surface (the built zh-CN locale is case-free).
//! * `lucide-react`'s `Table2` / `Folder` / `Layers` / `File` /
//!   `ChevronLeft` map to the registry's `ICON_TABLE` / `ICON_FOLDER` /
//!   `ICON_LAYERS` / `ICON_FILE` / `ICON_CHEVRON_LEFT` (14pt, React's
//!   `h-3.5 w-3.5`).
//! * The active row's `bg-accent/15 text-accent` wash and the non-active
//!   `hover:bg-surface-raised` arm use the same `alpha(bg_accent, 0.15)` /
//!   `hover(layer_02)` fills the chips use.

// The production consumer is the AiInput wiring (the picker mounts from
// the host's layer, anchored around the input); until then the component
// tests are the only callers.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, icon, row, text, El, KeyEvent, Key, ScrollSpec};

use super::ai_egress_notice::{ContextItem, ContextKind};
use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_LEFT, ICON_FILE, ICON_FOLDER, ICON_LAYERS, ICON_TABLE};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};

/// React's `MAX_RECENT`: the recents list cap.
pub const MAX_RECENT: usize = 8;

/// React's `w-72` (288px).
const W: f32 = 288.0;
/// React's `max-h-60` (240px).
const MAX_H: f32 = 240.0;
/// React's `h-3.5 w-3.5` icon (14px).
const ICON_PT: f32 = 14.0;

/// React's `View`: root (categories) or one drill-in list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickerView {
    #[default]
    Root,
    Tables,
    Files,
}

/// React's category ids (`'tables' | 'files'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerCategory {
    Tables,
    Files,
}

impl PickerCategory {
    /// The React id string (`'tables'` / `'files'`).
    pub fn as_str(self) -> &'static str {
        match self {
            PickerCategory::Tables => "tables",
            PickerCategory::Files => "files",
        }
    }

    /// The view a drill-in opens.
    fn view(self) -> PickerView {
        match self {
            PickerCategory::Tables => PickerView::Tables,
            PickerCategory::Files => PickerView::Files,
        }
    }
}

/// React's `position` prop ('above' | 'below'): a host placement hint (see
/// the deviations — no rune absolute layout, so it renders no offset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickerPosition {
    #[default]
    Above,
    Below,
}

/// React's `Row`: a category, a section header, or one item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerRow {
    Category { id: PickerCategory, label: String },
    Section { label: String },
    Item(ContextItem),
}

/// What the picker's keydown router did (the host translates `Selected` /
/// `Close` into its `onSelect` / `onClose`).
#[derive(Clone, Debug, PartialEq)]
pub enum PickerKeyOutcome {
    /// Not one of the picker's keys: the caller lets the key through.
    Ignored,
    /// Handled entirely inside the state (an arrow move, a view switch).
    Consumed,
    /// An item was picked: the recents already advanced; call `onSelect`.
    Selected(ContextItem),
    /// The root view's Escape: call `onClose`.
    Close,
}

/// React's `ContextEntry` mirror (`types/index.ts`): one file-tree entry
/// with its nested children. The host wiring flattens its fetch through
/// [`flatten_file_entries`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContextEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<ContextEntry>,
}

/// React's `matchesQuery`: the name or the id contains the lowercased
/// query.
pub fn matches_query(item: &ContextItem, q: &str) -> bool {
    let lower = q.to_lowercase();
    item.name.to_lowercase().contains(lower.as_str())
        || item.id.to_lowercase().contains(lower.as_str())
}

/// React's `isCtxYaml` arm: the item icon picks `Layers` for a
/// `.ctx.yaml` / `.ctx.yml` file.
pub fn is_ctx_yaml(name: &str) -> bool {
    name.ends_with(".ctx.yaml") || name.ends_with(".ctx.yml")
}

/// React's item icon ternary: `Table2` for tables, `Folder` for dirs,
/// `Layers` for ctx.yaml files, `File` otherwise.
pub fn item_icon(item: &ContextItem) -> &'static rune::Icon {
    match item.kind {
        ContextKind::Table => &ICON_TABLE,
        ContextKind::Dir => &ICON_FOLDER,
        ContextKind::File if is_ctx_yaml(&item.name) => &ICON_LAYERS,
        ContextKind::File => &ICON_FILE,
    }
}

/// React's `tableToItem` name: `schema.name` when the schema is truthy,
/// the bare name otherwise.
pub fn table_label(schema: Option<&str>, name: &str) -> String {
    match schema {
        Some(schema) if !schema.is_empty() => format!("{schema}.{name}"),
        _ => name.to_owned(),
    }
}

/// React's `entryToItem`: the entry's kind/id/name.
pub fn entry_to_item(entry: &ContextEntry) -> ContextItem {
    ContextItem {
        kind: if entry.is_dir { ContextKind::Dir } else { ContextKind::File },
        id: entry.path.clone(),
        name: entry.name.clone(),
    }
}

/// React's `flattenFileEntries`: the entry, then its children
/// depth-first (dirs included as their own items).
pub fn flatten_file_entries(entries: &[ContextEntry]) -> Vec<ContextItem> {
    let mut result = Vec::new();
    for entry in entries {
        result.push(entry_to_item(entry));
        result.extend(flatten_file_entries(&entry.children));
    }
    result
}

/// The state the host owns for one mounted picker: the view, the lists,
/// the loading flags, the recents, and the active row.
#[derive(Clone, Debug, Default)]
pub struct ContextPickerState {
    view: PickerView,
    recent: Vec<ContextItem>,
    tables: Vec<ContextItem>,
    files: Vec<ContextItem>,
    loading_tables: bool,
    loading_files: bool,
    active_index: usize,
}

impl ContextPickerState {
    /// The empty picker (React mounts with an empty `recent` until the
    /// effect reloads it — the host seeds through
    /// [`ContextPickerState::set_recent`]).
    pub fn new() -> Self {
        Self::default()
    }

    /// React's `view` state.
    pub fn view(&self) -> PickerView {
        self.view
    }

    /// React's `recent` state.
    pub fn recent(&self) -> &[ContextItem] {
        &self.recent
    }

    /// React's `tables` state (the fetched, already-mapped list).
    pub fn tables(&self) -> &[ContextItem] {
        &self.tables
    }

    /// React's `files` state (the fetched, already-flattened list).
    pub fn files(&self) -> &[ContextItem] {
        &self.files
    }

    /// React's `loadingTables`.
    pub fn loading_tables(&self) -> bool {
        self.loading_tables
    }

    /// React's `loadingFiles`.
    pub fn loading_files(&self) -> bool {
        self.loading_files
    }

    /// React's `activeIndex`.
    pub fn active_index(&self) -> usize {
        self.active_index
    }

    /// The recents seed (React's mount effect reloading `loadRecent`).
    pub fn set_recent(&mut self, recent: Vec<ContextItem>) {
        self.recent = recent;
    }

    /// The tables fetch's write end: the mapped list lands and the
    /// loading flag clears in the same write, exactly the pair React's
    /// `then` does (the catch arm writes an empty list — the same call).
    pub fn set_tables(&mut self, tables: Vec<ContextItem>) {
        self.tables = tables;
        self.loading_tables = false;
    }

    /// The files fetch's write end (same pairing as `set_tables`).
    pub fn set_files(&mut self, files: Vec<ContextItem>) {
        self.files = files;
        self.loading_files = false;
    }

    /// React's `setLoadingTables(true)` arm (the fetch's start).
    pub fn set_loading_tables(&mut self, loading: bool) {
        self.loading_tables = loading;
    }

    /// React's `setLoadingFiles(true)` arm.
    pub fn set_loading_files(&mut self, loading: bool) {
        self.loading_files = loading;
    }

    /// React's `setView` + `setActiveIndex(0)` pair — both call sites
    /// (the category click, the back button) reset the active row.
    pub fn set_view(&mut self, view: PickerView) {
        self.view = view;
        self.active_index = 0;
    }

    /// React's rows `useMemo`: the current view's row list, narrowed by
    /// the query exactly as React does (root filters only the recents;
    /// the drill-ins filter their lists; an empty query is no filter).
    pub fn rows(&self, query: &str, show_tables_category: bool) -> Vec<PickerRow> {
        match self.view {
            PickerView::Tables => filter_rows(&self.tables, query),
            PickerView::Files => filter_rows(&self.files, query),
            PickerView::Root => {
                let mut result = Vec::new();
                if show_tables_category {
                    result.push(PickerRow::Category {
                        id: PickerCategory::Tables,
                        label: t("context.tables").to_owned(),
                    });
                }
                result.push(PickerRow::Category {
                    id: PickerCategory::Files,
                    label: t("context.files").to_owned(),
                });
                let recent_rows: Vec<ContextItem> = if query.is_empty() {
                    self.recent.clone()
                } else {
                    self.recent
                        .iter()
                        .filter(|item| matches_query(item, query))
                        .cloned()
                        .collect()
                };
                if !recent_rows.is_empty() {
                    result.push(PickerRow::Section {
                        label: t("context.recent").to_owned(),
                    });
                    result.extend(recent_rows.into_iter().map(PickerRow::Item));
                }
                result
            }
        }
    }

    /// React's rows-length effect: clamp the active row into the list
    /// (`Math.min(prev, Math.max(rows.length - 1, 0))`).
    pub fn clamp_active(&mut self, rows_len: usize) {
        self.active_index = self.active_index.min(rows_len.saturating_sub(1));
    }

    /// React's `pushRecent` + `setRecent` pair: the picked item moves to
    /// the front (a duplicate by kind + id drops out first), the list caps
    /// at [`MAX_RECENT`].
    pub fn push_recent(&mut self, item: ContextItem) {
        self.recent
            .retain(|existing| !(existing.kind == item.kind && existing.id == item.id));
        self.recent.insert(0, item);
        self.recent.truncate(MAX_RECENT);
    }

    /// React's `handleKeyDown` (BUG-06): the picker's keyboard router,
    /// exposed for the host textarea's forwarding. The arrows and the
    /// view switches mutate here; the pick and the root Escape come back
    /// as [`PickerKeyOutcome::Selected`] / [`PickerKeyOutcome::Close`]
    /// for the host's `onSelect` / `onClose`.
    pub fn handle_key(&mut self, event: &KeyEvent, rows: &[PickerRow]) -> PickerKeyOutcome {
        match &event.key {
            Key::Down => {
                self.active_index = (self.active_index + 1).min(rows.len().saturating_sub(1));
                PickerKeyOutcome::Consumed
            }
            Key::Up => {
                self.active_index = self.active_index.saturating_sub(1);
                PickerKeyOutcome::Consumed
            }
            Key::Enter => match rows.get(self.active_index) {
                None => PickerKeyOutcome::Consumed,
                Some(PickerRow::Category { id, .. }) => {
                    self.set_view(id.view());
                    PickerKeyOutcome::Consumed
                }
                Some(PickerRow::Section { .. }) => PickerKeyOutcome::Consumed,
                Some(PickerRow::Item(item)) => {
                    let item = item.clone();
                    self.push_recent(item.clone());
                    PickerKeyOutcome::Selected(item)
                }
            },
            Key::Escape => {
                if self.view != PickerView::Root {
                    self.set_view(PickerView::Root);
                    PickerKeyOutcome::Consumed
                } else {
                    PickerKeyOutcome::Close
                }
            }
            _ => PickerKeyOutcome::Ignored,
        }
    }
}

/// The `query ? filtered : list` arm both drill-ins share.
fn filter_rows(items: &[ContextItem], query: &str) -> Vec<PickerRow> {
    let list: Vec<ContextItem> = if query.is_empty() {
        items.to_vec()
    } else {
        items
            .iter()
            .filter(|item| matches_query(item, query))
            .cloned()
            .collect()
    };
    list.into_iter().map(PickerRow::Item).collect()
}

/// The picker. Renders the state; the presses and keys mutate through the
/// state wire, the pick and the close come back through the callbacks.
pub struct ContextPicker<'a, S: 'static> {
    state: &'a ContextPickerState,
    query: String,
    position: PickerPosition,
    db_session: Option<&'a str>,
    state_access: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ContextPickerState>>,
    on_select: Option<Rc<dyn Fn(&mut S, ContextItem) + 'static>>,
    on_close: Option<Rc<dyn Fn(&mut S) + 'static>>,
}

impl<'a, S: 'static> ContextPicker<'a, S> {
    pub fn new(state: &'a ContextPickerState) -> Self {
        Self {
            state,
            query: String::new(),
            position: PickerPosition::Above,
            db_session: None,
            state_access: None,
            on_select: None,
            on_close: None,
        }
    }

    /// React's `query` prop (the `@`-word; empty means unfiltered).
    pub fn query(mut self, query: impl Into<String>) -> Self {
        self.query = query.into();
        self
    }

    /// React's `position` prop (a host placement hint; see the deviations).
    pub fn position(mut self, position: PickerPosition) -> Self {
        self.position = position;
        self
    }

    /// React's `dbSessionId` prop: presence alone gates the Tables
    /// category (`Boolean(dbSessionId)`); the fetch inputs themselves
    /// (`dbSessionId`, `database`) live with the host wiring.
    pub fn db_session(mut self, db_session: Option<&'a str>) -> Self {
        self.db_session = db_session;
        self
    }

    /// The wire that binds the picker's rows to the host's state: the
    /// presses (drill-in, back, pick) and the standalone key router
    /// mutate through it.
    pub fn state_access(
        mut self,
        access: impl for<'b> Fn(&'b mut S) -> &'b mut ContextPickerState + Clone + 'static,
    ) -> Self {
        self.state_access = Some(Rc::new(access));
        self
    }

    /// React's `onSelect` (a pick — the recents already advanced).
    pub fn on_select(mut self, f: impl Fn(&mut S, ContextItem) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// React's `onClose` (the root view's Escape; the click-away is the
    /// host layer's job).
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// React's `needsTables` / `needsFiles` / `loading` computation.
    fn loading(&self) -> bool {
        let needs_tables = self.state.view() == PickerView::Tables && self.db_session.is_some();
        let needs_files = self.state.view() == PickerView::Files;
        (needs_tables && self.state.loading_tables()) || (needs_files && self.state.loading_files())
    }

    /// React's empty-state ternary: no tables, or no query results, or
    /// no files.
    fn empty_label(&self) -> &'static str {
        if self.state.view() == PickerView::Tables {
            t("context.noTables")
        } else if !self.query.is_empty() {
            t("context.noResults")
        } else {
            t("context.noFiles")
        }
    }

    /// The picker, mounted (React renders it whenever the host layer
    /// mounts it).
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let show_tables = self.db_session.is_some();
        let rows = self.state.rows(&self.query, show_tables);
        // React's rows-length effect clamps after each rows change; the
        // render reads a render-time clamp so a stale host state cannot
        // overhang the list.
        let active = self.state.active_index().min(rows.len().saturating_sub(1));

        let mut root = col()
            .key("context-picker")
            .w(W)
            .max_h(MAX_H)
            // React's `overflow-y-auto`: the vertical viewport (the same
            // `.max_h` + `.scroll` pair the sql-preview dialogs use).
            .scroll(ScrollSpec { stick_bottom: false })
            .radius(k.radius.lg)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_01);

        // The back row, above every non-root view.
        if self.state.view() != PickerView::Root {
            root = root.child(self.back_row(&k));
        }

        if self.loading() {
            root = root.child(self.loading_row(dark, &k));
        } else if rows.is_empty() {
            root = root.child(self.empty_row(&k));
        } else {
            for (index, row) in rows.iter().enumerate() {
                root = root.child(self.render_row(index, row, active, &rows, &k));
            }
        }
        root
    }

    /// React's back button: the chevron + label on the list's border-B.
    fn back_row(&self, k: &rune_kit::Kit) -> El {
        let mut back = row()
            .role("button")
            .key("context-picker-back")
            .w_full()
            .items_center()
            .gap(6.0)
            .px(12.0)
            .py(6.0)
            .hover(k.c.layer_02)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .child(
                icon(&ICON_CHEVRON_LEFT, ICON_PT)
                    .key("context-picker-back-icon")
                    .color(k.c.text_muted)
                    .a11y_hidden(),
            )
            .child(
                text(t("context.back"))
                    .key("context-picker-back-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .truncate(),
            );
        if let Some(access) = self.state_access.clone() {
            back = back.focusable(true).on_press(move |s: &mut S| {
                access(s).set_view(PickerView::Root);
            });
        }
        back
    }

    /// React's loading row: the md spinner + the label.
    fn loading_row(&self, dark: bool, k: &rune_kit::Kit) -> El {
        row()
            .key("context-picker-loading")
            .items_center()
            .gap(8.0)
            .px(12.0)
            .py(12.0)
            .child(
                Spinner::new("context-picker-loading-spinner")
                    .size(SpinnerSize::Md)
                    .tone(SpinnerTone::Muted)
                    .label(t("common.loading"))
                    .build(dark),
            )
            .child(
                text(t("common.loading"))
                    .key("context-picker-loading-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint),
            )
    }

    /// React's empty row.
    fn empty_row(&self, k: &rune_kit::Kit) -> El {
        col()
            .key("context-picker-empty")
            .w_full()
            .px(12.0)
            .py(12.0)
            .child(
                text(self.empty_label())
                    .key("context-picker-empty-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint),
            )
    }

    /// One row of the list (React's `rows.map`).
    fn render_row(
        &self,
        index: usize,
        row: &PickerRow,
        active: usize,
        rows: &[PickerRow],
        k: &rune_kit::Kit,
    ) -> El {
        match row {
            PickerRow::Category { id, label } => {
                self.category_row(index, *id, label, index == active, rows, k)
            }
            PickerRow::Section { label } => self.section_row(label, k),
            PickerRow::Item(item) => self.item_row(index, item, index == active, rows, k),
        }
    }

    /// React's category button.
    fn category_row(
        &self,
        _index: usize,
        id: PickerCategory,
        label: &str,
        active: bool,
        rows: &[PickerRow],
        k: &rune_kit::Kit,
    ) -> El {
        let source = match id {
            PickerCategory::Tables => &ICON_TABLE,
            PickerCategory::Files => &ICON_FOLDER,
        };
        let mut el = row()
            .role("button")
            .key(format!("context-cat-{}", id.as_str()))
            .w_full()
            .items_center()
            .gap(8.0)
            .px(12.0)
            .py(6.0)
            .fill(if active {
                k.alpha(k.c.bg_accent, 0.15)
            } else {
                rune::Color::TRANSPARENT
            })
            .hover(k.c.layer_02)
            .child(
                icon(source, ICON_PT)
                    .key(format!("context-cat-{}-icon", id.as_str()))
                    .color(if active { k.c.text_accent } else { k.c.text_muted })
                    .a11y_hidden(),
            )
            .child(
                text(label.to_owned())
                    .key(format!("context-cat-{}-label", id.as_str()))
                    .font(k.ty.text_xsmall)
                    .color(if active { k.c.text_accent } else { k.c.text_muted })
                    .truncate(),
            );
        if let Some(access) = self.state_access.clone() {
            let rows_snapshot = rows.to_vec();
            el = el
                .focusable(true)
                .focus_selected(active)
                .on_press(move |s: &mut S| {
                    access(s).set_view(id.view());
                })
                .on_key(self.key_router(rows_snapshot));
        }
        el
    }

    /// React's section header.
    fn section_row(&self, label: &str, k: &rune_kit::Kit) -> El {
        row()
            .key(format!("context-section-{label}"))
            .w_full()
            .px(12.0)
            .pb(2.0)
            .pt(8.0)
            .child(
                text(label.to_owned())
                    .key(format!("context-section-{label}-label"))
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_faint)
                    .truncate(),
            )
    }

    /// React's item button: the kind icon (Layers for ctx.yaml files)
    /// next to the name.
    fn item_row(
        &self,
        _index: usize,
        item: &ContextItem,
        active: bool,
        rows: &[PickerRow],
        k: &rune_kit::Kit,
    ) -> El {
        let key = format!("context-item-{:?}:{}", item.kind, item.id);
        let mut el = row()
            .role("button")
            .key(key.clone())
            .w_full()
            .items_center()
            .gap(8.0)
            .px(12.0)
            .py(6.0)
            .fill(if active {
                k.alpha(k.c.bg_accent, 0.15)
            } else {
                rune::Color::TRANSPARENT
            })
            .hover(k.c.layer_02)
            .child(
                icon(item_icon(item), ICON_PT)
                    .key(format!("{key}-icon"))
                    .color(if active { k.c.text_accent } else { k.c.text_muted })
                    .a11y_hidden(),
            )
            .child(
                text(item.name.clone())
                    .key(format!("{key}-name"))
                    .font(k.ty.text_xsmall)
                    .color(if active { k.c.text_accent } else { k.c.text_muted })
                    .truncate(),
            );
        if let Some(access) = self.state_access.clone() {
            let picked = item.clone();
            let on_select = self.on_select.clone();
            let rows_snapshot = rows.to_vec();
            el = el
                .focusable(true)
                .focus_selected(active)
                .on_press(move |s: &mut S| {
                    // React's handleSelectItem: the recents advance, then
                    // the host's onSelect.
                    access(s).push_recent(picked.clone());
                    if let Some(on_select) = &on_select {
                        on_select(s, picked.clone());
                    }
                })
                .on_key(self.key_router(rows_snapshot));
        }
        el
    }

    /// The standalone key router every interactive row installs (the
    /// host-forwarded path is the same
    /// [`ContextPickerState::handle_key`]).
    fn key_router(
        &self,
        rows: Vec<PickerRow>,
    ) -> impl Fn(&mut S, &KeyEvent, &mut rune::Tx) -> bool + 'static {
        let access = self.state_access.clone();
        let on_select = self.on_select.clone();
        let on_close = self.on_close.clone();
        move |s: &mut S, event: &KeyEvent, _tx: &mut rune::Tx| {
            let Some(access) = &access else {
                return false;
            };
            let outcome = access(s).handle_key(event, &rows);
            match outcome {
                PickerKeyOutcome::Ignored => false,
                PickerKeyOutcome::Consumed => true,
                PickerKeyOutcome::Selected(item) => {
                    if let Some(on_select) = &on_select {
                        on_select(s, item);
                    }
                    true
                }
                PickerKeyOutcome::Close => {
                    if let Some(on_close) = &on_close {
                        on_close(s);
                    }
                    true
                }
            }
        }
    }
}
