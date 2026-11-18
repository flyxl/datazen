//! Rune ports of `packages/frontend/src/components/**`.
//!
//! One module per React file, at the same path: `components/DataTable/Foo.tsx`
//! becomes `components/data_table/foo.rs`. Nothing is re-exported flat; callers
//! name the module, so a port keeps the React file's identity.

pub(crate) mod data_table;
pub(crate) mod history;
