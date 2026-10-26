//! Schema application services: metadata reads, physical-context adaptation,
//! and schema scoping (D4-b, moved from `datazen-feature-core::services`).

pub mod error;
pub mod metadata;
pub mod physical_context;
pub mod ports;
pub mod scope;

pub use error::SchemaError;
pub use ports::{LogicalSchemaContextSource, PhysicalSchemaContextSource};
