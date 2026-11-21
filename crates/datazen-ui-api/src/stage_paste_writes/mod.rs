mod identity;
mod types;
mod writes;

#[cfg(test)]
mod tests;

pub use identity::{
    build_row_identity, effective_pending_identity, find_pending_for_row,
    has_pending_identity_collision, row_identity_is_unique, row_identity_key,
};
pub use types::{
    CellPasteWrite, GridErrorCode, PasteCellProblem, StagePasteWritesOutcome, StagedTableState,
    STAGE_PASTE_WRITES_GAP,
};
pub use writes::stage_paste_writes_into;
