//! Editor schema tree shape — the `SqlNamespace` union carried from React
//! `packages/frontend/src/lib/sqlNamespace.ts` (its ledger task is T617,
//! segment 2, together with `lib/sqlGenerator.ts`).
//!
//! Only what this crate's sql-editor area consumes is carried:
//!
//! * the recursive union itself — a branch maps child names to sub-trees,
//!   a **table leaf** is the ordered list of that table's column names;
//! * [`is_leaf`], the single discriminator (`Array.isArray` on the React
//!   side; there is no empty-object special case).
//!
//! Everything else in that React module (`mergeNamespacePath`,
//! `omitTableLeaf`, `namespaceChildNames`, …) belongs to tree *editing* and
//! is not read by `components/sql-editor`; it lands — and absorbs this
//! carried slice — with its own task. `buildEditorSchema` (T340) is the
//! producer of this shape.
//!
//! # Map choice
//!
//! A React branch is a plain object, so `Object.entries` walks insertion
//! order — completion order follows the tree's construction order. The Rust
//! branch therefore uses the crate's insertion-ordered
//! [`OrderedMap`][crate::relation_metadata::ordered_map::OrderedMap] rather
//! than a `BTreeMap`, which would silently alphabetize completions.

use crate::relation_metadata::ordered_map::OrderedMap;

/// The editor schema tree: either a namespace branch mapping child names
/// to sub-trees, or a table leaf carrying that table's column names.
///
/// No `PartialEq`: React compares trees by reference, and value equality
/// over an insertion-ordered branch would have to pick an order semantics
/// the React side never defines. Consumers compare the *outputs* (labels,
/// column lists), which stay plain `Vec`s.
#[derive(Clone, Debug)]
pub(crate) enum SqlNamespace {
    /// A schema/catalog/database node: child name → sub-tree.
    Branch(OrderedMap<String, SqlNamespace>),
    /// A table leaf: the table's column names, in tree order.
    Table(Vec<String>),
}

impl SqlNamespace {
    /// An empty branch — the shape `buildEditorSchema` starts from.
    pub(crate) fn empty_branch() -> Self {
        SqlNamespace::Branch(OrderedMap::new())
    }

    /// Build a branch from ordered `(name, child)` pairs.
    pub(crate) fn branch<I, S>(entries: I) -> Self
    where
        I: IntoIterator<Item = (S, SqlNamespace)>,
        S: Into<String>,
    {
        let mut map = OrderedMap::new();
        for (name, child) in entries {
            map.insert(name.into(), child);
        }
        SqlNamespace::Branch(map)
    }

    /// Build a table leaf from column names.
    pub(crate) fn table<I, S>(columns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        SqlNamespace::Table(columns.into_iter().map(Into::into).collect())
    }

    /// A table leaf with no columns — React's `table: []` (e.g.
    /// `flatFromTables`), the "table known, columns not loaded" shape.
    pub(crate) fn empty_table() -> Self {
        SqlNamespace::Table(Vec::new())
    }
}

/// `isLeaf` — the single discriminator between branch and table leaf.
pub(crate) fn is_leaf(node: &SqlNamespace) -> bool {
    matches!(node, SqlNamespace::Table(_))
}
