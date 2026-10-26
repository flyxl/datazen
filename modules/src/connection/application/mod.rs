//! Connection application use cases.

pub mod profile_view;
pub mod service;

pub use profile_view::ProfileView;
pub use service::{
    merge_missing_secrets, redact_secrets, ConnectionError, ConnectionService,
};
