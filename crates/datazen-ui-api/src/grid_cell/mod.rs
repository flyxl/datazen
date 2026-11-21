mod events;
mod types;
mod view;

#[cfg(test)]
mod tests;

pub use events::{cell_command, dispatch_cell_event, CellCommand, CellEvent, DispatchOutcome};
pub use types::{
    CellChild, CellRendererView, CellWriteTarget, ColumnCellFacts, EditorCellFacts, FkButtonView,
    FkCellAffordance, FkTextKind, GridCellInput, GridCellView, StickyInfo, StickyStyle, CELL_TEST_ID, CLASS_BASE,
    CLASS_DIRTY_BORDER, CLASS_DIRTY_BG, CLASS_DIRTY_MARK, CLASS_FK_JUMP, CLASS_FK_PICK,
    CLASS_STICKY_OVERLAY, CLASS_STICKY, EDITOR_KIND_ENUM, ROLE_GRIDCELL,
};
pub use view::build_grid_cell_view;

pub type FkTextFn<'a> = dyn Fn(FkTextKind, &str) -> String + 'a;
