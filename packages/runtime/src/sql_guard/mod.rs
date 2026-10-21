//! Connection-level SQL safety: read-only connections, Safe Mode, and parameter binding.

mod keyword;
mod params;
mod safety;
mod scanner;

pub use keyword::{
    find_main_verb_index, sql_main_keyword, strip_comments_for_tokenize, tokenize_sql,
};
pub use params::apply_params;
pub use safety::{check_sql, normalize_fullwidth, reject_null_bytes};

#[cfg(test)]
pub use safety::is_write_sql;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value as JsonValue};

    include!("tests/safety_regression.rs");
    include!("tests/params_regression.rs");
    include!("tests/keyword_regression.rs");
}
