use super::{FocusStrategy, ScalarEditorInput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalKindValue {
    Date,
    Time,
    DateTime,
}

impl TemporalKindValue {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Time => "time",
            Self::DateTime => "datetime",
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Date => "YYYY-MM-DD",
            Self::Time => "HH:MM:SS",
            Self::DateTime => "YYYY-MM-DDTHH:MM:SS",
        }
    }
}

pub const DEFAULT_TEMPORAL_KIND: TemporalKindValue = TemporalKindValue::Date;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporalEditorView {
    pub test_id: &'static str,
    pub wrapper_classes: Vec<&'static str>,
    pub kind: TemporalKindValue,
    pub value: String,
    pub invalid: bool,
    pub aria_label: String,
    pub auto_focus: bool,
    pub focus_strategy: FocusStrategy,
    pub handlers_target: &'static str,
    pub sr_only_label: bool,
}

pub fn build_temporal_view(input: &ScalarEditorInput) -> TemporalEditorView {
    TemporalEditorView {
        test_id: "cell-editor-temporal",
        wrapper_classes: vec!["flex", "h-7", "w-full", "items-center"],
        kind: input.temporal_kind,
        value: input.value.clone(),
        invalid: input.invalid,
        aria_label: input.aria_label.clone(),
        auto_focus: input.auto_focus,
        focus_strategy: FocusStrategy::FirstDescendantInput,
        handlers_target: "wrapper",
        sr_only_label: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: TemporalKindValue) -> ScalarEditorInput {
        ScalarEditorInput {
            value: "2026-01-02".to_owned(),
            invalid: false,
            aria_label: "Date".to_owned(),
            auto_focus: true,
            temporal_kind: kind,
        }
    }

    #[test]
    fn temporal_kinds_pin_wire_names_and_canonical_placeholders() {
        assert_eq!(DEFAULT_TEMPORAL_KIND, TemporalKindValue::Date);
        assert_eq!(TemporalKindValue::Date.as_str(), "date");
        assert_eq!(TemporalKindValue::Time.as_str(), "time");
        assert_eq!(TemporalKindValue::DateTime.as_str(), "datetime");
        assert_eq!(TemporalKindValue::Date.placeholder(), "YYYY-MM-DD");
        assert_eq!(TemporalKindValue::Time.placeholder(), "HH:MM:SS");
        assert_eq!(
            TemporalKindValue::DateTime.placeholder(),
            "YYYY-MM-DDTHH:MM:SS"
        );
    }

    #[test]
    fn temporal_editor_focuses_the_first_input_and_keeps_handlers_on_the_wrapper() {
        let view = build_temporal_view(&input(TemporalKindValue::Date));
        assert_eq!(view.test_id, "cell-editor-temporal");
        assert_eq!(
            view.wrapper_classes,
            ["flex", "h-7", "w-full", "items-center"]
        );
        assert_eq!(view.kind, TemporalKindValue::Date);
        assert_eq!(view.value, "2026-01-02");
        assert!(view.auto_focus);
        assert_eq!(view.focus_strategy, FocusStrategy::FirstDescendantInput);
        assert_eq!(view.handlers_target, "wrapper");
        assert!(view.sr_only_label);
    }

    #[test]
    fn invalid_and_non_default_kinds_are_preserved() {
        let mut item = input(TemporalKindValue::DateTime);
        item.invalid = true;
        let view = build_temporal_view(&item);
        assert!(view.invalid);
        assert_eq!(view.kind, TemporalKindValue::DateTime);
    }
}
