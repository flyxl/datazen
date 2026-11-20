//! Ports of `packages/frontend/src/commands/` — the frontend's typed IPC
//! command wrappers. On the Rust side the IPC boundary collapses into
//! [`DesktopAdapter::execute_driver_command`] (and, for commands with a
//! connection session, the adapter's session methods): one unified entry,
//! typed wrappers above it, exactly the React layering.
//!
//! `adb.ts` (T047) is first; schema-diff (T377), the AI family (T094) and
//! the backup family (T163) have landed; the remaining command families
//! land with their own tasks.

pub(crate) mod adb;
/// T130: the artifact export command (one call, the two-arm receipt).
pub(crate) mod artifact;
/// T163: the app-data backup/import family (merged-IPC shapers).
pub(crate) mod backup;
// T377: the schema-diff commands shell (pure helpers, wire folding, the
// job-observer bus and the submit→poll→resolve machine).
pub(crate) mod schema_diff;
// T094: the AI command family (ai_*/mcp_*/prompt_* legacy shapers, the
// workflow domain-client slice, the stream-event edges).
pub(crate) mod ai;
