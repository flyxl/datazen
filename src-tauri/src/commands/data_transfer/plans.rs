//! Thin command-layer adapter for the shared transfer plan store.
//!
//! The plan repository itself (issued plans, checkpoints, the one-shot
//! consume/lease state machine) lives in
//! [`datazen_data_transfer::plan_store`], next to the transfer engine it
//! guards — the command layer only forwards names so host call sites keep
//! their paths. A preview is the authority for a later run: the client
//! receives only the opaque plan id and may choose a subset of already
//! planned tables plus the final destructive confirmation; endpoints,
//! mappings, DDL and schema snapshots stay private to the store.

pub use datazen_data_transfer::plan_store::*;
