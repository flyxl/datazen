//! React component ports that keep the source tree's shape.
//!
//! The folders and file names mirror `packages/frontend/src/components/`, so a
//! reviewer can diff a port against its React source by path. Registration is
//! one `mod` line per ported file: parallel migration tracks each add their own
//! line to the owning `mod.rs` instead of sharing a file.

pub(crate) mod data_table;
