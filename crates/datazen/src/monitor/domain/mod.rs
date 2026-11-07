//! Monitor domain vocabulary and admission rules.

pub mod grant;

pub use grant::{revision, validate_read_only_sql, MonitorGrant, MonitorScope};
