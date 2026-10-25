//! Connection vertical slice: domain vocabulary, repository boundary, and use cases.

pub mod application;
pub mod domain;

pub use application::{
    redact_secrets, merge_missing_secrets, ConnectionError, ConnectionService,
};
pub use domain::{ConnectionRepository, ConnectionRepositoryError, ConnectionScope};
