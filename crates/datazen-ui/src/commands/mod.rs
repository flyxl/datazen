//! Ports of `packages/frontend/src/commands/` — the frontend's typed IPC
//! command wrappers. On the Rust side the IPC boundary collapses into
//! [`DesktopAdapter::execute_driver_command`] (and, for commands with a
//! connection session, the adapter's session methods): one unified entry,
//! typed wrappers above it, exactly the React layering.
//!
//! `adb.ts` (T047) is first; schema-diff (T377), the AI family (T094),
//! backup (T163), the connection family (T190), the context family
//! (T214), the dashboard family (T237) and the database family (T258)
//! have landed; the remaining command families land with their own
//! tasks.

pub(crate) mod adb;
/// T130: the artifact export command (one call, the two-arm receipt).
pub(crate) mod artifact;
/// T163: the app-data backup/import family (merged-IPC shapers).
pub(crate) mod backup;
/// T190: the connection command family + the in-flight connect joiner.
pub(crate) mod connection;
/// T214: the AI context-directory family (get/list/read).
pub(crate) mod context;
/// T237: the dashboard domain-client family + the two legacy dialog faces.
pub(crate) mod dashboard;
/// T258: the database family (target-command shapers, the domain-client
/// rail, the discovery joiner, the legacy privileges face).
pub(crate) mod database;
// T377: the schema-diff commands shell (pure helpers, wire folding, the
// job-observer bus and the submit→poll→resolve machine).
pub(crate) mod schema_diff;
// T094: the AI command family (ai_*/mcp_*/prompt_* legacy shapers, the
// workflow domain-client slice, the stream-event edges).
pub(crate) mod ai;
