//! Connection-level SQL safety: read-only connections, Safe Mode, and parameter binding.

mod params;
mod safety;
mod scanner;

pub use params::apply_params;
pub use safety::{
    check_sql, is_write_sql, normalize_fullwidth, normalize_sql, reject_null_bytes,
    strip_sql_comments,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value as JsonValue};

    include!("tests/safety_regression.rs");
    include!("tests/params_regression.rs");
}
