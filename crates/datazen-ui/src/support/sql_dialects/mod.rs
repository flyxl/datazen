//! Ports of `packages/frontend/src/lib/sqlDialects/` — dialect metadata the
//! editor, the Visual Query Builder and the host's strategy registry share
//! (dialectProfile and extra are the first; identifierQuoting, the
//! per-dialect modules and rowLimit land with their own tasks).

pub(crate) mod dialect_profile;
pub(crate) mod extra;
