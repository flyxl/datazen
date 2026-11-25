//! Robust relation metadata resolver — the React
//! `packages/frontend/src/components/sql-editor/metadata/findRelation.ts`
//! port (T242).
//!
//! Resolves an editor relation from the metadata snapshot with a fallback
//! to the editor schema tree, matching in order:
//!
//! 1. direct folded-key match;
//! 2. `dbSessionId::key` match (the cache's canonical key shape);
//! 3. case-insensitive raw-key **or** table-name match;
//! 4. fallback metadata synthesized from the editor schema tree (columns
//!    only, everything degraded to `text`/nullable, `id`-shaped primary
//!    key guess — a completion-grade shape, not a schema-browser one).
//!
//! Pure logic: no Rune tree, no `oklch`, no i18n, no IPC. Case folding
//! uses `str::to_lowercase`, the Rust counterpart of JS
//! `String.prototype.toLowerCase` (the same rule as the rest of the
//! relation-metadata family).
//!
//! # Deviations (semantic, documented deliberately)
//!
//! * The tier-3 linear scan walks the snapshot's `BTreeMap` in **sorted
//!   key order**, where React's `Map` walks insertion order. Both orders
//!   return "the first case-insensitive hit"; only the winner among
//!   *multiple* hits can differ, and no React test pins that tie. The
//!   landed snapshot type (segment 1's `EditorMetadataSnapshot`) is the
//!   single source of truth for the map kind, not this module.
//! * `Date.now()` for the synthesized fallback's `loadedAt` reads the
//!   system clock exactly like the cache's due-at stamps
//!   (`UNIX_EPOCH` millis, `i64`).

use std::time::{SystemTime, UNIX_EPOCH};

use crate::relation_metadata::identity::{
    relation_identity_key, EditorRelationKind, QualifiedRelationId,
};
use crate::relation_metadata::schema::{
    ColumnSchema, EditorMetadataSnapshot, EditorRelationMetadata,
};

use super::super::sql_namespace::SqlNamespace;

/// `Date.now()` — Unix epoch milliseconds, the `loadedAt` unit.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Find a relation's metadata: snapshot tiers 1-3, then the schema-tree
/// synthesis. `snapshot` and `schema_tree` are both optional and
/// independently skippable, exactly like the React parameters.
pub fn find_relation_metadata(
    snapshot: Option<&EditorMetadataSnapshot>,
    relation: &QualifiedRelationId,
    dialect_id: &str,
    schema_tree: Option<&SqlNamespace>,
) -> Option<EditorRelationMetadata> {
    let name = relation.name.name.as_str();
    let lower_name = name.to_lowercase();

    if let Some(snapshot) = snapshot {
        if !snapshot.relations().is_empty() {
            let key = relation_identity_key(relation, dialect_id);
            if let Some(direct) = snapshot.relation(&key) {
                return Some(direct.clone());
            }

            let prefixed_key = format!("{}::{}", snapshot.db_session_id, key);
            if let Some(prefixed) = snapshot.relation(&prefixed_key) {
                return Some(prefixed.clone());
            }

            let lower_key = key.to_lowercase();
            for (k, rel) in snapshot.relations() {
                // The cache's canonical keys are `dbSessionId::key`; the raw
                // key is whatever follows the marker when present.
                let raw_key = match k.split_once("::") {
                    Some((_, raw)) => raw,
                    None => k.as_str(),
                };
                if raw_key.to_lowercase() == lower_key
                    || rel.identity.name.name.to_lowercase() == lower_name
                {
                    return Some(rel.clone());
                }
            }
        }
    }

    // Fallback to the editor schema tree if available.
    if let Some(tree) = schema_tree {
        if let Some(cols) = extract_columns_from_schema(tree, name) {
            if !cols.is_empty() {
                return Some(synthesize_from_tree(relation.clone(), cols));
            }
        }
    }

    None
}

/// The schema-tree fallback shape: everything the tree knows is a column
/// name, so types degrade to `text`, nullability to `nullable`, and the
/// primary key to "leading `id` column" (the React guess, kept verbatim).
fn synthesize_from_tree(
    relation: QualifiedRelationId,
    cols: &[String],
) -> EditorRelationMetadata {
    EditorRelationMetadata {
        // React keys the synthesized entry by the raw relation name.
        key: relation.name.name.clone(),
        columns: cols
            .iter()
            .enumerate()
            .map(|(idx, col_name)| ColumnSchema {
                name: col_name.clone(),
                data_type: "text".to_string(),
                nullable: true,
                default_value: None,
                is_primary_key: idx == 0 && col_name.to_lowercase() == "id",
                is_auto_increment: false,
                comment: None,
            })
            .collect(),
        identity: relation,
        kind: EditorRelationKind::Table,
        primary_key: cols
            .iter()
            .filter(|c| c.to_lowercase() == "id")
            .cloned()
            .collect(),
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        comment: None,
        loaded_at: now_ms(),
    }
}

/// Depth-first search for a table leaf keyed by `table_name`
/// (case-insensitive), returning its column names. A key that matches but
/// names a nested namespace does not match — only table leaves resolve.
pub fn extract_columns_from_schema<'a>(
    schema: &'a SqlNamespace,
    table_name: &str,
) -> Option<&'a [String]> {
    let lower = table_name.to_lowercase();
    if let SqlNamespace::Branch(branch) = schema {
        for (key, child) in branch.iter() {
            match child {
                SqlNamespace::Table(cols) => {
                    if key.to_lowercase() == lower {
                        return Some(cols);
                    }
                }
                SqlNamespace::Branch(_) => {
                    if let Some(found) = extract_columns_from_schema(child, table_name) {
                        return Some(found);
                    }
                }
            }
        }
    }
    None
}
