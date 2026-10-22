//! Shared feature logic. Hosts bind I/O ports; no native application types occur here.
pub mod ai;
pub mod backup;
pub mod backup_gateway;
pub mod connection_import;
pub mod dashboard;
pub mod data_browsing;
pub mod error;
pub mod export;
pub mod mcp;
pub mod services;
pub mod workflow;

pub mod file_job;

pub mod restore_job;

pub mod restore_gateway;
