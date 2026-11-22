//! Verbatim ports of the libraries `buildFlatRows.ts` consumes, kept private
//! to the flat-row builder until their formal shard-2 tasks land. One file
//! per React source, so the future move is a directory rename:
//!
//! | file | React source | formal task |
//! | --- | --- | --- |
//! | [`object_filter`] | `lib/objectFilter.ts` | shard 2, T567 |
//! | [`locator`] | `lib/connectionLocator.ts` (search half) | shard 2, T403 |
//! | [`sql_namespace`] | `lib/sqlNamespace.ts` (tree + navigators) | shard 2, T617 |
//! | [`tree_rules`] | `@datazen/ui` `tree/{geometry,navigation}.ts` | shared tree kit |
//! | [`categories`] | `schema-tree/schemaTreeCategories.ts` | schema-tree family |
//! | [`utils`] | `navigator/utils.ts` (flat-row half) | T582 |
//! | [`groups`] | `lib/connectionGroups.ts` (label half) | shard 2, T394 |

pub(crate) mod categories;
pub(crate) mod groups;
pub(crate) mod locator;
pub(crate) mod object_filter;
pub(crate) mod sql_namespace;
pub(crate) mod tree_rules;
pub(crate) mod utils;
