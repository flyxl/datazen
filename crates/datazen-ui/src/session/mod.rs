//! Session value logic ported from React `packages/frontend/src/lib/session/`.
//!
//! Everything here is pure value judgements — counter narrowing, handle
//! identity and the wire form — with no Rune tree, no state and no IPC. The
//! session controllers that consume it land in later tasks; until then the
//! reusable API is `pub` and silences `dead_code` per item.

pub mod counters;
pub mod editor_session_controller;
pub mod execution_projection;
pub mod page_unload;
pub mod query_panel_session;
pub mod query_panel_session_root;
pub mod run_editor_explain;
pub mod run_editor_query;
pub mod session_prompts;
pub mod table_session;
pub mod target_command;
pub mod settings_export;
