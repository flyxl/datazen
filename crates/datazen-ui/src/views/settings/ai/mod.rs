//! The `windows/settings/ai` subtree of the settings view (AiContextSettings
//! is its first port; ModelProfileDialog/Form/ListItem and useModelProfileDraft
//! land with their own tasks).

pub(crate) mod ai_context_settings;
// T107: the model profile form (basic + safety tabs), split for the
// file-size rule into sections (the two tab bodies) and controls (the
// shared row/control builders).
pub(crate) mod model_profile_form;
pub(crate) mod model_profile_form_controls;
pub(crate) mod model_profile_form_sections;
// T479: the draft/validate/save state machine behind the model-profile form.
pub(crate) mod use_model_profile_draft;
