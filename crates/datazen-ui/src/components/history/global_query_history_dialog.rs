//! Global query history dialog — port of
//! `packages/frontend/src/components/history/GlobalQueryHistoryDialog.tsx`.
//!
//! A modal shell ([`Dialog`]) hosting the already-ported [`HistoryFilterBar`]
//! and [`HistoryEntryCard`]. React's `useConnectionStore` and `queryCommands`
//! are host concerns, not Rune's: this component takes a [`GlobalHistoryVm`]
//! projection (connection options, a paged entry list, selection, loading and
//! error state) plus callbacks that mutate the app state `S`. Every filter,
//! action, copy, export, clear, notice and page change is a callback input.
//!
//! Structure follows React's JSX: header (title + close, from [`Dialog`]), the
//! filter bar, an optional notice and truncation strip, the select-all / export
//! / clear bar, the entry list (loading / error / empty / cards), and a
//! pagination footer (a Rune extension — React asked for 200 rows in one read
//! and showed only a truncation notice).
//!
//! Copy is owned by [`HistoryEntryCard`]'s `CopyButton` (from
//! `ui/copyable.rs`): it writes the exact SQL to the clipboard and schedules
//! the feedback wake; this dialog only records which row was copied in the VM
//! so the card can show the `common.copied` label.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | panel / title / close / body | `<key>`, `<key>-title`, `<key>-close`, `<key>-body` (from [`Dialog`]) |
//! | filter bar | `history-filter-bar*` (from [`HistoryFilterBar`]) |
//! | cards | `history-entry-card-{id}*` (from [`HistoryEntryCard`]) |
//! | notice | `<key>-notice`, `<key>-notice-dismiss` |
//! | truncation | `<key>-truncation` |
//! | select all | `<key>-select-all` |
//! | export | `<key>-export` |
//! | clear / confirm / cancel | `<key>-clear`, `<key>-clear-confirm`, `<key>-clear-cancel` |
//! | clear confirm text | `<key>-clear-confirm-text` |
//! | loading | `<key>-loading` |
//! | error | `<key>-error` |
//! | empty | `<key>-empty`, `<key>-empty-icon`, `<key>-empty-text` |
//! | pagination | `<key>-pagination`, `<key>-page-label`, `<key>-page-prev`, `<key>-page-next` |
//!
//! All colours, type and radii come from the kit palette; there are no raw
//! colour literals here.

use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit};

use crate::components::history::history_filter_bar::HistoryStatus;
use crate::components::history::{
    HistoryConnectionOption, HistoryEntryAction, HistoryEntryCard, HistoryEntryVm,
    HistoryFilterBar, HistoryFilterOpen, HistoryQueryState, HistoryRange, HistorySort,
};
use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT, ICON_CLOCK, ICON_TRASH, ICON_X};
use crate::ui::banner::Banner;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

/// One paged row plus the connection id React uses for the display name.
#[allow(dead_code)] // projection type; fields are read by host wiring/tests
#[derive(Clone, Debug)]
pub(crate) struct GlobalHistoryEntry {
    pub(crate) connection_id: String,
    pub(crate) entry: HistoryEntryVm,
}

/// The paged result. `total` is the backend's honest count, so a short page is
/// never presented as the whole list.
#[allow(dead_code)] // projection type; fields are read by host wiring/tests
#[derive(Clone, Debug)]
pub(crate) struct HistoryPageVm {
    pub(crate) entries: Vec<GlobalHistoryEntry>,
    pub(crate) total: usize,
    /// Zero-based current page.
    pub(crate) page: usize,
    pub(crate) page_size: usize,
}

impl Default for HistoryPageVm {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            total: 0,
            page: 0,
            page_size: 200,
        }
    }
}

/// Everything the dialog renders, kept in the app state `S` and projected here.
#[allow(dead_code)] // projection type; fields are read by host wiring/tests
#[derive(Clone, Debug)]
pub(crate) struct GlobalHistoryVm {
    pub(crate) open: bool,
    /// Bumped on each open so Rune resets the body's scroll offset.
    pub(crate) open_epoch: u64,
    pub(crate) query: HistoryQueryState,
    pub(crate) page: HistoryPageVm,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) selected: Vec<String>,
    /// The row whose copy feedback is currently visible.
    pub(crate) copied_id: Option<String>,
    pub(crate) notice: Option<String>,
    pub(crate) confirm_clear: bool,
    pub(crate) filter_open: HistoryFilterOpen,
    pub(crate) connections: Vec<HistoryConnectionOption>,
    /// Mirrors React's `canOpen = Boolean(onSelectQuery)`.
    pub(crate) can_open: bool,
}

impl Default for GlobalHistoryVm {
    fn default() -> Self {
        Self {
            open: false,
            open_epoch: 0,
            query: HistoryQueryState::default(),
            page: HistoryPageVm::default(),
            loading: false,
            error: None,
            selected: Vec::new(),
            copied_id: None,
            notice: None,
            confirm_clear: false,
            filter_open: HistoryFilterOpen::None,
            connections: Vec::new(),
            can_open: false,
        }
    }
}

type CloseFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type IdFn<S> = Rc<dyn Fn(&mut S, String) + 'static>;
type QueryFn<S> = Rc<dyn Fn(&mut S, HistoryQueryState) + 'static>;
type FilterOpenFn<S> = Rc<dyn Fn(&mut S, HistoryFilterOpen) + 'static>;
type ActionFn<S> = Rc<dyn Fn(&mut S, HistoryEntryAction, String) + 'static>;
type CopyFn<S> = Rc<dyn Fn(&mut S, String, &mut Tx) + 'static>;
type PageFn<S> = Rc<dyn Fn(&mut S, usize) + 'static>;
type SearchFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;

/// Builder for the global query-history dialog, generic over the app state `S`
/// so every callback can write the state that actually owns the dialog's VM.
pub(crate) struct GlobalQueryHistoryDialog<'a, S: 'static> {
    key: &'a str,
    vm: &'a GlobalHistoryVm,
    search_edit: &'a TextEdit,
    close: Option<CloseFn<S>>,
    select_query: Option<IdFn<S>>,
    query: Option<QueryFn<S>>,
    filter_open: Option<FilterOpenFn<S>>,
    toggle_select: Option<IdFn<S>>,
    toggle_all: Option<CloseFn<S>>,
    action: Option<ActionFn<S>>,
    copy: Option<CopyFn<S>>,
    export: Option<CloseFn<S>>,
    clear: Option<CloseFn<S>>,
    confirm_clear: Option<CloseFn<S>>,
    cancel_clear: Option<CloseFn<S>>,
    dismiss_notice: Option<CloseFn<S>>,
    page_change: Option<PageFn<S>>,
    search_edit_fn: Option<SearchFn<S>>,
}

#[allow(dead_code)] // reusable API; wired by the query-history host/view track
impl<'a, S: 'static> GlobalQueryHistoryDialog<'a, S> {
    pub(crate) fn new(key: &'a str, vm: &'a GlobalHistoryVm, search_edit: &'a TextEdit) -> Self {
        Self {
            key,
            vm,
            search_edit,
            close: None,
            select_query: None,
            query: None,
            filter_open: None,
            toggle_select: None,
            toggle_all: None,
            action: None,
            copy: None,
            export: None,
            clear: None,
            confirm_clear: None,
            cancel_clear: None,
            dismiss_notice: None,
            page_change: None,
            search_edit_fn: None,
        }
    }

    /// Header close, Escape and outside-press all run this handler.
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.close = Some(Rc::new(f));
        self
    }

    /// React's `onSelectQuery`: the open action on a row. Its presence is also
    /// what makes the card expose an Open control (via `vm.can_open`).
    pub(crate) fn on_select_query(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.select_query = Some(Rc::new(f));
        self
    }

    /// Any filter change arrives as a complete new [`HistoryQueryState`].
    pub(crate) fn on_query(mut self, f: impl Fn(&mut S, HistoryQueryState) + 'static) -> Self {
        self.query = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_filter_open(
        mut self,
        f: impl Fn(&mut S, HistoryFilterOpen) + 'static,
    ) -> Self {
        self.filter_open = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_toggle_select(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.toggle_select = Some(Rc::new(f));
        self
    }

    /// Select-all / clear-selection toggle (the VM decides from `selected`).
    pub(crate) fn on_toggle_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.toggle_all = Some(Rc::new(f));
        self
    }

    /// Favorite / delete / open actions. Copy is forwarded for parity, but the
    /// clipboard write and feedback wake belong to the card's `CopyButton`.
    pub(crate) fn on_action(
        mut self,
        f: impl Fn(&mut S, HistoryEntryAction, String) + 'static,
    ) -> Self {
        self.action = Some(Rc::new(f));
        self
    }

    /// Recorded after the card has copied SQL; use it to set `vm.copied_id`.
    pub(crate) fn on_copy(mut self, f: impl Fn(&mut S, String, &mut Tx) + 'static) -> Self {
        self.copy = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_export(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.export = Some(Rc::new(f));
        self
    }

    /// Enter the clear-history confirmation.
    pub(crate) fn on_clear(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.clear = Some(Rc::new(f));
        self
    }

    /// The confirmed clear-history load/delete callback.
    pub(crate) fn on_confirm_clear(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.confirm_clear = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_cancel_clear(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.cancel_clear = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_dismiss_notice(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.dismiss_notice = Some(Rc::new(f));
        self
    }

    /// Request a different (zero-based) page; the host then reloads entries.
    pub(crate) fn on_page_change(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.page_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_search_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.search_edit_fn = Some(Rc::new(f));
        self
    }

    /// Paints the modal into `layer`. Dropdown overlays from the filter bar are
    /// appended after the dialog so they sit above the scrim and panel.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        if !self.vm.open {
            return;
        }
        let k = kit(dark);

        let mut overlay = Vec::new();
        let filter_bar = self.filter_bar(dark, &mut overlay);

        let key = self.key;
        let mut dialog = Dialog::new(key, t("query.historyTitle"))
            .open(true)
            .open_epoch(self.vm.open_epoch)
            .width(768.0)
            .fill_height()
            .max_height(720.0)
            .header_pad((20.0, 14.0))
            .body_pad((20.0, 16.0))
            .footer_pad((16.0, 8.0))
            .footer_height(48.0)
            .child(filter_bar);

        if let Some(notice) = self.vm.notice.as_deref() {
            dialog = dialog.child(self.notice_bar(k, notice));
        }
        if let Some((shown, total)) = truncation(&self.vm.page) {
            dialog = dialog.child(self.truncation_bar(k, shown, total));
        }
        dialog = dialog.child(self.selection_bar(dark));
        dialog = dialog.child(self.entry_list(dark));
        dialog = dialog.footer(self.pagination(k));

        // `Dialog` mounts nothing without a dismissal handler; fall back to a
        // no-op so a host that only wants a static preview still renders.
        match self.close.clone() {
            Some(close) => dialog = dialog.on_close(move |s: &mut S| close(s)),
            None => dialog = dialog.on_close(|_: &mut S| {}),
        }

        dialog.build(k, layer);
        layer.extend(overlay);
    }

    fn filter_bar(&self, dark: bool, layer: &mut Vec<El>) -> El {
        let mut bar = HistoryFilterBar::<S>::new(self.vm.query.clone(), self.search_edit)
            .connections(self.vm.connections.clone())
            .open(self.vm.filter_open);

        if let Some(edit) = self.search_edit_fn.clone() {
            bar = bar.on_search_edit(move |s: &mut S| edit(s));
        }

        // Every filter change rewrites the whole query, mirroring React's
        // immutable `setQuery({ ...q, field })` updates.
        if let Some(query) = self.query.clone() {
            // Each closure owns its own `Rc` clone so the shared callback can be
            // bound to every filter control.
            {
                let (q, cb) = (self.vm.query.clone(), query.clone());
                bar = bar.on_set_connection(move |s: &mut S, id| {
                    let mut next = q.clone();
                    next.connection_id = id;
                    next.database.clear();
                    next.schema.clear();
                    cb(s, next);
                });
            }
            {
                let (q, cb) = (self.vm.query.clone(), query.clone());
                bar = bar.on_set_database(move |s: &mut S, db| {
                    let mut next = q.clone();
                    next.database = db;
                    cb(s, next);
                });
            }
            {
                let (q, cb) = (self.vm.query.clone(), query.clone());
                bar = bar.on_set_schema(move |s: &mut S, schema| {
                    let mut next = q.clone();
                    next.schema = schema;
                    cb(s, next);
                });
            }
            {
                let (q, cb) = (self.vm.query.clone(), query.clone());
                bar = bar.on_set_status(move |s: &mut S, status: HistoryStatus| {
                    let mut next = q.clone();
                    next.status = status;
                    cb(s, next);
                });
            }
            {
                let (q, cb) = (self.vm.query.clone(), query.clone());
                bar = bar.on_set_range(move |s: &mut S, range: HistoryRange| {
                    let mut next = q.clone();
                    next.range = range;
                    cb(s, next);
                });
            }
            {
                let (q, cb) = (self.vm.query.clone(), query.clone());
                bar = bar.on_set_sort(move |s: &mut S, sort: HistorySort| {
                    let mut next = q.clone();
                    next.sort = sort;
                    cb(s, next);
                });
            }
            bar = bar.on_reset(move |s: &mut S| query(s, HistoryQueryState::default()));
        }

        if let Some(open) = self.filter_open.clone() {
            let close = open.clone();
            bar = bar.on_toggle_open(move |s: &mut S, which| open(s, which));
            bar = bar.on_close(move |s: &mut S| close(s, HistoryFilterOpen::None));
        } else {
            // Select requires a toggle to be a live control; keep it inert but
            // constructible for hosts that do not own filter-open state.
            bar = bar
                .on_toggle_open(|_: &mut S, _: HistoryFilterOpen| {})
                .on_close(|_: &mut S| {});
        }

        bar.build(dark, layer)
    }

    fn notice_bar(&self, k: &Kit, notice: &str) -> El {
        let key = self.key;
        let mut dismiss = IconButton::new(&ICON_X)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .label(t("common.close"))
            .build(k)
            .key(format!("{key}-notice-dismiss"));
        if let Some(handler) = self.dismiss_notice.clone() {
            dismiss = dismiss.on_press(move |s: &mut S| handler(s));
        }
        row()
            .key(format!("{key}-notice"))
            .w_full()
            .items_center()
            .justify_between()
            .gap(8.0)
            .radius(4.0)
            .fill(k.alpha(k.c.bg_accent, 0.1))
            .px(8.0)
            .py(4.0)
            .child(
                text(notice.to_owned())
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent)
                    .min_w(0.0),
            )
            .child(dismiss)
    }

    fn truncation_bar(&self, k: &Kit, shown: usize, total: usize) -> El {
        let label = t("query.historyShowingOf")
            .replace("{shown}", &shown.to_string())
            .replace("{total}", &total.to_string());
        row()
            .key(format!("{}-truncation", self.key))
            .w_full()
            .radius(4.0)
            .fill(k.c.layer_01)
            .px(8.0)
            .py(4.0)
            .child(text(label).font(k.ty.text_xsmall).color(k.c.text_muted))
    }

    fn selection_bar(&self, dark: bool) -> El {
        let k = kit(dark);
        let key = self.key;
        let visible = visible_entries(self.vm);
        let selected_count = visible
            .iter()
            .filter(|row| self.vm.selected.iter().any(|id| id == &row.entry.id))
            .count();

        let mut select_all = Button::new(t("query.historySelectAll"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        if let Some(toggle) = self.toggle_all.clone() {
            select_all = select_all.on_press(move |s: &mut S| toggle(s));
        }

        let export_label = if selected_count > 0 {
            t("query.historyExportSelected").replace("{count}", &selected_count.to_string())
        } else {
            t("query.historyExport").to_owned()
        };
        let mut export = Button::new(export_label)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(selected_count == 0)
            .help(t("query.historyExport"));
        if let Some(handler) = self.export.clone() {
            export = export.on_press(move |s: &mut S| handler(s));
        }

        let clear_area = if self.vm.confirm_clear {
            let mut confirm = Button::new(t("query.historyConfirm"))
                .variant(ButtonVariant::Danger)
                .control_size(ControlSize::Small);
            if let Some(handler) = self.confirm_clear.clone() {
                confirm = confirm.on_press(move |s: &mut S| handler(s));
            }
            let mut cancel = Button::new(t("query.historyCancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small);
            if let Some(handler) = self.cancel_clear.clone() {
                cancel = cancel.on_press(move |s: &mut S| handler(s));
            }
            row()
                .key(format!("{key}-clear-confirm-row"))
                .items_center()
                .gap(4.0)
                .child(
                    text(t("query.historyClearConfirm"))
                        .key(format!("{key}-clear-confirm-text"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.danger.fg),
                )
                .child(confirm.build(k).key(format!("{key}-clear-confirm")))
                .child(cancel.build(k).key(format!("{key}-clear-cancel")))
        } else {
            let mut clear = Button::new(t("query.historyClear"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .icon(&ICON_TRASH)
                .help(t("query.historyClearTitle"));
            if let Some(handler) = self.clear.clone() {
                clear = clear.on_press(move |s: &mut S| handler(s));
            }
            clear.build(k).key(format!("{key}-clear"))
        };

        row()
            .key(format!("{key}-selection-bar"))
            .w_full()
            .min_w(0.0)
            .items_center()
            .gap(8.0)
            .child(select_all.build(k).key(format!("{key}-select-all")))
            .child(export.build(k).key(format!("{key}-export")))
            .child(
                row()
                    .grow(1.0)
                    .min_w(0.0)
                    .justify_end()
                    .child(clear_area),
            )
    }

    fn entry_list(&self, dark: bool) -> El {
        let k = kit(dark);
        let key = self.key;
        let visible = visible_entries(self.vm);

        let mut list = col()
            .key(format!("{key}-list"))
            .w_full()
            .min_w(0.0)
            .gap(8.0)
            .max_h(360.0)
            .scroll(rune::ScrollSpec { stick_bottom: false });

        if let Some(error) = self.vm.error.clone() {
            let message = if error.is_empty() {
                t("common.error").to_owned()
            } else {
                error
            };
            return col()
                .key(format!("{key}-list"))
                .w_full()
                .children(Banner::new(format!("{key}-error"), message).build(k));
        }

        if self.vm.loading && visible.is_empty() {
            return col()
                .key(format!("{key}-list"))
                .w_full()
                .h(192.0)
                .center()
                .child(
                    text(t("query.historyLoading"))
                        .key(format!("{key}-loading"))
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                );
        }

        if visible.is_empty() {
            let no_history = self.vm.page.total == 0 && self.vm.page.entries.is_empty();
            let message = if no_history {
                t("query.noHistory")
            } else {
                t("query.noHistoryMatch")
            };
            return col()
                .key(format!("{key}-list"))
                .w_full()
                .h(192.0)
                .center()
                .child(
                    col()
                        .key(format!("{key}-empty"))
                        .items_center()
                        .gap(8.0)
                        .child(
                            icon(&ICON_CLOCK, 32.0)
                                .key(format!("{key}-empty-icon"))
                                .color(k.c.text_faint),
                        )
                        .child(
                            text(message)
                                .key(format!("{key}-empty-text"))
                                .font(k.ty.text_small)
                                .color(k.c.text_muted),
                        ),
                );
        }

        let names = connection_names(&self.vm.connections);
        for row in visible {
            let id = row.entry.id.clone();
            let name = names
                .get(row.connection_id.as_str())
                .cloned()
                .unwrap_or_else(|| row.connection_id.clone());
            let mut card = HistoryEntryCard::<S>::new(row.entry.clone(), name)
                .selected(self.vm.selected.iter().any(|selected| selected == &id))
                .copied(self.vm.copied_id.as_deref() == Some(id.as_str()))
                .can_open(self.vm.can_open || self.select_query.is_some());
            if let Some(toggle) = self.toggle_select.clone() {
                card = card.on_toggle_select(move |s: &mut S, clicked| toggle(s, clicked));
            }
            let forwarded_action = self.action.clone();
            let open_action = self.select_query.clone();
            if forwarded_action.is_some() || open_action.is_some() {
                card = card.on_action(move |s: &mut S, action_kind, action_id| {
                    if action_kind == HistoryEntryAction::Open {
                        if let Some(open) = &open_action {
                            open(s, action_id.clone());
                            return;
                        }
                    }
                    if let Some(action) = &forwarded_action {
                        action(s, action_kind, action_id);
                    }
                });
            }
            if let Some(copy) = self.copy.clone() {
                card = card.on_copy(move |s: &mut S, copied_id, tx| copy(s, copied_id, tx));
            }
            list = list.child(card.build(dark));
        }
        list
    }

    fn pagination(&self, k: &Kit) -> El {
        let key = self.key;
        let page = &self.vm.page;
        let page_size = page.page_size.max(1);
        let total_pages = (page.total + page_size - 1) / page_size.max(1);
        let total_pages = total_pages.max(1);
        let current = page.page.min(total_pages.saturating_sub(1));
        let label = t("pagination.pageOnly").replace("{page}", &(current + 1).to_string());

        let mut prev = Button::new(t("pagination.prev"))
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Small)
            .icon(&ICON_CHEVRON_LEFT)
            .disabled(self.vm.loading || current == 0);
        if let Some(change) = self.page_change.clone() {
            prev = prev.on_press(move |s: &mut S| change(s, current.saturating_sub(1)));
        }
        let mut next = Button::new(t("pagination.next"))
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Small)
            .icon(&ICON_CHEVRON_RIGHT)
            .disabled(self.vm.loading || current + 1 >= total_pages);
        if let Some(change) = self.page_change.clone() {
            next = next.on_press(move |s: &mut S| change(s, current + 1));
        }

        row()
            .key(format!("{key}-pagination"))
            .grow(1.0)
            .min_w(0.0)
            .items_center()
            .justify_between()
            .gap(8.0)
            .child(
                text(label)
                    .key(format!("{key}-page-label"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                row()
                    .gap(8.0)
                    .center_y()
                    .child(prev.build(k).key(format!("{key}-page-prev")))
                    .child(next.build(k).key(format!("{key}-page-next"))),
            )
    }
}

/// The status filter is the only predicate the backend cannot evaluate, so it
/// is applied here exactly as React's `applyStatusFilter` does.
fn visible_entries(vm: &GlobalHistoryVm) -> Vec<&GlobalHistoryEntry> {
    match vm.query.status {
        HistoryStatus::All => vm.page.entries.iter().collect(),
        HistoryStatus::Success => vm
            .page
            .entries
            .iter()
            .filter(|row| row.entry.success)
            .collect(),
        HistoryStatus::Failed => vm
            .page
            .entries
            .iter()
            .filter(|row| !row.entry.success)
            .collect(),
    }
}

/// React's `truncationNotice`: the backend had more matches than it returned.
fn truncation(page: &HistoryPageVm) -> Option<(usize, usize)> {
    (page.total > page.entries.len()).then_some((page.entries.len(), page.total))
}

fn connection_names(options: &[HistoryConnectionOption]) -> std::collections::HashMap<&str, String> {
    options
        .iter()
        .map(|option| (option.id.as_str(), option.name.clone()))
        .collect()
}
