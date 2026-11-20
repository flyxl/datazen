#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionEditorState {
    pub open: bool,
    pub edit_id: Option<String>,
    pub default_group: Option<String>,
    pub open_seq: u64,
}

impl Default for ConnectionEditorState {
    fn default() -> Self {
        Self {
            open: false,
            edit_id: None,
            default_group: None,
            open_seq: 0,
        }
    }
}

impl ConnectionEditorState {
    pub fn open_new_connection_dialog(&mut self, edit_id: Option<&str>, default_group: Option<&str>) {
        let editing = matches!(edit_id, Some(id) if !id.is_empty());
        self.open = true;
        self.edit_id = edit_id.map(str::to_owned);
        self.default_group = if editing { None } else { default_group.map(str::to_owned) };
        self.open_seq = self.open_seq.wrapping_add(1);
    }

    pub fn close_new_connection_dialog(&mut self) {
        self.open = false;
        self.edit_id = None;
        self.default_group = None;
    }
}

pub fn open_new_connection_dialog(
    state: &mut ConnectionEditorState,
    edit_id: Option<&str>,
    default_group: Option<&str>,
) {
    state.open_new_connection_dialog(edit_id, default_group);
}

pub fn close_new_connection_dialog(state: &mut ConnectionEditorState) {
    state.close_new_connection_dialog();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ConnectionEditorState {
        ConnectionEditorState::default()
    }

    #[test]
    fn the_initial_state_is_closed_and_empty() {
        let initial = state();
        assert!(!initial.open);
        assert_eq!(initial.edit_id, None);
        assert_eq!(initial.default_group, None);
        assert_eq!(initial.open_seq, 0);
    }

    #[test]
    fn opening_without_arguments_sets_only_open_and_bumps_the_sequence() {
        let mut current = state();
        current.open_new_connection_dialog(None, None);
        assert!(current.open);
        assert_eq!(current.edit_id, None);
        assert_eq!(current.default_group, None);
        assert_eq!(current.open_seq, 1);
    }

    #[test]
    fn creating_from_a_group_preselects_the_group() {
        let mut current = state();
        current.open_new_connection_dialog(None, Some("analytics"));
        assert!(current.open);
        assert_eq!(current.edit_id, None);
        assert_eq!(current.default_group.as_deref(), Some("analytics"));
        assert_eq!(current.open_seq, 1);
    }

    #[test]
    fn editing_discards_the_default_group_even_when_one_is_supplied() {
        let mut current = state();
        current.open_new_connection_dialog(Some("conn-7"), Some("analytics"));
        assert!(current.open);
        assert_eq!(current.edit_id.as_deref(), Some("conn-7"));
        assert_eq!(current.default_group, None);
        assert_eq!(current.open_seq, 1);
    }

    #[test]
    fn an_empty_edit_id_is_falsy_and_keeps_the_group() {
        let mut current = state();
        current.open_new_connection_dialog(Some(""), Some("analytics"));
        assert!(current.open);
        assert_eq!(current.edit_id.as_deref(), Some(""));
        assert_eq!(current.default_group.as_deref(), Some("analytics"));
    }

    #[test]
    fn an_empty_default_group_is_kept_when_creating() {
        let mut current = state();
        current.open_new_connection_dialog(None, Some(""));
        assert_eq!(current.default_group.as_deref(), Some(""));
    }

    #[test]
    fn repeated_opens_increment_the_sequence_without_resetting_other_fields() {
        let mut current = state();
        current.open_new_connection_dialog(Some("conn-1"), None);
        current.open_new_connection_dialog(Some("conn-2"), None);
        current.open_new_connection_dialog(None, Some("g"));
        assert!(current.open);
        assert_eq!(current.edit_id, None);
        assert_eq!(current.default_group.as_deref(), Some("g"));
        assert_eq!(current.open_seq, 3);
    }

    #[test]
    fn close_clears_the_dialog_fields_but_not_the_sequence() {
        let mut current = state();
        current.open_new_connection_dialog(Some("conn-1"), Some("g"));
        current.close_new_connection_dialog();
        assert!(!current.open);
        assert_eq!(current.edit_id, None);
        assert_eq!(current.default_group, None);
        assert_eq!(current.open_seq, 1);
    }

    #[test]
    fn closing_before_any_open_leaves_the_sequence_at_zero() {
        let mut current = state();
        current.close_new_connection_dialog();
        assert_eq!(current.open_seq, 0);
        assert!(!current.open);
    }

    #[test]
    fn reopening_after_close_continues_the_sequence() {
        let mut current = state();
        current.open_new_connection_dialog(None, None);
        current.close_new_connection_dialog();
        current.open_new_connection_dialog(Some("conn-9"), None);
        assert!(current.open);
        assert_eq!(current.edit_id.as_deref(), Some("conn-9"));
        assert_eq!(current.default_group, None);
        assert_eq!(current.open_seq, 2);
    }

    #[test]
    fn the_free_functions_drive_the_same_state_transitions() {
        let mut current = state();
        open_new_connection_dialog(&mut current, None, Some("ops"));
        assert_eq!(
            current,
            ConnectionEditorState {
                open: true,
                edit_id: None,
                default_group: Some("ops".to_owned()),
                open_seq: 1,
            }
        );
        close_new_connection_dialog(&mut current);
        assert_eq!(
            current,
            ConnectionEditorState {
                open: false,
                edit_id: None,
                default_group: None,
                open_seq: 1,
            }
        );
    }
}
