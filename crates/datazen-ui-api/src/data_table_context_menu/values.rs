use super::types::{
    DataTableContextMenuLabels, RowKind, SetValueAction, SetValueActionId, SetValueState,
};

pub const LOAD_CANDIDATES_LABEL_KEY: &str = "cellEditor.loadCandidates";

fn label_key(id: SetValueActionId) -> &'static str {
    match id {
        SetValueActionId::SetNull => "dataTable.setNull",
        SetValueActionId::SetEmpty => "cellEditor.setEmpty",
        SetValueActionId::SetDefault => "cellEditor.setDefault",
        SetValueActionId::RevertCell => "cellEditor.revertCell",
        SetValueActionId::PickDate => "dataTable.pickDate",
        SetValueActionId::LoadBinaryFromFile => "dataTable.loadBinaryFromFile",
        SetValueActionId::SetCurrentDatetime => "dataTable.setCurrentDatetime",
    }
}

pub fn resolve_set_value_actions(state: &SetValueState) -> Vec<SetValueAction> {
    let mut actions = Vec::new();
    let mut push = |id: SetValueActionId| {
        actions.push(SetValueAction {
            id,
            label_key: label_key(id).to_owned(),
        });
    };
    if state.can_null {
        push(SetValueActionId::SetNull);
    }
    if state.can_empty {
        push(SetValueActionId::SetEmpty);
    }
    if state.row_kind == RowKind::Insert {
        push(SetValueActionId::SetDefault);
    }
    if state.row_kind == RowKind::Update && state.has_pending {
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

pub fn set_value_action_label(id: SetValueActionId, labels: &DataTableContextMenuLabels) -> String {
    match id {
        SetValueActionId::SetNull => labels.set_null.clone(),
        SetValueActionId::SetEmpty => labels.set_empty.clone().unwrap_or_default(),
        SetValueActionId::SetDefault => labels.set_default.clone().unwrap_or_default(),
        SetValueActionId::RevertCell => labels.revert_cell.clone().unwrap_or_default(),
        SetValueActionId::PickDate => labels.pick_date.clone().unwrap_or_default(),
        SetValueActionId::LoadBinaryFromFile => {
            labels.load_binary_from_file.clone().unwrap_or_default()
        }
        SetValueActionId::SetCurrentDatetime => {
            labels.set_current_datetime.clone().unwrap_or_default()
        }
    }
}
