//! Port of `packages/frontend/src/hooks/useColumnResize.ts` (T185, DB-12).
//!
//! Column widths that belong to a **column**, not to a position: every
//! dragged width is filed under the column's name, seeded from the
//! persisted map, and reported whole on drag end — so a resized column
//! keeps its width across reorders, page changes and restarts.
//!
//! ## What maps where
//!
//! - `computeInitialColumnWidths` is the pure width heuristic, verbatim:
//!   the per-type defaults (bool 80 / serial-family 90 / numeric-family
//!   100 / timestamp 180 / json 260 / text-family 150 / else 150), the
//!   header reservation (`name.len() * 6.5 + 24 + SORT_CONTROL_WIDTH +
//!   8`), the 20-row data sample (objects/arrays stringify compact,
//!   scalars stringify, nulls skip) and the `[60, 400]` clamp with the
//!   type/header/data floors.
//! - `useMemo`'s deps (`[columns?.length, names.join(',')]`) are
//!   semantics, not just perf: rows changing does NOT recompute the
//!   smart widths — only a column-signature change does. That is the
//!   [`ColumnResizeState`] smart cache: keyed by the signature, so a
//!   re-render with new rows keeps the widths the columns were first
//!   measured with.
//! - `useState(resizedByName/resizedByIndex)` are the two maps on the
//!   state; the `useRef` mirrors are the same maps read from the drag
//!   (one owner in Rust — no ref/refresh split needed).
//! - `onResizeStart(colIndex, startX)` + the `pointermove`/`pointerup`
//!   listeners become [`ColumnResizeState::begin_drag`] +
//!   [`ColumnResizeDrag::on_move`]/[`ColumnResizeDrag::on_end`]: the
//!   host drives the pointer deltas it receives into the drag (the
//!   `document` listeners are the host's pointer plumbing; the body
//!   cursor/userSelect styling rides the host's drag surface).
//! - `onWidthsCommit` fires only when column names exist: without names
//!   there is nothing stable to key a width by (positional drags still
//!   work, they just cannot survive a reorder).
//! - `adjustWidthsForSort`: a single-column sort reuses the adaptive
//!   control (no change); a multi-sort adds the position badge width to
//!   each sorted column.
//!
//! Known deltas: JS `String.length` counts UTF-16 units; column names
//! and scalar cells count `char`s here (identical for the SQL-shaped
//! ASCII the heuristic serves). `JSON.stringify`'s insertion order vs
//! serde's key order does not change the string's length (the only
//! thing the heuristic reads).

use std::collections::BTreeMap;

/// The untyped fallback width (React `DEFAULT_COL_WIDTH`).
pub const DEFAULT_COL_WIDTH: f32 = 150.0;
/// The drag floor (React `MIN_COL_WIDTH`).
pub const MIN_COL_WIDTH: f32 = 60.0;
/// The measurement clamp (React `MAX_COL_WIDTH`).
pub const MAX_COL_WIDTH: f32 = 400.0;
/// Header char width estimate (React `HEADER_CHAR_PX`).
pub const HEADER_CHAR_PX: f32 = 6.5;
/// Data char width estimate (React `DATA_CHAR_PX`).
pub const DATA_CHAR_PX: f32 = 7.5;
/// Cell horizontal padding (React `CELL_PADDING`).
pub const CELL_PADDING: f32 = 24.0;
/// Header content gap (React `HEADER_CONTENT_GAP`).
pub const HEADER_CONTENT_GAP: f32 = 8.0;
/// How many rows the data sample measures (React `SAMPLE_ROWS`).
pub const SAMPLE_ROWS: usize = 20;
/// The always-present sort menu button's reservation.
pub const SORT_CONTROL_WIDTH: f32 = 24.0;
/// The multi-sort position badge's extra width.
pub const SORT_BADGE_EXTRA_WIDTH: f32 = 24.0;

/// One column's inputs for the width heuristic (React
/// `{ name, type? }`).
#[derive(Clone, Copy, Debug)]
pub struct ColumnWidthInput<'a> {
    pub name: &'a str,
    pub data_type: Option<&'a str>,
}

impl<'a> ColumnWidthInput<'a> {
    pub fn new(name: &'a str, data_type: Option<&'a str>) -> Self {
        Self { name, data_type }
    }
}

/// A sort the header shows (React `{ column, descending? }`; only the
/// column name matters for widths).
#[derive(Clone, Copy, Debug)]
pub struct ColumnSortSpec<'a> {
    pub column: &'a str,
    pub descending: Option<bool>,
}

/// `computeInitialColumnWidths` — the pure width heuristic, verbatim.
pub fn compute_initial_column_widths(
    columns: &[ColumnWidthInput<'_>],
    rows: &[Vec<serde_json::Value>],
) -> Vec<f32> {
    columns
        .iter()
        .enumerate()
        .map(|(col_idx, col)| {
            let t = col.data_type.unwrap_or_default().to_lowercase();

            let type_width = if t.contains("bool") {
                80.0
            } else if t.contains("serial")
                || t.contains("bigint")
                || t == "integer"
                || t == "int4"
                || t == "int2"
                || t == "smallint"
            {
                90.0
            } else if t.contains("numeric")
                || t.contains("decimal")
                || t.contains("float")
                || t.contains("double")
                || t.contains("real")
            {
                100.0
            } else if t.contains("timestamp") || t.contains("date") {
                180.0
            } else if t.contains("json") {
                260.0
            } else if t.contains("text") || t.contains("char") || t.contains("varchar") {
                150.0
            } else {
                DEFAULT_COL_WIDTH
            };

            // Reserve the always-present sort menu button before
            // sorting: it becomes the direction indicator after a sort,
            // so the label keeps the same usable width in both states.
            let header_width = col.name.chars().count() as f32 * HEADER_CHAR_PX
                + CELL_PADDING
                + SORT_CONTROL_WIDTH
                + HEADER_CONTENT_GAP;

            let mut data_width = 0.0;
            let sample_count = rows.len().min(SAMPLE_ROWS);
            for row in rows.iter().take(sample_count) {
                let Some(cell) = row.get(col_idx) else { continue };
                let Some(text) = cell_display(cell) else { continue };
                let w = text.chars().count() as f32 * DATA_CHAR_PX + CELL_PADDING;
                if w > data_width {
                    data_width = w;
                }
            }

            type_width.max(header_width).max(data_width).clamp(MIN_COL_WIDTH, MAX_COL_WIDTH)
        })
        .collect()
}

/// React `cell == null ? skip : typeof cell === 'object' ?
/// JSON.stringify(cell) : String(cell)` — objects/arrays stringify
/// compact, scalars stringify, nulls skip.
fn cell_display(cell: &serde_json::Value) -> Option<String> {
    match cell {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => Some(cell.to_string()),
        other => Some(other.to_string()),
    }
}

/// `useColumnResize`'s options (React `UseColumnResizeOptions`; the
/// commit callback is [`ColumnResizeDrag::on_end`]'s return — the host
/// persists what it hands back).
pub struct ColumnResizeOptions<'a> {
    /// Column definitions for smart width calculation.
    pub columns: Option<&'a [ColumnWidthInput<'a>]>,
    /// First page of rows for data-aware width calculation.
    pub rows: &'a [Vec<serde_json::Value>],
    /// Number of columns (the fallback when `columns` is absent).
    pub count: usize,
    /// Default width per column.
    pub default_width: f32,
    /// Minimum width per column (the drag floor).
    pub min_width: f32,
    /// The widths this table was last left with, by column name (DB-12).
    pub persisted_widths: Option<&'a BTreeMap<String, f32>>,
}

impl<'a> ColumnResizeOptions<'a> {
    /// React's defaults: `defaultWidth = 150`, `minWidth = 60`, no
    /// columns, no rows, no persisted widths.
    pub fn new(count: usize) -> Self {
        Self {
            columns: None,
            rows: &[],
            count,
            default_width: DEFAULT_COL_WIDTH,
            min_width: MIN_COL_WIDTH,
            persisted_widths: None,
        }
    }
}

/// The memoized smart widths (React's `smartWidths` `useMemo` keyed by
/// the column signature — a rows change does not recompute).
struct SmartCache {
    signature: String,
    widths: Vec<f32>,
}

/// The hook's state: the dragged widths by name and by position, plus
/// the signature-keyed smart cache.
#[derive(Default)]
pub struct ColumnResizeState {
    resized_by_name: BTreeMap<String, f32>,
    resized_by_index: BTreeMap<usize, f32>,
    smart: Option<SmartCache>,
}

impl ColumnResizeState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The memo key React's `useMemo` uses: the column count plus the
    /// names joined — identity of the column set, rows excluded.
    fn signature(columns: &[ColumnWidthInput<'_>]) -> String {
        let names: Vec<&str> = columns.iter().map(|c| c.name).collect();
        format!("{},{}", columns.len(), names.join(","))
    }

    fn ensure_smart(&mut self, opts: &ColumnResizeOptions<'_>) -> Option<&[f32]> {
        let columns = opts.columns.filter(|c| !c.is_empty())?;
        let signature = Self::signature(columns);
        let stale = self.smart.as_ref().is_none_or(|s| s.signature != signature);
        if stale {
            self.smart = Some(SmartCache {
                signature,
                widths: compute_initial_column_widths(columns, opts.rows),
            });
        }
        Some(self.smart.as_ref().map(|s| s.widths.as_slice()).unwrap_or_default())
    }

    /// The `columnWidths` memo: the smart (or default) widths with the
    /// dragged widths applied — by name when the columns carry names
    /// (`resized ?? persisted ?? smart ?? default`), by position
    /// otherwise.
    pub fn column_widths(&mut self, opts: &ColumnResizeOptions<'_>) -> Vec<f32> {
        // Compute (or hit the cache for) the smart widths first, then
        // read the dragged maps — the two borrows never overlap.
        let smart: Option<Vec<f32>> =
            self.ensure_smart(opts).map(<[f32]>::to_vec);
        let smart = smart.as_deref();
        let Some(columns) = opts.columns else {
            return (0..opts.count)
                .map(|i| {
                    self.resized_by_index
                        .get(&i)
                        .copied()
                        .unwrap_or_else(|| smart.map(|s| s[i]).unwrap_or(opts.default_width))
                })
                .collect();
        };
        columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                self.resized_by_name
                    .get(col.name)
                    .copied()
                    .or_else(|| {
                        opts.persisted_widths.and_then(|p| p.get(col.name).copied())
                    })
                    .or_else(|| smart.and_then(|s| s.get(i).copied()))
                    .unwrap_or(opts.default_width)
            })
            .collect()
    }

    /// `onResizeStart(colIndex, startX)`: the drag's fixed inputs. The
    /// host feeds each pointer move into [`ColumnResizeDrag::on_move`]
    /// and the release into [`ColumnResizeDrag::on_end`].
    pub fn begin_drag(
        &mut self,
        opts: &ColumnResizeOptions<'_>,
        col_index: usize,
        start_x: f32,
    ) -> ColumnResizeDrag {
        let widths = self.column_widths(opts);
        ColumnResizeDrag {
            col_index,
            name: opts
                .columns
                .and_then(|c| c.get(col_index))
                .map(|c| c.name.to_owned()),
            start_x,
            start_width: widths.get(col_index).copied().unwrap_or(opts.default_width),
            min_width: opts.min_width,
        }
    }
}

/// One drag in flight (React's `onMove`/`onUp` closures over
/// `colIndex`, `startX`, `startWidth` and the column's name).
pub struct ColumnResizeDrag {
    col_index: usize,
    name: Option<String>,
    start_x: f32,
    start_width: f32,
    min_width: f32,
}

impl ColumnResizeDrag {
    /// The `pointermove` handler: `max(minWidth, startWidth + delta)`,
    /// filed under the column's name when it has one, else by position
    /// (the drag still works, it just cannot survive a reorder).
    pub fn on_move(&self, state: &mut ColumnResizeState, client_x: f32) {
        let delta = client_x - self.start_x;
        let next = (self.start_width + delta).max(self.min_width);
        if let Some(name) = &self.name {
            state.resized_by_name.insert(name.clone(), next);
        } else {
            state.resized_by_index.insert(self.col_index, next);
        }
    }

    /// The `pointerup` handler: report the widths now in force, by name
    /// — the host files them under the table's identity (DB-12). `None`
    /// when there are no columns to key by (React never fires
    /// `onWidthsCommit` without names).
    pub fn on_end(
        &self,
        state: &ColumnResizeState,
        opts: &ColumnResizeOptions<'_>,
    ) -> Option<BTreeMap<String, f32>> {
        let columns = opts.columns?;
        let smart = state.smart.as_ref().map(|s| s.widths.as_slice());
        let mut effective = BTreeMap::new();
        for (i, col) in columns.iter().enumerate() {
            let width = state
                .resized_by_name
                .get(col.name)
                .copied()
                .or_else(|| opts.persisted_widths.and_then(|p| p.get(col.name).copied()))
                .or_else(|| smart.and_then(|s| s.get(i).copied()))
                .unwrap_or(opts.default_width);
            effective.insert(col.name.to_owned(), width);
        }
        Some(effective)
    }
}

/// `adjustWidthsForSort`: a single-column sort uses the same adaptive
/// control as the unsorted header (no change); a multi-column sort adds
/// the position badge width beside the control for each sorted column.
pub fn adjust_widths_for_sort(
    widths: &[f32],
    columns: &[ColumnWidthInput<'_>],
    sorts: &[ColumnSortSpec<'_>],
) -> Vec<f32> {
    if sorts.len() < 2 {
        return widths.to_vec();
    }
    widths
        .iter()
        .zip(columns)
        .map(|(w, col)| {
            if sorts.iter().any(|sort| sort.column == col.name) {
                w + SORT_BADGE_EXTRA_WIDTH
            } else {
                *w
            }
        })
        .collect()
}
