#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeMenuPredefined {
    Separator,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Undo,
    Redo,
}

pub fn predefined_edit_command(item: NativeMenuPredefined) -> Option<&'static str> {
    match item {
        NativeMenuPredefined::Separator => None,
        NativeMenuPredefined::Cut => Some("cut"),
        NativeMenuPredefined::Copy => Some("copy"),
        NativeMenuPredefined::Paste => Some("paste"),
        NativeMenuPredefined::SelectAll => Some("selectAll"),
        NativeMenuPredefined::Undo => Some("undo"),
        NativeMenuPredefined::Redo => Some("redo"),
    }
}

pub fn run_predefined_edit<F>(item: NativeMenuPredefined, exec: &F)
where
    F: Fn(&str),
{
    if let Some(command) = predefined_edit_command(item) {
        exec(command);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn commands_of(item: NativeMenuPredefined) -> Vec<String> {
        let captured = RefCell::new(Vec::new());
        run_predefined_edit(item, &|command: &str| {
            captured.borrow_mut().push(command.to_owned());
        });
        captured.into_inner()
    }

    #[test]
    fn command_mapping_is_pinned() {
        assert_eq!(
            predefined_edit_command(NativeMenuPredefined::Separator),
            None
        );
        assert_eq!(
            predefined_edit_command(NativeMenuPredefined::Cut),
            Some("cut")
        );
        assert_eq!(
            predefined_edit_command(NativeMenuPredefined::Copy),
            Some("copy")
        );
        assert_eq!(
            predefined_edit_command(NativeMenuPredefined::Paste),
            Some("paste")
        );
        assert_eq!(
            predefined_edit_command(NativeMenuPredefined::SelectAll),
            Some("selectAll")
        );
        assert_eq!(
            predefined_edit_command(NativeMenuPredefined::Undo),
            Some("undo")
        );
        assert_eq!(
            predefined_edit_command(NativeMenuPredefined::Redo),
            Some("redo")
        );
    }

    #[test]
    fn each_edit_item_dispatches_exactly_one_command() {
        let cases = [
            (NativeMenuPredefined::Cut, "cut"),
            (NativeMenuPredefined::Copy, "copy"),
            (NativeMenuPredefined::Paste, "paste"),
            (NativeMenuPredefined::SelectAll, "selectAll"),
            (NativeMenuPredefined::Undo, "undo"),
            (NativeMenuPredefined::Redo, "redo"),
        ];
        for (item, expected) in cases {
            assert_eq!(commands_of(item), [expected.to_owned()]);
        }
    }

    #[test]
    fn separator_is_a_noop() {
        assert!(commands_of(NativeMenuPredefined::Separator).is_empty());
    }
}
