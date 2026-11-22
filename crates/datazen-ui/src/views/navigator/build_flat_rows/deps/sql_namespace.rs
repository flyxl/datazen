//! `lib/sqlNamespace.ts` — the tree type and the two navigators the flat-row
//! builder needs, ported verbatim for T570. Removal stage: the formal
//! `sqlNamespace` port is shard 2's T617; until then this copy serves the
//! builder (and the locator's namespace candidates) only.
//!
//! React's `SqlNamespace` is `{ [name]: SqlNamespace } | readonly string[]` —
//! a branch is a record, a table leaf is an array of column names (the array
//! itself is never read by the tree; it only distinguishes leaves). Rust gets
//! the same shape as an enum, with a `BTreeMap` for the record so key
//! iteration is sorted — React sorts at the use site with `localeCompare`,
//! and so do the two users here.

use std::collections::BTreeMap;

/// React `SqlNamespace`: either a branch of named children or a table leaf
/// (the leaf's column array carries no meaning to the tree).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum SqlNamespace {
    #[default]
    Empty,
    /// A branch whose children are `name -> child`.
    Branch(BTreeMap<String, SqlNamespace>),
    /// A table leaf; the payload is its column list, kept for parity.
    Leaf(Vec<String>),
}

impl SqlNamespace {
    /// React `isLeaf` — an array, no matter how many entries.
    pub(crate) fn is_leaf(&self) -> bool {
        matches!(self, SqlNamespace::Leaf(_))
    }

    /// The record view React gets from `Object.entries(tree)`; leaves and the
    /// empty tree read as an empty record, exactly as `isLeaf`-guarded JS does.
    pub(crate) fn entries(&self) -> Vec<(String, SqlNamespace)> {
        match self {
            SqlNamespace::Branch(map) => map
                .iter()
                .map(|(key, child)| (key.clone(), child.clone()))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// How many named children a branch has (leaves and empty: 0).
    pub(crate) fn len(&self) -> usize {
        match self {
            SqlNamespace::Branch(map) => map.len(),
            _ => 0,
        }
    }
}

/// React `pathKey` — the `/`-joined path of a namespace segment list.
pub(crate) fn path_key(segments: &[&str]) -> String {
    segments.join("/")
}
