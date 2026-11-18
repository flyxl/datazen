//! The DataTable page strip — a Rune port of
//! `packages/frontend/src/components/DataTable/Pagination.tsx`.
//!
//! The React component is a stateless 40pt footer: a row count on the left, then
//! the paging notices, the count action, the page-size picker and the previous /
//! next buttons on the right. This port keeps that tree, in that order, and takes
//! every colour, radius and font from `kit(dark)`.
//!
//! # Key convention
//! `data-table-pagination-<role>` (the React `data-testid="pagination-*"` names
//! mapped onto the shared key prefix).
//!
//! | part                | key                                        |
//! |---------------------|--------------------------------------------|
//! | strip               | `data-table-pagination`                    |
//! | row count           | `data-table-pagination-row-count`          |
//! | estimate overshoot  | `data-table-pagination-estimate-overshot`  |
//! | estimate unavailable| `data-table-pagination-estimate-unavailable` |
//! | unstable notice     | `data-table-pagination-unstable`           |
//! | paging mode         | `data-table-pagination-mode`               |
//! | paging reason       | `data-table-pagination-reason`             |
//! | drift notice        | `data-table-pagination-drift`              |
//! | count exact         | `data-table-pagination-count-exact`        |
//! | per-page group      | `data-table-pagination-per-page`           |
//! | page-size picker    | `data-table-pagination-page-size`          |
//! | nav group           | `data-table-pagination-nav`                |
//! | previous page       | `data-table-pagination-prev`               |
//! | page label          | `data-table-pagination-page`               |
//! | next page           | `data-table-pagination-next`               |
//!
//! # Ported logic, not just markup
//! [`total_rows_view`], [`format_row_count`], [`can_page_forward`],
//! [`paging_hint_key`], [`show_paging_reason`], [`show_paging_label`] and
//! [`paging_label_key`] are line-for-line ports of `lib/totalRowsDisplay.ts` and
//! the helpers at the top of `Pagination.tsx`, including the rule that only an
//! exact total may clamp the page (an estimate may be smaller than the data).
//!
//! # Deliberate gaps
//! * The React `useEffect` that resets the page when `filterRevision` changes has
//!   no Rune equivalent: a Rune view is a pure function of the app state, so the
//!   host resets the page in the handler that changes the filter. There is no
//!   `filterRevision`/`onPageReset` prop here for that reason.
//! * `aria-busy` has no rune hook (`el/a11y.rs` exposes label/description/… but
//!   no busy); `loading` is carried by the disabled controls instead.
//! * `hidden sm:flex` (the page-size picker hidden below 640pt) has no rune
//!   breakpoint; the minimum window is 960pt, so the picker is always mounted.
//! * `title=` becomes `El::help`, `data-testid=` becomes the element key.

use std::rc::Rc;

use rune::{row, text, Align, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton};

use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT};
use crate::ui::kit::kit;
use crate::ui::select::Select;

/// The page sizes the React select offers, in order.
const PAGE_SIZES: [usize; 5] = [25, 50, 100, 200, 500];

/// `totalRowsKind` of the React props (default: exact, as `totalRowsView` does).
// Every kind is reachable from a back end; the DataTable view that selects one
// lands in a later migration task and `tests/data_table_parts.rs` covers them now.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TotalRowsKind {
    #[default]
    Exact,
    Estimated,
    Unsupported,
}

/// The three states of a row count (`lib/totalRowsDisplay.ts`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TotalRowsView {
    Exact(f64),
    Estimated(f64),
    Unknown,
}

impl TotalRowsView {
    pub(crate) fn value(self) -> f64 {
        match self {
            TotalRowsView::Exact(value) | TotalRowsView::Estimated(value) => value,
            TotalRowsView::Unknown => 0.0,
        }
    }

    pub(crate) fn is_unknown(self) -> bool {
        matches!(self, TotalRowsView::Unknown)
    }
}

/// `totalRowsView(kind, totalRows)`.
pub(crate) fn total_rows_view(kind: TotalRowsKind, total_rows: usize) -> TotalRowsView {
    match kind {
        TotalRowsKind::Estimated => TotalRowsView::Estimated(total_rows as f64),
        TotalRowsKind::Unsupported => TotalRowsView::Unknown,
        TotalRowsKind::Exact => TotalRowsView::Exact(total_rows as f64),
    }
}

/// `formatRowCount`: thousands separators under a million, then `1.2M` / `3.4B`.
pub(crate) fn format_row_count(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let count = value.max(0.0).floor();
    if count < 1_000_000.0 {
        let digits = count as u64;
        let digits = digits.to_string();
        let mut out = String::with_capacity(digits.len() + digits.len() / 3);
        for (index, ch) in digits.chars().enumerate() {
            if index > 0 && (digits.len() - index) % 3 == 0 {
                out.push(',');
            }
            out.push(ch);
        }
        out
    } else if count < 1_000_000_000.0 {
        format!("{:.1}M", count / 1_000_000.0)
    } else {
        format!("{:.1}B", count / 1_000_000_000.0)
    }
}

/// `canPageForward`: an exact total closes the grid, an estimate never does, and
/// the unknown state only trusts a full page.
pub(crate) fn can_page_forward(view: TotalRowsView, row_count: usize, page_size: usize) -> bool {
    match view {
        TotalRowsView::Unknown => page_size > 0 && row_count >= page_size,
        TotalRowsView::Estimated(_) => true,
        TotalRowsView::Exact(value) => value > row_count as f64,
    }
}

/// The reasons the back end can give for keyset paging being unavailable.
// The back end produces every variant; the view that maps it lands later.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeysetUnavailableReason {
    DriverUnsupported,
    NoStableOrder,
    NullableKeyColumn,
    NullBoundary,
    PredicateUnavailable,
    NoPaginationClause,
    StaleCursor,
    NotAdjacent,
}

impl KeysetUnavailableReason {
    fn key_name(self) -> &'static str {
        match self {
            KeysetUnavailableReason::DriverUnsupported => "driverUnsupported",
            KeysetUnavailableReason::NoStableOrder => "noStableOrder",
            KeysetUnavailableReason::NullableKeyColumn => "nullableKeyColumn",
            KeysetUnavailableReason::NullBoundary => "nullBoundary",
            KeysetUnavailableReason::PredicateUnavailable => "predicateUnavailable",
            KeysetUnavailableReason::NoPaginationClause => "noPaginationClause",
            KeysetUnavailableReason::StaleCursor => "staleCursor",
            KeysetUnavailableReason::NotAdjacent => "notAdjacent",
        }
    }
}

/// `pagingHintKey`: every reason has its own `pagination.hint.<reason>` key; the
/// one exception is `notAdjacent`, which says the cursor re-anchored.
pub(crate) fn paging_hint_key(reason: KeysetUnavailableReason) -> String {
    if reason == KeysetUnavailableReason::NotAdjacent {
        "pagination.hint.jumpReanchored".to_owned()
    } else {
        format!("pagination.hint.{}", reason.key_name())
    }
}

/// `showPagingReason`: a database that has no keyset paging at all is not a
/// warning the user needs on every page.
pub(crate) fn show_paging_reason(reason: KeysetUnavailableReason) -> bool {
    reason != KeysetUnavailableReason::DriverUnsupported
}

/// How the current page is paged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PagingMode {
    Keyset,
    #[default]
    Offset,
}

/// What the back end said about the paging of this page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PagingHint {
    pub(crate) reason: Option<KeysetUnavailableReason>,
    pub(crate) drift_suspected: bool,
    pub(crate) mode: PagingMode,
}

/// `showPagingLabel`: the mode shows once the user can actually page, or when a
/// reason explains why they cannot.
pub(crate) fn show_paging_label(hint: PagingHint, has_more_than_one_page: bool) -> bool {
    if let Some(reason) = hint.reason {
        if show_paging_reason(reason) {
            return true;
        }
    }
    hint.mode == PagingMode::Keyset && has_more_than_one_page
}

/// `pagingLabelKey`.
pub(crate) fn paging_label_key(hint: PagingHint) -> &'static str {
    match hint.mode {
        PagingMode::Keyset => "pagination.mode.keyset",
        PagingMode::Offset => "pagination.mode.offset",
    }
}

type PageFn<S> = Rc<dyn Fn(&mut S, usize)>;
type PressFn<S> = Rc<dyn Fn(&mut S)>;

/// The page strip. Build it with the state the view already owns, then hand the
/// callbacks the next page / page size.
pub(crate) struct Pagination<S: 'static> {
    page: usize,
    page_size: usize,
    total_rows: usize,
    kind: TotalRowsKind,
    row_count: Option<usize>,
    has_more: Option<bool>,
    loading: bool,
    counting: bool,
    unstable_paging: bool,
    estimate_unavailable: bool,
    hint: Option<PagingHint>,
    page_size_open: bool,
    on_page_change: Option<PageFn<S>>,
    on_page_size_change: Option<PageFn<S>>,
    on_page_size_toggle: Option<PressFn<S>>,
    on_page_size_close: Option<PressFn<S>>,
    on_count_exact: Option<PressFn<S>>,
}

// The DataTable view that calls this lands in a later migration task;
// `tests/data_table_parts.rs` is the caller today.
#[allow(dead_code)]
impl<S: 'static> Pagination<S> {
    pub(crate) fn new(page: usize, page_size: usize, total_rows: usize) -> Pagination<S> {
        Pagination {
            page,
            page_size,
            total_rows,
            kind: TotalRowsKind::Exact,
            row_count: None,
            has_more: None,
            loading: false,
            counting: false,
            unstable_paging: false,
            estimate_unavailable: false,
            hint: None,
            page_size_open: false,
            on_page_change: None,
            on_page_size_change: None,
            on_page_size_toggle: None,
            on_page_size_close: None,
            on_count_exact: None,
        }
    }

    /// The three states of the row count (`totalRowsKind`).
    pub(crate) fn total_rows_kind(mut self, kind: TotalRowsKind) -> Pagination<S> {
        self.kind = kind;
        self
    }

    /// Rows in the page on screen; `None` means the back end did not say.
    pub(crate) fn row_count(mut self, row_count: Option<usize>) -> Pagination<S> {
        self.row_count = row_count;
        self
    }

    /// The back end's `hasMore`; overrides the derived forward rule.
    pub(crate) fn has_more(mut self, has_more: Option<bool>) -> Pagination<S> {
        self.has_more = has_more;
        self
    }

    pub(crate) fn loading(mut self, loading: bool) -> Pagination<S> {
        self.loading = loading;
        self
    }

    pub(crate) fn counting(mut self, counting: bool) -> Pagination<S> {
        self.counting = counting;
        self
    }

    pub(crate) fn unstable_paging(mut self, unstable: bool) -> Pagination<S> {
        self.unstable_paging = unstable;
        self
    }

    pub(crate) fn estimate_unavailable(mut self, unavailable: bool) -> Pagination<S> {
        self.estimate_unavailable = unavailable;
        self
    }

    pub(crate) fn paging_hint(mut self, hint: Option<PagingHint>) -> Pagination<S> {
        self.hint = hint;
        self
    }

    /// Whether the page-size picker's listbox is open (the app owns that state,
    /// as it does for every `ui::select::Select`).
    pub(crate) fn page_size_open(mut self, open: bool) -> Pagination<S> {
        self.page_size_open = open;
        self
    }

    /// Previous / next / page-size reset all arrive here with the target page.
    pub(crate) fn on_page_change(mut self, f: impl Fn(&mut S, usize) + 'static) -> Pagination<S> {
        self.on_page_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_page_size_change(
        mut self,
        f: impl Fn(&mut S, usize) + 'static,
    ) -> Pagination<S> {
        self.on_page_size_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_page_size_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Pagination<S> {
        self.on_page_size_toggle = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_page_size_close(mut self, f: impl Fn(&mut S) + 'static) -> Pagination<S> {
        self.on_page_size_close = Some(Rc::new(f));
        self
    }

    /// The `onCountExact` action, shown only while the total is an estimate.
    pub(crate) fn on_count_exact(mut self, f: impl Fn(&mut S) + 'static) -> Pagination<S> {
        self.on_count_exact = Some(Rc::new(f));
        self
    }

    /// Builds the strip. `layer` receives the page-size listbox while it is open.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let c = &k.c;
        let page_size = self.page_size.max(1);
        let view = total_rows_view(self.kind, self.total_rows);
        let rows_on_page = self.row_count.unwrap_or(0);
        let rows_known = self.row_count.is_some();
        // No number of pages without a number: zero would say "one page" and pin
        // the user to the first one forever.
        let total_pages = if view.is_unknown() {
            0
        } else {
            ((view.value() / page_size as f64).ceil() as usize).max(1)
        };
        // Only an exact total may clamp the page; an estimate never moves the user.
        let current = match view {
            TotalRowsView::Exact(_) => self.page.min(total_pages.saturating_sub(1)),
            _ => self.page,
        };
        let (from, to) = match view {
            TotalRowsView::Unknown => {
                let from = if rows_on_page > 0 {
                    current * page_size + 1
                } else {
                    0
                };
                (from, current * page_size + rows_on_page)
            }
            TotalRowsView::Estimated(value) if rows_known => {
                let from = if rows_on_page > 0 {
                    current * page_size + 1
                } else {
                    0
                };
                let _ = value;
                (from, current * page_size + rows_on_page)
            }
            TotalRowsView::Exact(value) | TotalRowsView::Estimated(value) => {
                let from = if value == 0.0 {
                    0
                } else {
                    current * page_size + 1
                };
                (from, (value as usize).min((current + 1) * page_size))
            }
        };
        let row_count_text = match view {
            TotalRowsView::Exact(value) => {
                format!("{from}-{to} / {}", format_row_count(value))
            }
            TotalRowsView::Estimated(value) => format!(
                "{from}-{to} / {}",
                t("pagination.rowCountEstimated")
                    .replace("{count}", &format_row_count(value))
            ),
            TotalRowsView::Unknown => {
                format!("{from}-{to} / {}", t("pagination.rowCountUnknown"))
            }
        };
        let estimate_tooltip = match view {
            TotalRowsView::Estimated(_) => Some(t("pagination.estimateTooltip")),
            _ => None,
        };
        // The estimate may be smaller than the real count: past its end the page
        // is empty through no fault of the user.
        let estimate_overshot =
            matches!(view, TotalRowsView::Estimated(_)) && rows_on_page == 0 && self.page > 0;
        let can_go_forward = match view {
            TotalRowsView::Exact(_) => current < total_pages - 1,
            _ => self
                .has_more
                .unwrap_or_else(|| can_page_forward(view, rows_on_page, page_size)),
        };
        let page_label = if view.is_unknown() {
            t("pagination.pageOnly").replace("{page}", &(current + 1).to_string())
        } else {
            format!(
                "{} {} / {} {}",
                t("pagination.page"),
                current + 1,
                total_pages,
                t("pagination.pageOf")
            )
        };

        // Left: the row count, then the two estimate notices.
        let mut left = row()
            .key("data-table-pagination-count-group")
            .min_w(0.0)
            .h_full()
            .gap(8.0)
            .center_y();
        let mut count = text(row_count_text)
            .key("data-table-pagination-row-count")
            .font(k.ty.text_xsmall)
            .color(c.text_muted);
        if let Some(tooltip) = estimate_tooltip {
            count = count.help(tooltip).label(tooltip);
        }
        left = left.child(count);
        if estimate_overshot {
            left = left.child(
                text(t("pagination.estimatedLessThanActual"))
                    .key("data-table-pagination-estimate-overshot")
                    .font(k.ty.text_xsmall)
                    .color(c.warning.fg)
                    .max_w(352.0)
                    .truncate()
                    .help(t("pagination.estimatedLessThanActual")),
            );
        }
        if self.estimate_unavailable {
            left = left.child(
                text(t("pagination.estimateUnavailable"))
                    .key("data-table-pagination-estimate-unavailable")
                    .font(k.ty.text_xsmall)
                    .color(c.warning.fg)
                    .max_w(352.0)
                    .truncate()
                    .help(t("pagination.estimateUnavailable")),
            );
        }

        // Right: notices, the count action, the page-size picker and the nav.
        let mut right = row()
            .key("data-table-pagination-controls")
            .h_full()
            .gap(8.0)
            .center_y();
        if self.unstable_paging {
            right = right.child(
                text(t("pagination.unstablePaging"))
                    .key("data-table-pagination-unstable")
                    .font(k.ty.text_xsmall)
                    .color(c.warning.fg)
                    .max_w(352.0)
                    .truncate()
                    .help(t("pagination.unstablePaging")),
            );
        }
        if let Some(hint) = self.hint {
            if show_paging_label(hint, self.total_rows > page_size) {
                let label = paging_label_key(hint);
                right = right.child(
                    text(t(label))
                        .key("data-table-pagination-mode")
                        .font(k.ty.text_xsmall)
                        .color(c.text_faint)
                        .max_w(192.0)
                        .truncate()
                        .help(t(label)),
                );
            }
            if let Some(reason) = hint.reason {
                if show_paging_reason(reason) {
                    let key = paging_hint_key(reason);
                    let text_value = t(&key).replace("{page}", &(current + 1).to_string());
                    right = right.child(
                        text(&text_value)
                            .key("data-table-pagination-reason")
                            .font(k.ty.text_xsmall)
                            .color(c.warning.fg)
                            .max_w(352.0)
                            .truncate()
                            .help(&text_value),
                    );
                }
            }
            if hint.drift_suspected {
                right = right.child(
                    text(t("pagination.hint.drift"))
                        .key("data-table-pagination-drift")
                        .font(k.ty.text_xsmall)
                        .color(c.warning.fg)
                        .max_w(352.0)
                        .truncate()
                        .help(t("pagination.hint.drift")),
                );
            }
        }
        if matches!(view, TotalRowsView::Estimated(_)) {
            if let Some(action) = &self.on_count_exact {
                let label = if self.counting {
                    t("pagination.counting")
                } else {
                    t("pagination.countExact")
                };
                let action = Rc::clone(action);
                right = right.child(
                    Button::new(label)
                        .variant(ButtonVariant::Neutral)
                        .control_size(ControlSize::Large)
                        .disabled(self.loading || self.counting)
                        .on_press(move |state: &mut S| action(state))
                        .build(k)
                        .key("data-table-pagination-count-exact"),
                );
            }
        }

        // Per page: label + the five sizes.
        let options: Vec<String> = PAGE_SIZES.iter().map(|size| size.to_string()).collect();
        let current_size = page_size.to_string();
        let mut per_page = row()
            .key("data-table-pagination-per-page")
            .gap(8.0)
            .center_y()
            .child(
                text(t("pagination.perPage"))
                    .font(k.ty.text_xsmall)
                    .color(c.text_muted),
            );
        let mut select = Select::new("data-table-pagination-page-size", &options)
            .current(Some(&current_size))
            .width(92.0)
            .disabled(self.loading)
            .open(self.page_size_open);
        match (&self.on_page_size_toggle, self.loading) {
            (Some(toggle), _) => {
                let toggle = Rc::clone(toggle);
                select = select.on_toggle(move |state: &mut S| toggle(state));
            }
            // A host that did not wire the picker still gets the field; opening it
            // is a no-op rather than a debug assertion.
            (None, false) => select = select.on_toggle(|_: &mut S| {}),
            (None, true) => {}
        }
        if let Some(close) = &self.on_page_size_close {
            let close = Rc::clone(close);
            select = select.on_close(move |state: &mut S| close(state));
        }
        if let (Some(choose), true) = (&self.on_page_size_change, !self.loading) {
            let choose = Rc::clone(choose);
            select = select.on_choose(move |state: &mut S, label: &str, _tx: &mut Tx| {
                if let Ok(size) = label.parse::<usize>() {
                    choose(state, size);
                }
            });
        }
        per_page = per_page.child(select.build(k, layer));
        right = right.child(per_page);

        // Previous / page label / next.
        let mut nav = row()
            .key("data-table-pagination-nav")
            .gap(4.0)
            .center_y();
        let mut prev = IconButton::new(&ICON_CHEVRON_LEFT)
            .control_size(ControlSize::Large)
            .label(t("pagination.prev"))
            .disabled(self.loading || current == 0);
        if let Some(change) = &self.on_page_change {
            let change = Rc::clone(change);
            prev = prev.on_press(move |state: &mut S| change(state, current.saturating_sub(1)));
        }
        nav = nav.child(prev.build(k).key("data-table-pagination-prev"));
        nav = nav.child(
            text(page_label)
                .key("data-table-pagination-page")
                .font(k.ty.text_xsmall.features("'tnum' 1"))
                .color(c.text_muted)
                .w(120.0)
                .align(Align::Center)
                .truncate(),
        );
        let mut next = IconButton::new(&ICON_CHEVRON_RIGHT)
            .control_size(ControlSize::Large)
            .label(t("pagination.next"))
            .disabled(self.loading || !can_go_forward);
        if let Some(change) = &self.on_page_change {
            let change = Rc::clone(change);
            next = next.on_press(move |state: &mut S| change(state, current + 1));
        }
        nav = nav.child(next.build(k).key("data-table-pagination-next"));
        right = right.child(nav);

        row()
            .key("data-table-pagination")
            .w_full()
            .h(40.0)
            .px(12.0)
            .gap(12.0)
            .justify_between()
            .center_y()
            .fill(c.layer_01)
            .border_sides([1.0, 0.0, 0.0, 0.0], c.border_weak)
            .child(left)
            .child(right)
    }
}
