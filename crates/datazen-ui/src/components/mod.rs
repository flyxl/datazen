//! React component ports that keep the source tree's shape.
//!
//! One module per React file, at the same path: `components/DataTable/Foo.tsx`
//! becomes `components/data_table/foo.rs`. Nothing is re-exported flat; callers
//! name the module, so a port keeps the React file's identity. Registration is
//! one `mod` line per ported file: parallel migration tracks each add their own
//! line to the owning `mod.rs` instead of sharing a file.

pub(crate) mod data_table;
pub(crate) mod filter;
pub(crate) mod history;
pub(crate) mod query;
pub(crate) mod schema;
