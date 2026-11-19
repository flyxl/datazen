//! The `windows/settings/ai` subtree of the settings view (AiContextSettings
//! is its first port; ModelProfileDialog/Form/ListItem and useModelProfileDraft
//! land with their own tasks).

pub(crate) mod ai_context_settings;
// T479: the draft/validate/save state machine behind the model-profile form.
pub(crate) mod use_model_profile_draft;
