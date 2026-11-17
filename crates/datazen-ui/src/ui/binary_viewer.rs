//! The binary viewer: hex + ASCII dump of a byte payload.
//!
//! This is a reusable base ported from the hex-rendering part of
//! `packages/ui/src/BinaryViewer.tsx` (`hexRows`). No `views/` caller uses it
//! yet; it exists so later tasks can compose it instead of re-implementing hex
//! display by hand.
//!
//! ## What it owns
//!
//! - A read-only hex + ASCII table: offset column (8 hex digits), hex bytes, and
//!   ASCII gutter, `row_bytes` bytes per line (default 16).
//! - Incomplete trailing rows are padded with spaces in the hex column but no
//!   placeholder bytes are invented; non-printable ASCII bytes render as `.`.
//! - Empty input renders no data lines (zero rows).
//!
//! Everything visual comes from [`Kit`] (`k.ty.mono`, `c.*`, `k.radius.*`); no
//! colour literals live here.
//!
//! ## What it leaves out
//!
//! - Image sniffing/preview and save-as-file actions (React-only concerns owned
//!   by callers/host integration), row-count truncation footer, and copy/export.
//!
//! ## Key convention
//!
//! | part      | key                    |
//! |-----------|------------------------|
//! | root      | `<key>`                |
//! | body      | `<key>-body`           |
//! | row       | `<key>-line-{i}`       |
//! | offset    | `<key>-offset-{i}`     |
//! | hex cell  | `<key>-hex-{i}-{j}`    |
//! | ascii     | `<key>-ascii-{i}`      |
// No `views/` caller uses this yet; it is a reusable base for later migration tasks.
#![allow(dead_code)]

use rune::{col, row, text, El};
use rune_kit::Kit;

/// Default bytes rendered per row.
pub(crate) const DEFAULT_ROW_BYTES: usize = 16;

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
            .key(format!("{}-body", self.key))
            .w_full()
            .gap(0.0)
            .scroll(rune::ScrollSpec { stick_bottom: false });

        if self.bytes.is_empty() {
            // Empty input: zero data rows (explicit empty state).
        } else {
            let row_bytes = self.row_bytes;
            let total_rows = (self.bytes.len() + row_bytes - 1) / row_bytes;
            for i in 0..total_rows {
                let start = i * row_bytes;
                let end = (start + row_bytes).min(self.bytes.len());
                let slice = &self.bytes[start..end];
                body = body.child(self.build_row(k, i, start, slice, row_bytes));
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
        let mut line = row()
            .key(format!("{}-line-{i}", self.key))
            .w_full()
            .items_start()
            .gap(12.0);

        let offset_text = format!("{offset:08x}");
        line = line.child(
            text(offset_text)
                .key(format!("{}-offset-{i}", self.key))
                .font(k.ty.mono)
                .color(k.c.text_muted),
        );

        let mut hex_row = row().gap(4.0);
        for j in 0..row_bytes {
            let cell = if j < slice.len() {
                format!("{:02x}", slice[j])
            } else {
                "  ".to_owned()
            };
            hex_row = hex_row.child(
                text(cell)
                    .key(format!("{}-hex-{i}-{j}", self.key))
                    .font(k.ty.mono)
                    .color(k.c.text_base),
            );
        }
        line = line.child(hex_row);

        let ascii: String = slice.iter().map(|&b| ascii_gutter(b)).collect();
        line = line.child(
            text(format!("|{ascii}|"))
                .key(format!("{}-ascii-{i}", self.key))
                .font(k.ty.mono)
                .color(k.c.text_muted),
        );

        line
    }
}

fn ascii_gutter(byte: u8) -> char {
    if (0x21..=0x7e).contains(&byte) {
        byte as char
    } else {
        '.'
    }
}
