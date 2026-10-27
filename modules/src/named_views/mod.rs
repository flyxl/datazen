//! Named views vertical slice: domain vocabulary, repository boundary, and use cases.
//!
//! Migrated from `datazen::services::named_views` (D6 item 1,
//! 2026-10-26). The rules — name uniqueness inside a scope, the
//! one-default-per-scope rule, the idempotent delete, the menu order — are
//! host-neutral; the host provides only the storage port and the id minter.

pub mod application;
pub mod domain;

#[cfg(test)]
mod tests;

pub use application::{delete_named_view, list_named_views, save_named_view, NamedViewService};
pub use domain::{
    normalize_view_name, normalized_name_key, resolve_import_name, scope_key, view_names_equal,
    NamedView, NamedViewError, NamedViewIdMinter, NamedViewRepository, NamedViewScope,
};
