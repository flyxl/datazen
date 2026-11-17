//! The DataZen table: a keyed grid over rows the view already owns, with a sticky header,
//! per-column width and alignment, a sorted order, a selected row and an empty state.
//!
//! `rune_kit::Table` cannot carry DataZen's key contract: it keys its root, header and cells
//! by the app's row and column ids and expects a `TableState` in the app, so a wrapper cannot
//! name `<key>-row-<i>` or `<key>-cell-<i>-<j>`. Like `ui::dialog`, this base therefore mirrors
//! the kit table's build on the same [`TableStyle`] (row 28, header 30, cell pad 10, the header
//! rule, the sort chevron, the selected fill) and adds DataZen's positional keys and handlers.
//!
//! The view passes the rows complete; the table sorts a copy of the order itself (stable,
//! numbers compared as numbers inside a cell) and every index it takes or reports is a
//! rendered-order index: after `sort`, row 0 is the first row on screen. The table owns no
//! state, so selection, sort and activation stay in the app and arrive on every build.
//!
//! | part        | key                  |
//! |-------------|----------------------|
//! | container   | `<key>`              |
//! | header row  | `<key>-head`         |
//! | header cell | `<key>-col-<i>`      |
//! | body        | `<key>-body`         |
//! | row         | `<key>-row-<i>`      |
//! | cell        | `<key>-cell-<i>-<j>` |
//! | empty state | `<key>-empty`        |
//!
//! Keyboard: with `on_select`, the container is the single tab stop; Down and Up focus the
//! first and last row, and a focused row passes Up/Down/Home/End to its ancestors, which move
//! the focus between rows (clamped). Enter and Space activate the focused row through
//! `on_activate`, with that row's rendered index. Pointer behaviour follows the kit: a press
//! selects the row and focuses it, a double press activates it.
//!
//! Nothing here virtualises: every row becomes an element, so a view should hand over the
//! page it shows. The kit table's `virtual_rows` needs the app's row state and a row-key
//! function, which this static base deliberately does not own; a virtualised DataZen table is
//! a separate base.

// The views that call this land in later migration tasks; `tests/table.rs` is the only
// caller today, so the lib target would otherwise warn about every item.
#![allow(dead_code)]

use std::cmp::Ordering;
use std::rc::Rc;

use rune::{col, el, icon, row, text, Align, El, Key, KeyEvent, ScrollSpec, Tx};
use rune_kit::{SortDir, TableStyle};

use super::kit::kit;

/// Wraps a built row: the select model adds the press handler and a focus-move layer around
/// the row, so the row keeps its single `on_key` for activation.
type RowWrap = Box<dyn Fn(usize, &str, usize) -> Box<dyn FnOnce(El) -> El>>;
/// Wraps the container with the focus fallback (Down/Up while the container itself is focused).
type RootWrap = Box<dyn Fn(&str, usize) -> Box<dyn FnOnce(El) -> El>>;
/// Wraps a built row with the activation keys and the double press.
type ActivateWrap = Box<dyn Fn(usize) -> Box<dyn FnOnce(El) -> El>>;

/// What `on_select` contributes: pointer presses, per-row focus movement and the container
/// fallback.
struct SelectBinds {
    row: RowWrap,
    root: RootWrap,
}

/// One column: its title, an optional fixed width and the alignment of its cells.
#[derive(Clone, Debug)]
pub(crate) struct Column {
    pub(crate) title: String,
    /// `None` takes an equal share of what the fixed columns leave.
    pub(crate) width: Option<f32>,
    pub(crate) align: Align,
}

impl Column {
    pub(crate) fn new(title: impl Into<String>) -> Column {
        Column {
            title: title.into(),
            width: None,
            align: Align::Start,
        }
    }

    /// A fixed width; columns without one share the rest equally.
    pub(crate) fn width(mut self, width: f32) -> Column {
        self.width = Some(width);
        self
    }

    pub(crate) fn align(mut self, align: Align) -> Column {
        self.align = align;
        self
    }

    /// A numeric column: cells hug the right edge.
    pub(crate) fn end(self) -> Column {
        self.align(Align::End)
    }
}

/// One cell's content. `mono` and `muted` follow the kit table's cell variants.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CellValue {
    pub(crate) text: String,
    pub(crate) mono: bool,
    pub(crate) muted: bool,
}

impl CellValue {
    pub(crate) fn new(text: impl Into<String>) -> CellValue {
        CellValue {
            text: text.into(),
            mono: false,
            muted: false,
        }
    }

    pub(crate) fn mono(mut self) -> CellValue {
        self.mono = true;
        self
    }

    pub(crate) fn muted(mut self) -> CellValue {
        self.muted = true;
        self
    }
}

impl From<&str> for CellValue {
    fn from(text: &str) -> CellValue {
        CellValue::new(text)
    }
}

impl From<String> for CellValue {
    fn from(text: String) -> CellValue {
        CellValue::new(text)
    }
}

impl From<&String> for CellValue {
    fn from(text: &String) -> CellValue {
        CellValue::new(text.clone())
    }
}

/// A row: its cells and an optional app key. The element key stays positional
/// (`<key>-row-<i>`); the app key is what `row_key` hands back.
#[derive(Clone, Debug, Default)]
pub(crate) struct Row {
    pub(crate) key: Option<String>,
    pub(crate) cells: Vec<CellValue>,
}

impl Row {
    /// A key the app can use to match a rendered row to its data (`DataTable::row_key`).
    pub(crate) fn key(mut self, key: impl Into<String>) -> Row {
        self.key = Some(key.into());
        self
    }
}

impl From<Vec<CellValue>> for Row {
    fn from(cells: Vec<CellValue>) -> Row {
        Row { key: None, cells }
    }
}

impl From<Vec<String>> for Row {
    fn from(cells: Vec<String>) -> Row {
        Row {
            key: None,
            cells: cells.into_iter().map(CellValue::from).collect(),
        }
    }
}

impl From<Vec<&str>> for Row {
    fn from(cells: Vec<&str>) -> Row {
        Row {
            key: None,
            cells: cells.into_iter().map(CellValue::from).collect(),
        }
    }
}

/// A table over rows the view owns: a sticky header, sorted rows, a selected row.
pub(crate) struct DataTable<'a> {
    key: String,
    columns: &'a [Column],
    rows: Vec<Row>,
    sort: Option<(usize, SortDir)>,
    selected: Option<usize>,
    empty_text: Option<String>,
    height: Option<f32>,
    grow: bool,
    label: Option<String>,
    select: Option<SelectBinds>,
    activate: Option<ActivateWrap>,
}

impl<'a> DataTable<'a> {
    pub(crate) fn new(key: impl Into<String>, columns: &'a [Column]) -> DataTable<'a> {
        DataTable {
            key: key.into(),
            columns,
            rows: Vec::new(),
            sort: None,
            selected: None,
            empty_text: None,
            height: None,
            grow: false,
            label: None,
            select: None,
            activate: None,
        }
    }

    /// The rows to show, already in the app's base order (the table sorts a copy).
    pub(crate) fn rows<R: Into<Row>>(mut self, rows: impl IntoIterator<Item = R>) -> DataTable<'a> {
        self.rows = rows.into_iter().map(Into::into).collect();
        self
    }

    /// The sort shown in the header and applied to the rows: column index and direction.
    pub(crate) fn sort(mut self, column: usize, dir: SortDir) -> DataTable<'a> {
        self.sort = Some((column, dir));
        self
    }

    /// The rendered-order index of the selected row: it gets the selection fill.
    pub(crate) fn selected(mut self, selected: Option<usize>) -> DataTable<'a> {
        self.selected = selected;
        self
    }

    /// Replaces the empty state's text (defaults to `dataTable.empty`).
    pub(crate) fn empty_text(mut self, empty_text: impl Into<String>) -> DataTable<'a> {
        self.empty_text = Some(empty_text.into());
        self
    }

    /// A fixed height. Without it the table grows with its rows, or fills the parent when
    /// `grow` is set.
    pub(crate) fn height(mut self, height: f32) -> DataTable<'a> {
        self.height = Some(height);
        self
    }

    /// Take the parent's remaining height; the body scrolls inside it.
    pub(crate) fn grow(mut self) -> DataTable<'a> {
        self.grow = true;
        self
    }

    /// The accessible name.
    pub(crate) fn label(mut self, label: impl Into<String>) -> DataTable<'a> {
        self.label = Some(label.into());
        self
    }

    /// A row press (select and focus) and the focus movement keys. `f(state, row)` receives
    /// the rendered row index of the row that was pressed or focused.
    pub(crate) fn on_select<S: 'static>(
        mut self,
        f: impl Fn(&mut S, usize) + 'static,
    ) -> DataTable<'a> {
        let f: Rc<dyn Fn(&mut S, usize)> = Rc::new(f);
        let row_f = Rc::clone(&f);
        let root_f = Rc::clone(&f);
        self.select = Some(SelectBinds {
            row: Box::new(move |i, key, len| {
                let key = key.to_owned();
                let press_f = Rc::clone(&row_f);
                let nav_f = Rc::clone(&row_f);
                let row_key = format!("{key}-row-{i}");
                Box::new(move |el: El| {
                    let el = el.on_press_tx(move |s: &mut S, tx: &mut Tx| {
                        tx.focus(row_key.clone(), false);
                        press_f(s, i);
                    });
                    let up = i.saturating_sub(1);
                    let down = (i + 1).min(len.saturating_sub(1));
                    col()
                        .w_full()
                        .on_key(move |s: &mut S, ev: &KeyEvent, tx: &mut Tx| {
                            let target = match ev.key {
                                Key::Up => up,
                                Key::Down => down,
                                Key::Home => 0,
                                Key::End => len.saturating_sub(1),
                                _ => return false,
                            };
                            tx.focus(format!("{key}-row-{target}"), false);
                            nav_f(s, target);
                            true
                        })
                        .child(el)
                })
            }),
            root: Box::new(move |key, len| {
                let key = key.to_owned();
                let f = Rc::clone(&root_f);
                Box::new(move |el: El| {
                    el.on_key(move |s: &mut S, ev: &KeyEvent, tx: &mut Tx| {
                        if len == 0 {
                            return false;
                        }
                        let target = match ev.key {
                            Key::Down => 0,
                            Key::Up => len - 1,
                            Key::Home => 0,
                            Key::End => len - 1,
                            _ => return false,
                        };
                        tx.focus(format!("{key}-row-{target}"), false);
                        f(s, target);
                        true
                    })
                })
            }),
        });
        self
    }

    /// Enter, Space and a double press activate a row: `f(state, row)` receives the rendered
    /// row index that has focus (or was double-pressed).
    pub(crate) fn on_activate<S: 'static>(
        mut self,
        f: impl Fn(&mut S, usize) + 'static,
    ) -> DataTable<'a> {
        let f: Rc<dyn Fn(&mut S, usize)> = Rc::new(f);
        self.activate = Some(Box::new(move |i| {
            let key_f = Rc::clone(&f);
            let double_f = Rc::clone(&f);
            Box::new(move |el: El| {
                el.on_key(move |s: &mut S, ev: &KeyEvent, _: &mut Tx| {
                    let space = matches!(&ev.key, Key::Text(t) if t.as_str() == " ");
                    if ev.key == Key::Enter || space {
                        key_f(s, i);
                        true
                    } else {
                        false
                    }
                })
                .on_double_press_tx(move |s: &mut S, _: &mut Tx| double_f(s, i))
            })
        }));
        self
    }

    /// The app key of the row at this rendered index, when the row was given one.
    pub(crate) fn row_key(&self, rendered: usize) -> Option<&str> {
        let source = order(&self.rows, self.sort).get(rendered).copied()?;
        self.rows.get(source)?.key.as_deref()
    }

    /// Builds the table for one appearance.
    pub(crate) fn build(self, dark: bool) -> El {
        let DataTable {
            key,
            columns,
            rows,
            sort,
            selected,
            empty_text,
            height,
            grow,
            label,
            select,
            activate,
        } = self;
        let k = kit(dark);
        let style = TableStyle::from_kit(k);
        let order = order(&rows, sort);
        let len = rows.len();
        let body_h = if len == 0 {
            44.0
        } else {
            len as f32 * style.row_height
        };

        let head = header(&key, columns, sort, &style, k);
        let body = body(
            &key,
            columns,
            &rows,
            &order,
            selected,
            empty_text.as_deref(),
            &style,
            &select,
            &activate,
        );

        let mut root = col().key(key.clone()).role("grid").w_full().clip_soft(0.0);
        root = match height {
            Some(h) => root.h(h),
            None if grow => root.grow(1.0).min_h(0.0),
            None => root.h(style.header_height + body_h),
        };
        if let Some(label) = label {
            root = root.label(label);
        }
        if let Some(select) = &select {
            root = root.focusable(true);
            root = (select.root)(&key, len)(root);
        }
        root.child(head).child(body)
    }
}

/// The header row: one column header per column, the rule under it and between columns.
fn header(
    key: &str,
    columns: &[Column],
    sort: Option<(usize, SortDir)>,
    style: &TableStyle,
    k: &rune_kit::Kit,
) -> El {
    let mut head = row()
        .key(format!("{key}-head"))
        .role("row")
        .w_full()
        .h(style.header_height)
        .child(
            el().key(format!("{key}-head-rule"))
                .pin(Some(0.0), None, Some(0.0), Some(0.0))
                .h(1.0)
                .fill(style.rule)
                .a11y_hidden(),
        );
    let last = columns.len().saturating_sub(1);
    for (i, c) in columns.iter().enumerate() {
        let sorted = sort.filter(|(col, _)| *col == i).map(|(_, dir)| dir);
        let title = text(c.title.clone())
            .key(format!("{key}-col-{i}-title"))
            .font(style.header_font)
            .color(if sorted.is_some() {
                style.text
            } else {
                style.header_text
            })
            .truncate()
            .shrink()
            .min_w(0.0)
            .help_when_truncated(c.title.clone());
        let mut inner = row()
            .key(format!("{key}-col-{i}-inner"))
            .gap(2.0)
            .center_y()
            .min_w(0.0)
            .shrink()
            .child(title)
            .a11y_hidden();
        if let Some(dir) = sorted {
            let glyph = if dir == SortDir::Ascending {
                k.glyph.chevron_up
            } else {
                k.glyph.chevron_down
            };
            inner = inner.child(
                icon(glyph, 14.0)
                    .key(format!("{key}-col-{i}-sort"))
                    .color(style.text),
            );
        }
        let cell = sized(
            row()
                .role("columnheader")
                .label(c.title.clone())
                .sorted(sorted.map(|dir| dir == SortDir::Ascending))
                .h_full()
                .px(style.cell_pad)
                .center_y(),
            c,
        )
        .key(format!("{key}-col-{i}"))
        .child(inner);
        let mut cell = justify(cell, c.align);
        if i != last {
            cell = cell.child(
                el().key(format!("{key}-col-{i}-rule"))
                    .pin(None, Some(8.0), Some(0.0), Some(8.0))
                    .w(1.0)
                    .fill(style.rule)
                    .a11y_hidden(),
            );
        }
        head = head.child(cell);
    }
    head
}

#[allow(clippy::too_many_arguments)]
fn body(
    key: &str,
    columns: &[Column],
    rows: &[Row],
    order: &[usize],
    selected: Option<usize>,
    empty_text: Option<&str>,
    style: &TableStyle,
    select: &Option<SelectBinds>,
    activate: &Option<ActivateWrap>,
) -> El {
    let mut body = col()
        .key(format!("{key}-body"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        });
    if rows.is_empty() {
        let empty = empty_text.unwrap_or_else(|| crate::i18n::t("dataTable.empty"));
        return body.child(
            el().key(format!("{key}-empty"))
                .role("status")
                .label(empty)
                .w_full()
                .h(44.0)
                .px(style.cell_pad)
                .center_y()
                .child(text(empty).font(style.font).color(style.muted)),
        );
    }
    // Rows are only focus targets when something can happen on Enter/Space. A
    // static table (no handlers) must not put empty shells in the tab order.
    let interactive = select.is_some() || activate.is_some();
    for (i, &source) in order.iter().enumerate() {
        let cells = &rows[source].cells;
        let is_selected = selected == Some(i);
        let mut r = row()
            .key(format!("{key}-row-{i}"))
            .role("row")
            .focusable(interactive)
            .tab_index(if select.is_some() { -1 } else { 0 })
            .focus_selected(is_selected)
            .w_full()
            .h(style.row_height)
            .center_y();
        r = if is_selected {
            r.fill(style.selected)
        } else {
            r.hover(style.stripe.mix(style.selected, 0.5))
        };
        for (j, c) in columns.iter().enumerate() {
            let value = cells.get(j).cloned().unwrap_or_default();
            r = r.child(cell(key, i, j, c, value, style));
        }
        if let Some(activate) = activate {
            r = (activate)(i)(r);
        }
        if let Some(select) = select {
            r = (select.row)(i, key, rows.len())(r);
        }
        body = body.child(r);
    }
    body
}

/// One body cell: the column's width, its padding, its alignment and the text.
fn cell(key: &str, i: usize, j: usize, c: &Column, value: CellValue, style: &TableStyle) -> El {
    let color = if value.muted { style.muted } else { style.text };
    let inner = text(value.text.clone())
        .key(format!("{key}-cell-{i}-{j}-text"))
        .font(if value.mono { style.mono } else { style.font })
        .color(color)
        .align(c.align)
        .truncate()
        .shrink()
        .min_w(0.0)
        .help_when_truncated(value.text);
    justify(
        sized(
            row()
                .role("gridcell")
                .h_full()
                .px(style.cell_pad)
                .center_y()
                .clip_soft(0.0),
            c,
        )
        .key(format!("{key}-cell-{i}-{j}"))
        .child(inner),
        c.align,
    )
}

/// A fixed column keeps its width; a flexible one grows into what is left.
fn sized(e: El, c: &Column) -> El {
    match c.width {
        Some(w) => e.w(w),
        None => e.w(0.0).grow(1.0).shrink(),
    }
}

fn justify(e: El, align: Align) -> El {
    match align {
        Align::Start => e,
        Align::Center => e.justify_center(),
        Align::End => e.justify_end(),
    }
}

/// The rendered order: every row index, sorted by the chosen column (stable).
fn order(rows: &[Row], sort: Option<(usize, SortDir)>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    if let Some((column, dir)) = sort {
        order.sort_by(|&a, &b| {
            dir.apply(compare(
                cell_text(&rows[a], column),
                cell_text(&rows[b], column),
            ))
        });
    }
    order
}

fn cell_text(row: &Row, column: usize) -> &str {
    row.cells.get(column).map(|c| c.text.as_str()).unwrap_or("")
}

/// Numbers compare as numbers, everything else case-insensitively, then verbatim.
fn compare(a: &str, b: &str) -> Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a
            .to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b)),
    }
}
