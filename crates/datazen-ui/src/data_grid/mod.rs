//! DataGrid proof of concept.
//!
//! Rows come from `virtual_rows`: the list is long, and only the mounted window is built.
//! Columns are not virtualized by Rune, so each mounted row builds a window of cells plus the
//! frozen first column. The window follows the horizontal scroll offset.
//!
//! Cell text, the selection rectangle and the editor live on the grid, not on the row element.
//! A row that scrolls away is destroyed. The values are not.

mod input;
mod model;
mod view;

#[cfg(test)]
mod tests;

pub use input::on_key;
pub use model::{DataGrid, COL_COUNT, COL_W, COL_WINDOW, GUTTER_W, ROW_COUNT, ROW_H};
pub use view::{panel, result_panel, GridSlot};

pub(crate) use view::grid_of;
