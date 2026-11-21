use super::{common_flags, CommonEditorFlags, FocusStrategy, ScalarEditorInput};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEditorView {
    pub common: CommonEditorFlags,
    pub value: String,
    pub aria_label: String,
    pub focus_strategy: FocusStrategy,
    pub classes: Vec<&'static str>,
}

pub fn build_text_view(input: &ScalarEditorInput) -> TextEditorView {
    let mut classes = vec![
        "w-full",
        "min-w-0",
        "h-7",
        "bg-surface",
        "align-middle",
        "font-mono",
        "text-xs",
        "text-fg",
        "outline-none",
        "placeholder:text-fg-muted",
        "selection:bg-accent/30",
        "rounded-sm",
        "border",
        "border-accent",
        "shadow-none",
        "px-1.5",
    ];
    if input.invalid {
        classes.push("border-danger");
    }
    TextEditorView {
        common: common_flags(input, "table-edit-input", "input"),
        value: input.value.clone(),
        aria_label: input.aria_label.clone(),
        focus_strategy: FocusStrategy::SelectAll,
        classes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar_cell_editors::TemporalKindValue;

    fn input(invalid: bool) -> ScalarEditorInput {
        ScalarEditorInput {
            value: "hello".to_owned(),
            invalid,
            aria_label: "Name".to_owned(),
            auto_focus: true,
            temporal_kind: TemporalKindValue::Date,
        }
    }

    #[test]
    fn text_editor_selects_all_and_shares_the_numeric_test_id() {
        let view = build_text_view(&input(false));
        assert_eq!(view.common.test_id, "table-edit-input");
        assert_eq!(view.value, "hello");
        assert_eq!(view.focus_strategy, FocusStrategy::SelectAll);
        assert_eq!(view.common.handlers_target, "input");
        assert!(!view.classes.contains(&"text-right"));
    }

    #[test]
    fn invalid_adds_the_danger_border_class() {
        let view = build_text_view(&input(true));
        assert_eq!(view.common.aria_invalid, Some(true));
        assert!(view.classes.contains(&"border-danger"));
    }
}
