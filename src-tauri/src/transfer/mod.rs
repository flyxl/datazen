//! Cross-database sync with an LLVM-style intermediate type representation.

pub mod adapter_registry;
pub mod adapters;
pub mod ddl;
pub mod pairing;

pub use datazen_driver_api::sync::{
    BoxedSyncAdapter, IRColumn, IRDefault, IRTable, IRType, SyncAdapterFactory, SyncSourceAdapter,
    SyncTargetAdapter,
};

/// Compatibility re-exports so existing `crate::transfer::adapter` / `crate::transfer::ir` paths keep working.
pub mod adapter {
    pub use super::{SyncSourceAdapter, SyncTargetAdapter};
}

pub mod ir {
    pub use super::{IRColumn, IRDefault, IRTable, IRType};
}
