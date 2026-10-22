//! Named views — the saved filter views of one table (08 §3.7).
//!
//! The feature is split in two, and the split is the point:
//!
//! - **this module** holds everything that does not depend on where the views
//!   live: the DTO, the name and scope rules, the storage port and the
//!   service that applies the rules. A browser build and the desktop host
//!   reach the same answers here.
//! - **the host** holds the persistence. The desktop writes
//!   `{appData}/named_views.json` through its own store; a browser build has
//!   no such place yet, so its transport declares the capability gap instead
//!   of answering with an empty list that reads like "you have no views".
//!
//! The rules are:
//!
//! - a view's name is unique **inside its scope**, compared case-insensitively
//!   after the name is trimmed and its whitespace collapsed;
//! - at most one view per scope is the default;
//! - a save mints the id and the timestamps, and a rename keeps the id;
//! - a delete is idempotent;
//! - the list is ordered with the default first, then by name.

mod dto;
mod error;
mod repository;
mod rules;
mod service;

#[cfg(test)]
mod tests;

pub use dto::{NamedView, NamedViewScope};
pub use error::NamedViewError;
pub use repository::{NamedViewIdMinter, NamedViewRepository};
pub use rules::{
    normalize_view_name, normalized_name_key, resolve_import_name, scope_key, view_names_equal,
};
pub use service::{delete_named_view, list_named_views, save_named_view};
