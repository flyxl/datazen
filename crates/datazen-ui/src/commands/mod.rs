//! Ports of `packages/frontend/src/commands/` — the frontend's typed IPC
//! command wrappers. On the Rust side the IPC boundary collapses into
//! [`DesktopAdapter::execute_driver_command`] (and, for commands with a
//! connection session, the adapter's session methods): one unified entry,
//! typed wrappers above it, exactly the React layering.
//!
//! `adb.ts` (T047) is first; the remaining command families land with
//! their own tasks.

pub(crate) mod adb;
