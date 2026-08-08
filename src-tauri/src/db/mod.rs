//! Database driver abstraction and shared types.
//!
//! Types and traits are re-exported from `datazen_driver_api` so that the rest
//! of the main crate can continue using `crate::db::*` unchanged.

pub use datazen_driver_api::*;

pub mod mysql;
pub mod postgres;
pub mod redis_driver;
pub mod sqlite;
pub mod registry;
pub mod reuse;
pub mod http_support;
pub mod mongodb;
pub mod sqlserver;
pub mod clickhouse;
pub mod duckdb;
pub mod elasticsearch;
pub mod rqlite;
pub mod turso;
pub mod influxdb;
pub mod victoriametrics;
pub mod hbase;
pub mod vector;

pub use registry::{init_drivers, DriverRegistry};
