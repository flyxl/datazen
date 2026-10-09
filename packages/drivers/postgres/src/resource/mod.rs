//! The PostgreSQL implementation of the resource contract.
//!
//! The module is split by responsibility, not by convenience:
//!
//! * [`capabilities`] — what this driver *declares*, with the file and line each
//!   claim rests on. A capability that is not proven is declared unsupported.
//! * [`provider`] — the provider itself: the state it owns and the 14 contract
//!   methods, each wired to this crate's own connection and execution code.
//! * [`registry`] — what one live resource remembers. No I/O.
//! * [`observation`] — reading the session back off the server. Every field is
//!   either answered by the server or left empty.
//! * [`payload`] — decoding a Driver Command result for the `ResultSink`, and
//!   refusing the shapes that channel cannot carry truthfully.
//!
//! The rule the whole module exists to enforce: **no method is a shell.** There
//! is no path here that returns `Ok(())`, an empty result, or a fabricated
//! context. A capability this driver cannot back is refused with an explicit
//! [`ResourceError`](datazen_driver_api::resource::ResourceError), and a fact
//! the server did not give is never invented.

mod capabilities;
mod observation;
mod payload;
mod provider;
mod registry;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests_behaviour.rs"]
mod tests_behaviour;

pub use provider::PostgresResourceProvider;
