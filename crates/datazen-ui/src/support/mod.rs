//! Pure support layer ported from React `packages/frontend/src/lib/`.
//!
//! The React folder is `lib/`, but a Rust crate cannot have both `src/lib.rs`
//! and `src/lib/`; the directory is therefore named `support`. Every module
//! here is a pure function or a value judgement — no Rune tree, no state, no
//! IPC — so the DataTable bodies (`CellRenderer`, `GridCell`, `DetailPanel`,
//! T004/T015/T009) can consume them without pulling in a view.
#![allow(dead_code)] // consumers: T004 CellRenderer, T015 GridCell, T009 DetailPanel

pub(crate) mod cell_editor_kind;
pub(crate) mod cell_types;
pub(crate) mod data_type_colors;
pub(crate) mod formatters;
pub(crate) mod to_edit_string;
