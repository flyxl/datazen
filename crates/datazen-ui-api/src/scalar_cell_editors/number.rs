use super::{common_flags, CommonEditorFlags, FocusStrategy, ScalarEditorInput};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberEditorView {
    pub common: CommonEditorFlags,
    pub value: String,
    pub aria_label: String,
    pub input_mode: &'static str,
    pub text_align: &'static str,
    pub font_mono: bool,
    pub focus_strategy: FocusStrategy,
    pub classes: Vec<&'static str>,
}

pub fn build_number_view(input: &ScalarEditorInput) -> NumberEditorView {
    let mut classes = vec![
        "w-full",
        "min-w-0",
        "h-7",
        "bg-surface",
        "text-right",
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
    NumberEditorView {
        common: common_flags(input, "table-edit-input", "input"),
        value: input.value.clone(),
        aria_label: input.aria_label.clone(),
        input_mode: "decimal",
        text_align: "right",
        font_mono: true,
        focus_strategy: FocusStrategy::CaretToEnd,
        classes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar_cell_editors::TemporalKindValue;

    fn input(invalid: bool) -> ScalarEditorInput {
        ScalarEditorInput {
            value: "12.5".to_owned(),
            invalid,
            aria_label: "Amount".to_owned(),
            auto_focus: true,
            temporal_kind: TemporalKindValue::Date,
        }
    }

    #[test]
    fn numeric_input_is_text_with_decimal_mode_and_end_caret() {
        let view = build_number_view(&input(false));
        assert_eq!(view.common.test_id, "table-edit-input");
        assert_eq!(view.value, "12.5");
        assert_eq!(view.input_mode, "decimal");
        assert_eq!(view.text_align, "right");
        assert!(view.font_mono);
        assert_eq!(view.focus_strategy, FocusStrategy::CaretToEnd);
        assert_eq!(view.common.handlers_target, "input");
        assert!(!view.classes.contains(&"border-danger"));
    }

    #[test]
    fn invalid_adds_the_danger_border_class() {
        let view = build_number_view(&input(true));
        assert_eq!(view.common.aria_invalid, Some(true));
        assert!(view.classes.contains(&"border-danger"));
    }
}
