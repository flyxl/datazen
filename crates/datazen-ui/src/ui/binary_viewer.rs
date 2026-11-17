//! The binary viewer: hex + ASCII dump of a byte payload.
//!
//! This is a reusable base ported from the hex-rendering part of
//! `packages/ui/src/BinaryViewer.tsx` (`hexRows`). No `views/` caller uses it
//! yet; it exists so later tasks can compose it instead of re-implementing hex
//! display by hand.
//!
//! ## What it owns
//!
//! - A read-only hex + ASCII table: offset column (8 hex digits), whole-row hex
//!   bytes text, and ASCII gutter, `row_bytes` bytes per line (default 16).
//! - Hex row shape matches React `hexRows`: each present byte is `"xx "`, each
//!   missing slot in the final (incomplete) row is `"   "`, and the row hex string
//!   is `trimEnd()`ed. ASCII is the bare string of existing bytes only (no `|...|`
//!   wrapper), with non-printable bytes rendered as `.`.
//! - Body is clamped to `max_h(360.0)` and scrolls internally. Rendering is capped
//!   at `MAX_HEX_ROWS` (4096) rows; when truncated a `{key}-hex-truncated` marker is
//!   shown (4 KB / 256 rows is not truncated).
//! - Empty input renders no data lines (zero rows).
//!
//! Everything visual comes from [`Kit`] (`k.ty.mono`, `c.*`, `k.radius.*`); no
//! colour literals live here.
//!
//! ## What it leaves out
//!
//! - Image sniffing/preview and save-as-file actions (React-only concerns owned
//!   by callers/host integration), and copy/export.
//!
//! ## Key convention and React mapping
//!
//! Caller passes a prefix key (e.g. `bv`).
//!
//! | React testid/suffix      | Rune key                | Notes |
//! |--------------------------|-------------------------|-------|
//! | `<key>`                  | `<key>`                 | Equivalent root container |
//! | `<key>-hex`              | `<key>-hex`             | Equivalent hex/body container (scrollable, clamped) |
//! | *(row, not keyed in React)* | `<key>-line-{i}`    | Rune-specific row container |
//! | *(offset text)*          | `<key>-offset-{i}`      | 8-digit hex offset |
//! | `<key>-hex-bytes`        | `<key>-hex-bytes-{i}`   | Whole-row hex string (React uses one testid for all rows; Rune keys per row for assertions) |
//! | *(ascii text)*           | `<key>-ascii-{i}`       | Bare ASCII gutter string (no `|` wrapper), existing bytes only |
//! | `<key>-hex-truncated`    | `<key>-hex-truncated`   | Truncation notice when `rows > MAX_HEX_ROWS` |
//!
//! Per-byte hex cell keys (`-hex-{i}-{j}`) are intentionally removed: hex is rendered
//! as a single row string for React parity.
// No `views/` caller uses this yet; it is a reusable base for later migration tasks.
#![allow(dead_code)]

use rune::{col, row, text, El};
use rune_kit::Kit;

/// Default bytes rendered per row.
pub(crate) const DEFAULT_ROW_BYTES: usize = 16;
/// Max rows rendered for hex views (matches React), to keep large blobs bounded.
pub(crate) const MAX_HEX_ROWS: usize = 4096;

/// Hex + ASCII viewer for binary data.
pub(crate) struct BinaryViewer {
    key: String,
    bytes: Vec<u8>,
    row_bytes: usize,
    grow: bool,
    height: Option<f32>,
}

impl BinaryViewer {
    pub(crate) fn new(key: impl Into<String>, bytes: &[u8]) -> Self {
        Self {
            key: key.into(),
            bytes: bytes.to_vec(),
            row_bytes: DEFAULT_ROW_BYTES,
            grow: false,
            height: None,
        }
    }

    pub(crate) fn row_bytes(mut self, n: usize) -> Self {
        self.row_bytes = n.max(1);
        self
    }

    pub(crate) fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    pub(crate) fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let mut body = col()
            .key(format!("{}-hex", self.key))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .max_h(360.0)
            .gap(0.0)
            .scroll(rune::ScrollSpec { stick_bottom: false });

        if !self.bytes.is_empty() {
            let row_bytes = self.row_bytes;
            let total_rows = (self.bytes.len() + row_bytes - 1) / row_bytes;
            let shown_rows = total_rows.min(MAX_HEX_ROWS);
            for i in 0..shown_rows {
                let start = i * row_bytes;
                let end = (start + row_bytes).min(self.bytes.len());
                let slice = &self.bytes[start..end];
                body = body.child(self.build_row(k, i, start, slice, row_bytes));
            }
            if total_rows > shown_rows {
                body = body.child(
                    text(format!("… truncated: showing {shown_rows} of {total_rows} rows"))
                        .key(format!("{}-hex-truncated", self.key))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                );
            }
        }

        let mut root = col()
            .key(self.key.clone())
            .w_full()
            .fill(k.c.layer_01)
            .border(1.0, k.c.border_base)
            .radius(k.radius.md)
            .padding(8.0)
            .child(body);
        if self.grow {
            root = root.grow(1.0).min_h(0.0);
        }
        if let Some(h) = self.height {
            root = root.h(h);
        }
        root
    }

    fn build_row(&self, k: &Kit, i: usize, offset: usize, slice: &[u8], row_bytes: usize) -> El {
        let offset_text = format!("{offset:08x}");
        let mut hex = String::new();
        let mut ascii = String::with_capacity(slice.len());
        for j in 0..row_bytes {
            if j < slice.len() {
                hex.push_str(&format!("{:02x} ", slice[j]));
                ascii.push(ascii_gutter(slice[j]));
            } else {
                hex.push_str("   ");
            }
        }
        let hex = hex.trim_end().to_owned();

        row()
            .key(format!("{}-line-{i}", self.key))
            .w_full()
            .items_start()
            .gap(12.0)
            .child(
                text(offset_text)
                    .key(format!("{}-offset-{i}", self.key))
                    .font(k.ty.mono)
                    .color(k.c.text_muted),
            )
            .child(
                text(hex)
                    .key(format!("{}-hex-bytes-{i}", self.key))
                    .font(k.ty.mono)
                    .color(k.c.text_base),
            )
            .child(
                text(ascii)
                    .key(format!("{}-ascii-{i}", self.key))
                    .font(k.ty.mono)
                    .color(k.c.text_muted),
            )
    }
}

fn ascii_gutter(byte: u8) -> char {
    if (0x21..=0x7e).contains(&byte) {
        byte as char
    } else {
        '.'
    }
}
