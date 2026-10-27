//! Schema metadata ports: the logical and physical sources every metadata
//! consumer depends on (moved from `datazen::services::ports`,
//! D4-b 2026-10-26).
//!
//! The logical half ([`LogicalSchemaContextSource`]) is the port every
//! consumer uses; the physical half ([`PhysicalSchemaContextSource`]) is
//! answered only by sources that own live connection handles. The adapter
//! presenting a physical source through the logical port lives in
//! [`crate::schema::application::physical_context`].

use std::sync::Arc;

use datazen_driver_api::{ConnectionHandle, DatabaseDriver, TableInfo, TableSchema};

/// The metadata a form-neutral consumer needs to describe a session's database.
///
/// Every method here is answered from the session's *identity* — which profile
/// owns it, and which metadata commands its connection answers — so a source
/// never has to own a live [`ConnectionHandle`]. The Runtime path addresses
/// sessions logically and implements this trait in full; it is the port every
/// metadata consumer depends on.
///
/// The trait carries **no** physical-shaped default implementation. A default
/// built on a connection handle could only ever be inherited by a source that
/// owns one, and the only source in the tree owns none — so the defaults were
/// unusable everywhere and had to be overridden one by one. The physical half
/// lives on [`PhysicalSchemaContextSource`], where it belongs.
#[async_trait::async_trait]
pub trait LogicalSchemaContextSource: Send + Sync {
    /// The driver that serves the session's connection.
    async fn driver_for_session(&self, session_id: &str)
        -> Result<Arc<dyn DatabaseDriver>, String>;
    /// The schema configured on the session's connection, if any.
    async fn session_schema(&self, session_id: &str) -> Option<String>;
    /// The tables of one `(database, schema)` scope.
    async fn tables(
        &self,
        session_id: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, String>;
    /// The full schema of one table.
    async fn table_schema(
        &self,
        session_id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<TableSchema, String>;
}

/// The metadata source of a form that owns live connection handles.
///
/// This is the *physical* half of the metadata port: the source answers with
/// the driver's own handle, so metadata can be read straight against the
/// connection. The `ConnectionHandle`-based reads are not default methods of
/// this trait — they need the [`SchemaCache`](crate::schema::SchemaCache)
/// that memoizes them, and the cache belongs to the implementation that owns
/// the session, not to the caller of the port. They live in
/// [`PhysicalSchemaContextAdapter`](crate::schema::application::physical_context::PhysicalSchemaContextAdapter),
/// which presents a physical source through [`LogicalSchemaContextSource`].
///
/// A source that addresses sessions logically — the Runtime path — has no
/// handle to hand out, so it answers this contract with the refusal that
/// rejects physical-handle exposure. Its own reads live on
/// [`LogicalSchemaContextSource`], which is the port every consumer uses.
#[async_trait::async_trait]
pub trait PhysicalSchemaContextSource: Send + Sync {
    /// The live driver and physical handle of a session.
    async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String>;
    /// The schema configured on the session's connection, if any.
    ///
    /// The same fact the logical port calls `session_schema`, under a name of
    /// its own: a source that answers both contracts would otherwise carry two
    /// identically named methods, and every call on it would be ambiguous.
    async fn configured_schema(&self, session_id: &str) -> Option<String>;
}
