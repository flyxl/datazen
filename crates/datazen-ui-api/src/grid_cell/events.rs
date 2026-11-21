use super::types::{CellWriteTarget, GridCellInput, GridCellView};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum CellEvent {
    DoubleClick,
    MouseDown { button: u8 },
    FkActivateClick,
    FkPickClick,
    FkChildDoubleClick,
    FkChildMouseDown,
    RendererCommit { value: Option<Value> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum CellCommand {
    DoubleClick {
        row_index: usize,
        column_name: String,
    },
    QuickLook {
        row_index: usize,
        column_name: String,
    },
    FkActivate {
        row_index: usize,
        column_name: String,
    },
    Commit {
        row_index: usize,
        column_name: String,
        value: Option<Value>,
        target: CellWriteTarget,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchOutcome {
    pub command_emitted: bool,
    pub stop_propagation: bool,
    pub prevent_default: bool,
}

pub fn dispatch_cell_event(
    event: CellEvent,
    input: &GridCellInput,
    view: &GridCellView,
) -> DispatchOutcome {
    let mut outcome = DispatchOutcome {
        command_emitted: false,
        stop_propagation: false,
        prevent_default: false,
    };
    match event {
        CellEvent::DoubleClick => {
            outcome.stop_propagation = true;
            outcome.command_emitted = true;
        }
        CellEvent::MouseDown { button } => {
            if button == 1 && input.has_quick_look {
                outcome.prevent_default = true;
                outcome.command_emitted = true;
            }
        }
        CellEvent::FkActivateClick => {
            let navigable = input
                .fk
                .as_ref()
                .map(|fk| fk.navigable && !fk.resolving)
                .unwrap_or(false);
            if navigable && !input.editing && input.has_fk_activate {
                outcome.stop_propagation = true;
                outcome.command_emitted = true;
            }
        }
        CellEvent::FkPickClick => {
            let pickable = input
                .fk
                .as_ref()
                .and_then(|fk| fk.pickable)
                .unwrap_or(false);
            if pickable && !input.editing {
                outcome.stop_propagation = true;
                outcome.command_emitted = true;
            }
        }
        CellEvent::FkChildDoubleClick => {
            outcome.stop_propagation = true;
        }
        CellEvent::FkChildMouseDown => {
            outcome.prevent_default = true;
        }
        CellEvent::RendererCommit { .. } => {
            outcome.command_emitted = true;
        }
    }
    let _ = view;
    outcome
}

pub fn cell_command(
    event: CellEvent,
    input: &GridCellInput,
    view: &GridCellView,
) -> Option<CellCommand> {
    let outcome = dispatch_cell_event(event.clone(), input, view);
    if !outcome.command_emitted {
        return None;
    }
    match event {
        CellEvent::DoubleClick | CellEvent::FkPickClick => Some(CellCommand::DoubleClick {
            row_index: input.row_index,
            column_name: input.column_name.clone(),
        }),
        CellEvent::MouseDown { .. } => Some(CellCommand::QuickLook {
            row_index: input.row_index,
            column_name: input.column_name.clone(),
        }),
        CellEvent::FkActivateClick => Some(CellCommand::FkActivate {
            row_index: input.row_index,
            column_name: input.column_name.clone(),
        }),
        CellEvent::RendererCommit { value } => Some(CellCommand::Commit {
            row_index: input.row_index,
            column_name: input.column_name.clone(),
            value,
            target: view.write_target,
        }),
        CellEvent::FkChildDoubleClick | CellEvent::FkChildMouseDown => None,
    }
}
