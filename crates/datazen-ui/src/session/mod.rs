//! Session value logic ported from React `packages/frontend/src/lib/session/`.
//!
//! Everything here is pure value judgements — counter narrowing, handle
//! identity and the wire form — with no Rune tree, no state and no IPC. The
//! session controllers that consume it land in later tasks; until then the
//! reusable API is `pub(crate)` and silences `dead_code` per item.

pub(crate) mod counters;
pub(crate) mod editor_session_controller;
pub(crate) mod execution_projection;
pub(crate) mod page_unload;
pub(crate) mod query_panel_session;
pub(crate) mod query_panel_session_root;
pub(crate) mod run_editor_explain;
