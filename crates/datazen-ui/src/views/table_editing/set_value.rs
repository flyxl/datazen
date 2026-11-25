//! The "Set Value" menu — `resolveSetValueState`, `applySetValueAction` and the
//! hook's dialog routing (DBR-05).
//!
//! The state is asked for at the moment of the click: it depends on the column
//! (nullable? binary? zoned?) and on the row (is there a staged change to
//! revert? were candidates loaded?). The two dialog entries are gated on the
//! **panel's** writability, not the column's in-place editor: `binaryColumn`
//! is exactly the reason the file loader exists, so it must not hide it.
//!
//! The label keys are the exact `lib/setValueActions.ts` keys; there are no
//! invented strings. The host owns the dialog and the staging port.

use crate::support::cell_types::{is_zoned_cell_type_with_hints, CellType};

use super::{
    build_panel_column_editor_info, resolve_panel_read_only_reason, EditingContextVm,
    EditingReadOnlyReason,
};

/// React `SetValueActionId`.
#[allow(dead_code)] // VM mirror; drop with the Set Value menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetValueActionId {
    SetNull,
    SetEmpty,
    SetDefault,
    RevertCell,
    PickDate,
    LoadBinaryFromFile,
    SetCurrentDatetime,
}

impl SetValueActionId {
    /// The exact wire id from `lib/setValueActions.ts`.
    #[allow(dead_code)] // pure helper; drop with the menu.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SetNull => "setNull",
            Self::SetEmpty => "setEmpty",
            Self::SetDefault => "setDefault",
            Self::RevertCell => "revertCell",
            Self::PickDate => "pickDate",
            Self::LoadBinaryFromFile => "loadBinaryFromFile",
            Self::SetCurrentDatetime => "setCurrentDatetime",
        }
    }

    /// `ACTION_LABEL_KEYS` — exact, no invented keys.
    #[allow(dead_code)] // pure helper; drop with the menu.
    pub fn label_key(self) -> &'static str {
        match self {
            Self::SetNull => "dataTable.setNull",
            Self::SetEmpty => "cellEditor.setEmpty",
            Self::SetDefault => "cellEditor.setDefault",
            Self::RevertCell => "cellEditor.revertCell",
            Self::PickDate => "dataTable.pickDate",
            Self::LoadBinaryFromFile => "dataTable.loadBinaryFromFile",
            Self::SetCurrentDatetime => "dataTable.setCurrentDatetime",
        }
    }
}

/// The row facts the store contributes to `resolveSetValueState`.
#[allow(dead_code)] // VM mirror; drop when the host owns pending changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetValueRowFactsVm {
    /// `pendingChanges.get(anchor)?.changedColumns.includes(column)`
    pub has_pending: bool,
    /// `enumCandidates.get(column)` — a loaded list hides the load entry.
    pub candidates_loaded: bool,
}

/// React `SetValueState` (the panel hook always reports `rowKind: 'update'`).
#[allow(dead_code)] // VM mirror; drop with the Set Value menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetValueStateVm {
    pub can_null: bool,
    pub can_empty: bool,
    pub row_is_insert: bool,
    pub has_pending: bool,
    pub can_load_candidates: bool,
    pub can_set_current_datetime: bool,
    pub can_pick_date: bool,
    pub can_load_binary: bool,
}

/// React `resolveSetValueState`: `None` when the panel or column is unknown.
#[allow(dead_code)] // pure decision; drop when the host owns the menu.
pub fn resolve_set_value_state(
    context: &EditingContextVm,
    column_name: &str,
    facts: &SetValueRowFactsVm,
) -> Option<SetValueStateVm> {
    let column = context
        .columns
        .iter()
        .find(|entry| entry.name == column_name)?;
    let info = build_panel_column_editor_info(context).get(column_name).copied();
    let panel_writable = resolve_panel_read_only_reason(context).is_none();
    let column_reason = info.and_then(|info| info.read_only_reason);
    let cell_type = info.map(|info| info.cell_type);
    let data_type = column.data_type.as_deref().unwrap_or("");

    Some(SetValueStateVm {
        can_null: column.nullable,
        can_empty: true,
        row_is_insert: false,
        has_pending: facts.has_pending,
        can_load_candidates: cell_type == Some(CellType::Enum) && !facts.candidates_loaded,
        can_set_current_datetime: context.current_datetime_write == Some(true)
            && cell_type == Some(CellType::DateTime)
            && panel_writable,
        can_pick_date: panel_writable
            && matches!(cell_type, Some(CellType::Date) | Some(CellType::DateTime))
            && !is_zoned_cell_type_with_hints(data_type, context.hints.as_ref()),
        // `binaryColumn` is the in-place editor's refusal, i.e. the reason the
        // loader exists; every other column reason still hides the entry.
        can_load_binary: panel_writable
            && cell_type == Some(CellType::Bytes)
            && (column_reason.is_none()
                || column_reason == Some(EditingReadOnlyReason::BinaryColumn))
            && !column.is_auto_increment,
    })
}

/// The hook's `setValueState`: a panel without a dialog host must not offer
/// entries nobody can finish.
#[allow(dead_code)] // pure gate; drop when the host owns the menu.
pub fn with_dialog_host_gate(
    state: SetValueStateVm,
    has_dialog_host: bool,
) -> SetValueStateVm {
    if has_dialog_host {
        state
    } else {
        SetValueStateVm {
            can_pick_date: false,
            can_load_binary: false,
            ..state
        }
    }
}

/// The write intents of `applySetValueAction`. `Absent` is React's
/// `undefined` — leave the column out of the write; it is shared by
/// `setDefault` and `revertCell`.
#[allow(dead_code)] // host intent; drop when the host owns staging.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StagedIntentVm {
    Null,
    Empty,
    Absent,
    CurrentDatetimeExpression,
}

/// The two entries that open a dialog instead of staging a value.
#[allow(dead_code)] // host intent; drop with the dialog host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetValueDialogVm {
    PickDate,
    LoadBinaryFromFile,
}

impl SetValueDialogVm {
    #[allow(dead_code)] // exact React action id; drop with the dialog host.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PickDate => "pickDate",
            Self::LoadBinaryFromFile => "loadBinaryFromFile",
        }
    }
}

/// The hook's `onSetValue` routing, reduced to a pure plan.
#[allow(dead_code)] // decision; drop when the host owns the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetValueRouteVm {
    OpenDialog(SetValueDialogVm),
    Stage(StagedIntentVm),
    /// A dialog entry with no host falls through to `applySetValueAction`,
    /// whose switch has no case for it: nothing is staged.
    NoOp,
}

/// React `onSetValue` + `applySetValueAction` in one decision.
#[allow(dead_code)] // pure decision; drop when the host owns staging.
pub fn route_set_value(action: SetValueActionId, has_dialog_host: bool) -> SetValueRouteVm {
    if has_dialog_host {
        match action {
            SetValueActionId::PickDate => {
                return SetValueRouteVm::OpenDialog(SetValueDialogVm::PickDate)
            }
            SetValueActionId::LoadBinaryFromFile => {
                return SetValueRouteVm::OpenDialog(SetValueDialogVm::LoadBinaryFromFile)
            }
            _ => {}
        }
    }
    match action {
        SetValueActionId::SetNull => SetValueRouteVm::Stage(StagedIntentVm::Null),
        SetValueActionId::SetEmpty => SetValueRouteVm::Stage(StagedIntentVm::Empty),
        SetValueActionId::SetDefault | SetValueActionId::RevertCell => {
            SetValueRouteVm::Stage(StagedIntentVm::Absent)
        }
        SetValueActionId::SetCurrentDatetime => {
            SetValueRouteVm::Stage(StagedIntentVm::CurrentDatetimeExpression)
        }
        SetValueActionId::PickDate | SetValueActionId::LoadBinaryFromFile => SetValueRouteVm::NoOp,
    }
}

/// React `SetValueAction` (id + the menu's label key).
#[allow(dead_code)] // VM mirror; drop with the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetValueActionVm {
    pub id: SetValueActionId,
    pub label_key: &'static str,
}

/// React `resolveSetValueActions`: display order is part of the contract.
/// The dialog entries come after the value intents, and the typed expression
/// reads as an answer rather than a tool.
#[allow(dead_code)] // pure derivation; drop with the menu.
pub fn resolve_set_value_actions(state: &SetValueStateVm) -> Vec<SetValueActionVm> {
    let mut actions = Vec::new();
    let mut push = |id: SetValueActionId| actions.push(SetValueActionVm {
        id,
        label_key: id.label_key(),
    });
    if state.can_null {
        push(SetValueActionId::SetNull);
    }
    if state.can_empty {
        push(SetValueActionId::SetEmpty);
    }
    if state.row_is_insert {
        push(SetValueActionId::SetDefault);
    }
    if !state.row_is_insert && state.has_pending {
        push(SetValueActionId::RevertCell);
    }
    if state.can_pick_date {
        push(SetValueActionId::PickDate);
    }
    if state.can_load_binary {
        push(SetValueActionId::LoadBinaryFromFile);
    }
    if state.can_set_current_datetime {
        push(SetValueActionId::SetCurrentDatetime);
    }
    actions
}
