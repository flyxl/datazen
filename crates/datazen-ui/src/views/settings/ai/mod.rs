//! The `windows/settings/ai` subtree of the settings view (AiContextSettings
//! is its first port; ModelProfileDialog/Form/ListItem and useModelProfileDraft
//! land with their own tasks).

pub mod ai_context_settings;
// T107: the model profile form (basic + safety tabs), split for the
// file-size rule into sections (the two tab bodies) and controls (the
// shared row/control builders).
pub mod model_profile_form;
pub mod model_profile_form_controls;
pub mod model_profile_form_sections;
// T479: the draft/validate/save state machine behind the model-profile form.
pub mod use_model_profile_draft;
// T065: the settings dialog shell around the form (open/close gate, the
// add/edit title, and the hook's onSave/onSaved wiring).
pub mod model_profile_dialog;
// T141: one configured model profile row (badges, meta line, actions).
pub mod model_profile_list_item;
