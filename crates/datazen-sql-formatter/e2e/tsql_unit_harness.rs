// Link the exact instrumented production rlib; do not instrument this test harness.
pub use datazen_sql_formatter::{format, Dialect, Error, Options};
#[path = "../src/layout/tests/tsql_format.rs"]
mod tsql_format;
