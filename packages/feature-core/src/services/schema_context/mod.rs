//! Shared implementation of the [`SchemaContextSource`](crate::services::ports::SchemaContextSource) port.
//!
//! The AI feature needs database metadata (tables, columns, types, schema) for
//! the session the user is working in. That read is **not** a desktop concern:
//! every form that owns a Runtime session needs the same answer. The
//! implementation therefore lives here, in the shared feature layer, and a
//! form only supplies the two cross-form dependencies:
//!
//! - [`RuntimeConnectionUseCases`](datazen_runtime::application::RuntimeConnectionUseCases)
//!   for the session-to-connection mapping, and
//! - [`ResourceExecutionHost`](crate::workflow::resource_execution::ResourceExecutionHost)
//!   for the profile, the driver and command execution.
//!
//! The source holds no host type: it cannot see a store, a platform entry or
//! a driver registry, which is what keeps the same code usable by the desktop
//! and by a future web form.

mod source;

pub use source::{RuntimeSchemaContextSource, UnavailableSchemaContextSource};

#[cfg(test)]
mod tests;
