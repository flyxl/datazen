mod number;
mod temporal;
mod text;

pub use number::{build_number_view, NumberEditorView};
pub use temporal::{
    build_temporal_view, TemporalEditorView, TemporalKindValue, DEFAULT_TEMPORAL_KIND,
};
pub use text::{build_text_view, TextEditorView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusStrategy {
    CaretToEnd,
    SelectAll,
    FirstDescendantInput,
}

impl FocusStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CaretToEnd => "caret-to-end",
            Self::SelectAll => "select-all",
            Self::FirstDescendantInput => "first-descendant-input",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalarEditorInput {
    pub value: String,
    pub invalid: bool,
    pub aria_label: String,
    pub auto_focus: bool,
    pub temporal_kind: TemporalKindValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommonEditorFlags {
    pub test_id: &'static str,
    pub auto_focus: bool,
    pub aria_invalid: Option<bool>,
    pub spell_check: bool,
    pub auto_capitalize: &'static str,
    pub auto_correct: &'static str,
    pub handlers_target: &'static str,
}

pub fn common_flags(
    input: &ScalarEditorInput,
    test_id: &'static str,
    handlers_target: &'static str,
) -> CommonEditorFlags {
    CommonEditorFlags {
        test_id,
        auto_focus: input.auto_focus,
        aria_invalid: if input.invalid { Some(true) } else { None },
        spell_check: false,
        auto_capitalize: "off",
        auto_correct: "off",
        handlers_target,
    }
}

pub const SCALAR_CELL_EDITORS_SURFACE: [&str; 6] = [
    "NumberCellEditor",
    "TemporalCellEditor",
    "TextCellEditor",
    "build_number_view",
    "build_temporal_view",
    "build_text_view",
];

pub const SCALAR_CELL_EDITORS_GAP: [&str; 6] = [
    "React input/div DOM elements and refs",
    "useEffect focus / setSelectionRange / select / querySelector",
    "TemporalValueInput from @datazen/ui",
    "cn class merging and Tailwind class literals",
    "KeyboardEvent, blur and composition events",
    "onDraftChange host callback",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_strategy_literals_are_pinned() {
        assert_eq!(FocusStrategy::CaretToEnd.as_str(), "caret-to-end");
        assert_eq!(FocusStrategy::SelectAll.as_str(), "select-all");
        assert_eq!(
            FocusStrategy::FirstDescendantInput.as_str(),
            "first-descendant-input"
        );
    }

    #[test]
    fn surfaces_and_gaps_are_pinned_and_root_reachable() {
        assert_eq!(SCALAR_CELL_EDITORS_SURFACE.len(), 6);
        assert!(SCALAR_CELL_EDITORS_SURFACE.contains(&"TemporalCellEditor"));
        assert_eq!(SCALAR_CELL_EDITORS_GAP.len(), 6);
        assert!(SCALAR_CELL_EDITORS_GAP.contains(&"TemporalValueInput from @datazen/ui"));
        assert_eq!(crate::SCALAR_CELL_EDITORS_SURFACE.len(), 6);
        assert_eq!(crate::SCALAR_CELL_EDITORS_GAP.len(), 6);
    }

    #[test]
    fn common_flags_fold_invalid_and_fixed_input_attributes() {
        let input = ScalarEditorInput {
            value: "1".to_owned(),
            invalid: true,
            aria_label: "n".to_owned(),
            auto_focus: true,
            temporal_kind: TemporalKindValue::Date,
        };
        let flags = common_flags(&input, "table-edit-input", "input");
        assert_eq!(flags.test_id, "table-edit-input");
        assert!(flags.auto_focus);
        assert_eq!(flags.aria_invalid, Some(true));
        assert!(!flags.spell_check);
        assert_eq!(flags.auto_capitalize, "off");
        assert_eq!(flags.auto_correct, "off");
        assert_eq!(flags.handlers_target, "input");
    }
}
