//! Pure logic of `windows/connection/tableEditing.ts` (DB-05 / DB-20 / DBR-05).
//!
//! React keeps one editing context per open panel so its entry points can be
//! plain functions: the grid asks whether a cell may be edited, the keyboard
//! chapter opens an editor with `Enter`, the navigation chapter asks an open
//! editor to commit before it moves the page, and the "Set Value" menu derives
//! its entries from the column and the row. This port keeps every one of those
//! decisions as a pure function over host-owned state. There is no registry
//! with global mutable state, no command, no timer and no fake IPC: the host
//! supplies the open editor, the driver facts and the command results, and
//! applies the returned intents.
//!
//! Two invariants are load-bearing and pinned by the tests:
//!
//! - the panel's **read-only reason** (mount reason → connection → driver
//!   capability → relation kind → missing primary key) is a separate decision
//!   from a single cell's editability (which additionally checks row identity,
//!   auto-increment, type rules and byte size) — an unloaded panel with no
//!   columns is *not* refused, but a loaded keyless table is;
//! - applying a saved value goes through the one staging path; the two dialog
//!   entries are routed to their host and never stage by themselves, and a
//!   draft the editor refuses to commit stops the navigation instead of being
//!   silently dropped.
//!
//! ## Removal stage
//!
//! No production host mounts this module yet; the table host will own the VMs
//! and execute the emitted intents. Per-item `#[allow(dead_code)]` annotations
//! carry that stage: drop each item when the host owns that editing decision.

mod candidates;
mod editability;
mod navigation;
mod set_value;

pub use candidates::*;
pub use editability::*;
pub use navigation::*;
pub use set_value::*;
