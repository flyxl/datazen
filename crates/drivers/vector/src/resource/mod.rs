//! Qdrant's implementation of the resource contract.
//!
//! This is a hand-written [`ResourceProvider`], not a shell over a legacy
//! adapter: `VectorDriver` had no `LegacyResourceAdapter` to reuse, so every
//! one of the contract's **14 methods** is answered explicitly in
//! [`provider::contract`]. Nine of them do real work against the driver's own
//! machinery; the remaining five answer `Unsupported` *as* an explicit value,
//! never as a silent `Ok(())`.
//!
//! Why the split matters for a driver with no SQL, no schema and no
//! transactions:
//!
//! * `describe_resource`, `acquire_resource`, `execute_on_resource`,
//!   `close_resource` and `observe_session` run against the driver's real
//!   `reqwest` client pool.
//! * `begin_transaction`, `commit_transaction` and `rollback_transaction`
//!   return `Err(ResourceError::OperationNotSupported)` — the Qdrant REST API
//!   this driver speaks exposes no transaction endpoint, and the driver already
//!   refuses writes outright
//!   ([`VectorDriver::execute`](crate::vector::VectorDriver::execute)), so a
//!   transaction here would cover nothing.
//! * `change_context` returns `Unsupported` rather than a fake in-place switch:
//!   an instance has exactly one namespace and it is fixed by the connection
//!   URL.
//! * `request_cancel` returns [`CancelDisposition::Unsupported`] and never calls
//!   `cancel_query`, which is a no-op `Ok(())` in this driver.
//! * `reset_resource` returns [`ResetDisposition::Discard`]: nothing is replayed
//!   and nothing can be proven clean, so the resource is never handed out again
//!   without a full re-acquire.
//!
//! Every cell of [`capabilities::vector_capability_set`] carries a reason, and
//! the two domains this provider cannot honestly fill in — `data` and `backup` —
//! are left `Unknown` rather than guessed. See that module for why.

mod capabilities;
mod observation;
mod payload;
mod provider;
mod registry;

#[cfg(test)]
mod tests;

pub use provider::VectorResourceProvider;
