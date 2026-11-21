use super::{KeyDecision, LoadButtonView, SelectOption};

#[derive(Clone, Debug, PartialEq)]
pub struct EnumEditorInput {
    pub draft: String,
    pub candidates: Option<Vec<String>>,
    pub has_load: bool,
    pub loading: bool,
    pub failed: bool,
    pub message: Option<String>,
    pub load_label: String,
    pub loading_label: String,
    pub failure_label: String,
    pub has_pick: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EnumEditorMode {
    Prompt {
        draft: String,
        input_test_id: &'static str,
        load_button: Option<LoadButtonView>,
        failure: Option<String>,
    },
    Select {
        options: Vec<SelectOption>,
        selected: String,
        test_id: &'static str,
        searchable: bool,
        pick_enabled: bool,
    },
}

pub fn build_enum_view(input: &EnumEditorInput) -> EnumEditorMode {
    match input
        .candidates
        .as_ref()
        .filter(|values| !values.is_empty())
    {
        None => {
            let load_button = if input.has_load {
                Some(LoadButtonView {
                    label: if input.loading {
                        input.loading_label.clone()
                    } else {
                        input.load_label.clone()
                    },
                    disabled: input.loading,
                    test_id: "cell-editor-load-candidates",
                })
            } else {
                None
            };
            let failure = if input.failed {
                let message = input.message.clone().filter(|text| !text.is_empty());
                Some(message.unwrap_or_else(|| input.failure_label.clone()))
            } else {
                None
            };
            EnumEditorMode::Prompt {
                draft: input.draft.clone(),
                input_test_id: "table-edit-input",
                load_button,
                failure,
            }
        }
        Some(candidates) => {
            let options = candidates
                .iter()
                .map(|value| SelectOption {
                    value: value.clone(),
                    label: value.clone(),
                })
                .collect::<Vec<_>>();
            let selected = if options.iter().any(|option| option.value == input.draft) {
                input.draft.clone()
            } else {
                String::new()
            };
            EnumEditorMode::Select {
                options,
                selected,
                test_id: "cell-editor-enum",
                searchable: true,
                pick_enabled: input.has_pick,
            }
        }
    }
}

pub fn enum_key_decision(key: &str) -> KeyDecision {
    if key == "Enter" {
        KeyDecision::Swallow
    } else {
        KeyDecision::Forward
    }
}
