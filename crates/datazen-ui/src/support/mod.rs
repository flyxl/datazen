//! Pure support layer ported from React `packages/frontend/src/lib/`.
//!
//! The React folder is `lib/`, but a Rust crate cannot have both `src/lib.rs`
//! and `src/lib/`; the directory is therefore named `support`. Every module
//! here is a pure function or a value judgement — no Rune tree, no state, no
//! IPC — so the DataTable bodies (`CellRenderer`, `GridCell`, `DetailPanel`,
//! T004/T015/T009) can consume them without pulling in a view.
#![allow(dead_code)] // consumers: T004 CellRenderer, T015 GridCell, T009 DetailPanel

pub mod cell_editor_kind;
pub mod sql_dialects; // lib/sqlDialects/ (T035: dialectProfile + extra)
pub mod cell_types;
pub mod chart_export;
pub mod chart_field_inference;
pub mod chart_format;
// T117: the post-query table-vs-chart decision (lib/chart/postQueryView.ts).
pub mod chart_post_query_view;
// T151: chart type recommendation (lib/chart/recommend.ts).
pub mod chart_recommend;
// T151: series overlap dashes (lib/chart/seriesOverlap.ts).
pub mod chart_series_overlap;
pub mod data_type_colors;
pub mod formatters;
pub mod to_edit_string;
pub mod value_presentation; // lib/valuePresentation/ (T207: temporalMath first)
