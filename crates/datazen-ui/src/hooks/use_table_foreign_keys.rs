//! Port of `packages/frontend/src/hooks/useTableForeignKeys.ts` (T422).
//!
//! The foreign keys of one table, as the grid needs them (DB-07).
//!
//! The table data does **not** carry the foreign keys:
//! `get_table_data` answers with rows and columns, and the constraints
//! live in the relation schema. The schema cache is the one entry
//! point for it — the same one the "foreign keys" structure tab reads
//! — so the grid and the structure tab can never disagree about a
//! constraint.
//!
//! A failure is a **degradation, not an error**: the metadata may be
//! unreadable (no permission, a table that vanished, a driver that
//! cannot describe it). Then the grid simply has no arrows. The rest
//! of the grid must keep working, so the error is returned as a value
//! and never thrown.
//!
//! ## What maps where
//!
//! - The schema-cache read (`getCachedTableSchema` — `lib/schemaCache.ts`,
//!   T589, another segment) is the host's seam: [`mount`] returns the
//!   exact read arguments ([`TableSchemaRead`]], the React test pins
//!   them call-argument for call-argument) and the host feeds the
//!   answer back through [`on_schema`] / [`on_error`].
//! - `indexForeignKeys` + `FkGroupAffordance` are carried from
//!   `lib/fkNavigation.ts` (T515, another segment) as
//!   [`index_foreign_keys`] below — the same carry the
//!   useFkNavigationShortcuts port documented for its halves.
//! - The effect's `cancelled` gate becomes the rule that no callback
//!   mutates state after [`unmount`].
//! - `EMPTY_INDEX` (the shared empty map so the grid does not
//!   re-render on every failure) is the `HashMap`'s default; Rust has
//!   no reference identity to share, and a fresh empty map is
//!   behaviourally identical.
//!
//! Known deltas: `schema: string | null` and `connectionId?: string |
//! null` collapse to `Option<String>` (the `?? undefined`
//! normalization's only effect was the JS argument spelling; the
//! cache key treats absent and null the same). `tableSchema.
//! foreignKeys ?? []` needs no port — the Rust `TableSchema` always
//! carries the `Vec` (an empty one is the absent case).

use std::collections::HashMap;

use datazen_driver_api::{ForeignKeyInfo, TableSchema};

/// `FkGroupAffordance` (`lib/fkNavigation.ts`, carried — T515): one
/// foreign-key group as the grid sees it. Every member column of the
/// group maps to the same entry, so a cell can ask "which group am I
/// part of?" with one lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FkGroupAffordance {
    /// The local columns of the group, in `ForeignKeyInfo.columns`
    /// order.
    pub(crate) local_columns: Vec<String>,
    /// The referenced columns, aligned with `local_columns`.
    pub(crate) target_columns: Vec<String>,
    /// Display label of the target: `customers.id` or
    /// `sales.customers.(x, y)`.
    pub(crate) target_label: String,
    /// The constraint name; disambiguates two constraints over the
    /// same column.
    pub(crate) constraint_name: String,
    /// The referenced table as the driver reported it (never split
    /// here).
    pub(crate) reference_table: String,
}

/// `targetLabelOf`: the display label of a foreign-key target.
fn target_label_of(reference_table: &str, target_columns: &[String]) -> String {
    if target_columns.len() == 1 {
        format!("{reference_table}.{}", target_columns[0])
    } else {
        format!("{reference_table}.({})", target_columns.join(", "))
    }
}

/// `indexForeignKeys` (`lib/fkNavigation.ts`, carried — T515):
/// flatten a table's foreign keys into a `column → group` index.
///
/// Every member column of a group points at the same group, so a
/// composite key stays one thing instead of splitting into
/// single-column lookups. When two constraints cover the same column
/// the **first** one wins, mirroring the "take the first
/// ordered-columns match" rule of the request path. (The React map
/// shares one group object per constraint; the port's entries are
/// value-equal — the same group, read twice.)
pub(crate) fn index_foreign_keys(
    foreign_keys: &[ForeignKeyInfo],
) -> HashMap<String, FkGroupAffordance> {
    let mut index: HashMap<String, FkGroupAffordance> = HashMap::new();
    for foreign_key in foreign_keys {
        let group = FkGroupAffordance {
            local_columns: foreign_key.columns.clone(),
            target_columns: foreign_key.referenced_columns.clone(),
            target_label: target_label_of(
                &foreign_key.referenced_table,
                &foreign_key.referenced_columns,
            ),
            constraint_name: foreign_key.name.clone(),
            reference_table: foreign_key.referenced_table.clone(),
        };
        for column in &foreign_key.columns {
            index.entry(column.clone()).or_insert_with(|| group.clone());
        }
    }
    index
}

/// `UseTableForeignKeysParams`: what identifies the read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UseTableForeignKeysParams {
    /// The runtime session the table belongs to.
    pub(crate) db_session_id: String,
    /// The table the constraints belong to.
    pub(crate) table_name: String,
    /// The database of the table.
    pub(crate) database: String,
    /// The namespace of the table; `None` when the driver has none.
    pub(crate) schema: Option<String>,
    /// The connection of the table; part of the metadata cache key.
    pub(crate) connection_id: Option<String>,
}

/// The schema-cache read the mount asks for (the
/// `getCachedTableSchema(dbSessionId, tableName, database, schema,
/// connectionId ?? undefined)` call's arguments).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TableSchemaRead {
    pub(crate) db_session_id: String,
    pub(crate) table_name: String,
    pub(crate) database: String,
    pub(crate) schema: Option<String>,
    pub(crate) connection_id: Option<String>,
}

/// `useTableForeignKeys(params)`'s state machine.
pub(crate) struct UseTableForeignKeys {
    params: UseTableForeignKeysParams,
    /// `index`: the columns that take part in a foreign key, mapped
    /// to their group.
    index: HashMap<String, FkGroupAffordance>,
    /// The metadata is being read.
    loading: bool,
    /// Why the metadata is missing; `None` when it was read.
    error: Option<String>,
    cancelled: bool,
}

impl UseTableForeignKeys {
    /// `useTableForeignKeys(params)` — the hook's initial state
    /// (empty index, not loading, no error).
    pub(crate) fn new(params: UseTableForeignKeysParams) -> Self {
        Self { params, index: HashMap::new(), loading: false, error: None, cancelled: false }
    }

    /// The effect: ask the schema cache for this table's schema. The
    /// returned read is the host's cue (`loading` is already true).
    pub(crate) fn mount(&mut self) -> TableSchemaRead {
        self.loading = true;
        self.error = None;
        TableSchemaRead {
            db_session_id: self.params.db_session_id.clone(),
            table_name: self.params.table_name.clone(),
            database: self.params.database.clone(),
            schema: self.params.schema.clone(),
            connection_id: self.params.connection_id.clone(),
        }
    }

    /// The `.then` arm: index the table's foreign keys. A no-op once
    /// unmounted (the `cancelled` gate).
    pub(crate) fn on_schema(&mut self, table_schema: &TableSchema) {
        if self.cancelled {
            return;
        }
        self.index = index_foreign_keys(&table_schema.foreign_keys);
        self.loading = false;
    }

    /// The `.catch` arm: degrade — no arrows, the grid keeps working.
    /// A no-op once unmounted.
    pub(crate) fn on_error(&mut self, message: String) {
        if self.cancelled {
            return;
        }
        self.index = HashMap::new();
        self.error = Some(message);
        self.loading = false;
    }

    /// The effect cleanup's `cancelled = true`: no later callback may
    /// mutate state.
    pub(crate) fn unmount(&mut self) {
        self.cancelled = true;
    }

    /// `result.index`.
    pub(crate) fn index(&self) -> &HashMap<String, FkGroupAffordance> {
        &self.index
    }

    /// `result.loading`.
    pub(crate) fn loading(&self) -> bool {
        self.loading
    }

    /// `result.error`.
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> UseTableForeignKeysParams {
        UseTableForeignKeysParams {
            db_session_id: "session-1".to_owned(),
            table_name: "orders".to_owned(),
            database: "app".to_owned(),
            schema: None,
            connection_id: None,
        }
    }

    fn fk(name: &str, columns: &[&str], table: &str, referenced: &[&str]) -> ForeignKeyInfo {
        ForeignKeyInfo {
            name: name.to_owned(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
            referenced_table: table.to_owned(),
            referenced_columns: referenced.iter().map(|s| s.to_string()).collect(),
            on_update: String::new(),
            on_delete: String::new(),
            deferrability: Default::default(),
        }
    }

    fn schema_with(foreign_keys: Vec<ForeignKeyInfo>) -> TableSchema {
        TableSchema {
            table_name: "orders".to_owned(),
            columns: Vec::new(),
            primary_keys: Vec::new(),
            indexes: Vec::new(),
            foreign_keys,
            check_constraints: Vec::new(),
            table_options: Default::default(),
        }
    }

    #[test]
    fn indexes_the_foreign_keys_of_the_table_by_column() {
        let mut hook = UseTableForeignKeys::new(params());
        hook.mount();
        hook.on_schema(&schema_with(vec![
            fk("orders_customer_fk", &["customer_id"], "customers", &["id"]),
            fk("orders_parent_fk", &["parent_a", "parent_b"], "parents", &["x", "y"]),
        ]));
        assert!(!hook.loading());
        assert_eq!(hook.error(), None);
        assert_eq!(hook.index().len(), 3);
        // Both members of the composite key point at the same group
        // (the React `.toBe` identity pin — value-equal here).
        assert_eq!(hook.index().get("parent_a"), hook.index().get("parent_b"));
        assert_eq!(
            hook.index().get("customer_id").expect("group").target_label,
            "customers.id"
        );
        assert_eq!(hook.index().get("parent_a").expect("group").target_label, "parents.(x, y)");
    }

    #[test]
    fn degrades_silently_when_the_metadata_cannot_be_read() {
        let mut hook = UseTableForeignKeys::new(params());
        hook.mount();
        hook.on_error("no permission for table orders".to_owned());
        assert!(!hook.loading());
        assert!(hook.index().is_empty());
        assert!(hook.error().expect("degradation message").contains("no permission"));
    }

    #[test]
    fn has_no_index_before_the_metadata_arrives() {
        let mut hook = UseTableForeignKeys::new(params());
        // The effect ran (mount), the read has not answered yet.
        hook.mount();
        assert!(hook.loading());
        assert!(hook.index().is_empty());
        assert_eq!(hook.error(), None);
    }

    #[test]
    fn reads_the_metadata_of_the_table_it_is_given() {
        let mut hook = UseTableForeignKeys::new(params());
        let read = hook.mount();
        assert_eq!(read.db_session_id, "session-1");
        assert_eq!(read.table_name, "orders");
        assert_eq!(read.database, "app");
        assert_eq!(read.schema, None);
        assert_eq!(read.connection_id, None);
    }

    #[test]
    fn after_unmount_no_callback_mutates_state() {
        let mut hook = UseTableForeignKeys::new(params());
        hook.mount();
        hook.unmount();
        hook.on_schema(&schema_with(vec![fk("fk", &["c"], "t", &["id"])]));
        assert!(hook.index().is_empty());
        assert!(hook.loading(), "the read is still notionally in flight");
        hook.on_error("late failure".to_owned());
        assert_eq!(hook.error(), None);
    }

    #[test]
    fn the_first_constraint_over_a_column_wins() {
        let index = index_foreign_keys(&[
            fk("first_fk", &["shared"], "first", &["id"]),
            fk("second_fk", &["shared"], "second", &["id"]),
        ]);
        assert_eq!(index.len(), 1);
        assert_eq!(index.get("shared").expect("group").constraint_name, "first_fk");
        assert_eq!(index.get("shared").expect("group").reference_table, "first");
    }

    #[test]
    fn a_failed_re_read_clears_the_stale_index_of_the_previous_table() {
        // Params changed: the effect re-runs with the previous index
        // still in place (React state persists across effect runs);
        // the failure arm must degrade back to EMPTY_INDEX.
        let mut hook = UseTableForeignKeys::new(params());
        hook.mount();
        hook.on_schema(&schema_with(vec![fk("fk", &["customer_id"], "customers", &["id"])]));
        assert_eq!(hook.index().len(), 1);
        hook.mount();
        assert!(hook.loading());
        assert_eq!(hook.index().len(), 1, "the old index stays until the read answers");
        hook.on_error("table vanished".to_owned());
        assert!(hook.index().is_empty(), "degradation means no arrows");
        assert_eq!(hook.error(), Some("table vanished"));
    }

    #[test]
    fn the_group_carries_the_aligned_member_lists() {
        let index = index_foreign_keys(&[
            fk("orders_parent_fk", &["parent_a", "parent_b"], "parents", &["x", "y"]),
        ]);
        let group = index.get("parent_b").expect("group");
        assert_eq!(group.local_columns, ["parent_a", "parent_b"]);
        assert_eq!(group.target_columns, ["x", "y"]);
        assert_eq!(group.constraint_name, "orders_parent_fk");
        assert_eq!(group.reference_table, "parents");
    }
}
