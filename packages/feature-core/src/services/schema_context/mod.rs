//! Shared implementation of the [`LogicalSchemaContextSource`](crate::services::ports::LogicalSchemaContextSource) port.
//!
//! The AI feature needs database metadata (tables, columns, types, schema) for
//! the session the user is working in. That read is **not** a desktop concern:
//! every form that owns a Runtime session needs the same answer. The
//! implementation therefore lives here, in the shared feature layer, and a
//! form only supplies the two cross-form dependencies:
//!
//! - [`SessionConnectionLookup`](crate::services::ports::SessionConnectionLookup)
//!   for the session-to-connection mapping (a consumer-defined port the host answers), and
//! - [`ResourceExecutionHost`](crate::workflow::resource_execution::ResourceExecutionHost)
//!   for the profile, the driver and command execution.
//!
//! The source holds no host type: it cannot see a store, a platform entry or
//! a driver registry, which is what keeps the same code usable by the desktop
//! and by a future web form.
//!
//! The port it implements is the *logical* one: the Runtime path addresses
//! sessions logically, so it never owns a live connection handle. It answers
//! the physical contract — [`PhysicalSchemaContextSource`](crate::services::ports::PhysicalSchemaContextSource)
//! — with the refusal that rejects physical-handle exposure.

mod source;

pub use source::{RuntimeSchemaContextSource, UnavailableSchemaContextSource};

#[cfg(test)]
mod tests;
