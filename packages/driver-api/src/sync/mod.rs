//! Cross-database sync IR and adapter traits.
//!
//! Path drivers implement adapters and register them via [`SyncAdapterFactory`]
//! + `inventory`. The host owns the runtime registry and orchestration.

mod adapter;
mod ir;

pub use adapter::*;
pub use ir::*;
