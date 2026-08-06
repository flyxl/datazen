//! Concrete sync adapters for each supported database type.

pub mod mysql;
pub mod postgresql;
pub mod sqlite;
pub mod trino;

#[cfg(test)]
mod roundtrip_tests;
