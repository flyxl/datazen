//! Named-view domain: the wire shape, the rules, and the storage boundary.
//!
//! Everything that decides *what* a named-view list looks like lives here:
//! the DTO, the name and scope rules, the storage port and the id minter.
//! A desktop host and a web host reach the same answers here; the host only
//! contributes persistence and id minting.

pub mod dto;
pub mod error;
pub mod repository;
pub mod rules;

pub use dto::{NamedView, NamedViewScope};
pub use error::NamedViewError;
pub use repository::{NamedViewIdMinter, NamedViewRepository};
pub use rules::{
    normalize_view_name, normalized_name_key, resolve_import_name, scope_key, view_names_equal,
};
