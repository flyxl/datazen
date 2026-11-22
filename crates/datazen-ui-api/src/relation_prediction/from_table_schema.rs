//! Adapt the ER diagram's table schemas into prediction input.
//!
//! Ported from React `packages/frontend/src/lib/relationPrediction/fromTableSchema.ts`
//! (tasklist T033). The ER diagram already loads a full schema per table —
//! columns with types, the primary key, every index and every declared foreign
//! key — so it can predict over a whole database without any extra IPC. That
//! makes it the cheapest place to use the engine, and the one where a missing
//! relationship is most visible. Kept separate from the
//! `EditorRelationMetadata` adapter (React `fromRelationMetadata.ts`, tasklist
//! T081) because the two carry the same facts in different shapes; sharing one
//! would mean bending one shape into the other for no gain.
//!
//! ## Input types
//!
//! React's input is `TableSchema` from `types/connection.ts`. The adapter uses
//! the single canonical facade definition from `connection_types` (T128) —
//! three names at the crate root, `ColumnSchema` via the `types` aggregate —
//! and reads only the name/type/nullability, key and foreign-key columns from
//! the full shape. There are no local slice twins.
//!
//! ## Output types
//!
//! `PredictionColumn`/`DeclaredForeignKey`/`PredictionTable` are the input
//! half of React `relationPrediction/types.ts` — the slice this adapter
//! populates, landed tasklist T033. The `schema` namespace field landed
//! with the engine (`predict_relations/`, tasklist T082): prediction does
//! not cross schemas, so the engine reads it, while this adapter leaves it
//! unset — the ER diagram feeds table names as ids and the metadata
//! adapter (React `fromRelationMetadata.ts`, still to land) is the caller
//! that knows the namespace. The output half of `types.ts`
//! (`PredictionEvidence*`/`RelationCandidate`/`PredictionOptions`) lives
//! with the engine in `predict_relations/`.

use std::collections::HashSet;

use crate::types::ColumnSchema;
use crate::{ForeignKeyInfo, IndexInfo, TableSchema};

/// One column of a table under consideration (React `types.ts`
/// `PredictionColumn`, the fields this adapter fills).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PredictionColumn {
    pub name: String,
    /// Driver-reported type string, e.g. `integer`, `varchar`.
    pub data_type: String,
    pub nullable: bool,
    /// True when the column is indexed (any index), a weak signal of FK usage.
    pub indexed: bool,
}

/// A declared foreign key, so prediction never restates ground truth (React
/// `types.ts` `DeclaredForeignKey`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeclaredForeignKey {
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
}

/// A table to consider as a source and/or target of a relationship (React
/// `types.ts` `PredictionTable`, the fields this adapter fills plus the
/// engine's schema namespace field).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PredictionTable {
    /// Stable identity used in evidence and output.
    pub id: String,
    /// Bare table name, used for name matching.
    pub name: String,
    /// Schema the table lives in; prediction does not cross schemas.
    /// `None` when the caller does not know (React leaves it `undefined`).
    pub schema: Option<String>,
    pub columns: Vec<PredictionColumn>,
    pub primary_key: Vec<String>,
    /// Every column set covered by a unique index (single-column or
    /// composite). A relationship can only target a key, never an arbitrary
    /// column.
    pub unique_column_sets: Vec<Vec<String>>,
    pub declared_foreign_keys: Vec<DeclaredForeignKey>,
}

/// Every column set a relationship may target: the primary key plus each unique
/// index. Non-unique indexes are not keys and are excluded.
fn unique_column_sets(schema: &TableSchema) -> Vec<Vec<String>> {
    let mut sets = Vec::new();
    if !schema.primary_keys.is_empty() {
        sets.push(schema.primary_keys.clone());
    }
    for index in &schema.indexes {
        if !index.is_unique || index.columns.is_empty() {
            continue;
        }
        sets.push(index.columns.clone());
    }
    sets
}

fn declared_foreign_keys(schema: &TableSchema) -> Vec<DeclaredForeignKey> {
    schema
        .foreign_keys
        .iter()
        .map(|fk| DeclaredForeignKey {
            columns: fk.columns.clone(),
            referenced_table: fk.referenced_table.clone(),
            referenced_columns: fk.referenced_columns.clone(),
        })
        .collect()
}

/// Convert one ER table schema into prediction input.
///
/// `id` is the identity used in the output; the ER diagram keys nodes by table
/// name, so callers normally pass that (and
/// [`to_prediction_tables_from_schemas`] does).
pub fn to_prediction_table_from_schema(id: &str, schema: &TableSchema) -> PredictionTable {
    // Any index (unique or not) marks its columns as indexed. Membership folds
    // case on both sides, exactly like React's `toLowerCase()` pair.
    let mut indexed: HashSet<String> = HashSet::new();
    for index in &schema.indexes {
        for column in &index.columns {
            indexed.insert(column.to_lowercase());
        }
    }
    let columns = schema
        .columns
        .iter()
        .map(|column| PredictionColumn {
            name: column.name.clone(),
            data_type: column.data_type.clone(),
            nullable: column.nullable,
            indexed: indexed.contains(&column.name.to_lowercase()),
        })
        .collect();
    PredictionTable {
        id: id.to_string(),
        name: schema.table_name.clone(),
        // React's adapter leaves the namespace undefined; callers that know
        // it (the metadata adapter) are the ones to fill it in.
        schema: None,
        columns,
        primary_key: schema.primary_keys.clone(),
        unique_column_sets: unique_column_sets(schema),
        declared_foreign_keys: declared_foreign_keys(schema),
    }
}

/// Convert a whole ER schema into prediction input, keyed by table name.
pub fn to_prediction_tables_from_schemas(schemas: &[TableSchema]) -> Vec<PredictionTable> {
    schemas
        .iter()
        .map(|schema| to_prediction_table_from_schema(&schema.table_name, schema))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relation_prediction::predict_relations::{
        PredictionOptions, PredictionTier, predict_relations,
    };

    // The React suite (`relationPrediction/__tests__/fromTableSchema.test.ts`)
    // is carried case by case, including the end-to-end case that calls
    // `predictRelations` over the converted schema pair — the engine
    // (predict_relations/, tasklist T082) now in reach of this adapter's
    // output.

    /// The suite's `schema(over)` helper: one `id` column, itself the primary
    /// key, no indexes, no foreign keys.
    fn schema(table_name: &str) -> TableSchema {
        TableSchema {
            table_name: table_name.to_string(),
            columns: vec![column("id", "integer", false)],
            primary_keys: vec!["id".to_string()],
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        }
    }

    fn column(name: &str, data_type: &str, nullable: bool) -> ColumnSchema {
        ColumnSchema {
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable,
            ..Default::default()
        }
    }

    fn index(columns: &[&str], is_unique: bool) -> IndexInfo {
        IndexInfo {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            is_unique,
            ..Default::default()
        }
    }

    fn foreign_key(
        columns: &[&str],
        referenced_table: &str,
        referenced_columns: &[&str],
    ) -> ForeignKeyInfo {
        ForeignKeyInfo {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            referenced_table: referenced_table.to_string(),
            referenced_columns: referenced_columns.iter().map(|c| c.to_string()).collect(),
            ..Default::default()
        }
    }

    fn pred_column(name: &str, data_type: &str, nullable: bool, indexed: bool) -> PredictionColumn {
        PredictionColumn {
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable,
            indexed,
        }
    }

    fn declared_fk(
        columns: &[&str],
        referenced_table: &str,
        referenced_columns: &[&str],
    ) -> DeclaredForeignKey {
        DeclaredForeignKey {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            referenced_table: referenced_table.to_string(),
            referenced_columns: referenced_columns.iter().map(|c| c.to_string()).collect(),
        }
    }

    #[test]
    fn keys_the_table_by_the_caller_id_and_keeps_the_bare_name() {
        let table = to_prediction_table_from_schema("public.users", &schema("users"));
        assert_eq!(table.id, "public.users");
        assert_eq!(table.name, "users");
    }

    #[test]
    fn collects_the_primary_key_and_unique_indexes_as_targetable_key_sets() {
        let mut source = schema("orders");
        source.indexes = vec![
            index(&["tenant_id", "code"], true),
            index(&["total"], false),
        ];
        let table = to_prediction_table_from_schema("orders", &source);
        // A non-unique index is not a key and must not become a target.
        assert_eq!(
            table.unique_column_sets,
            vec![vec!["id"], vec!["tenant_id", "code"]]
        );
    }

    #[test]
    fn marks_only_indexed_columns_as_indexed() {
        let mut source = schema("orders");
        source.columns = vec![column("id", "integer", false), column("user_id", "integer", false)];
        source.indexes = vec![index(&["user_id"], false)];
        let table = to_prediction_table_from_schema("orders", &source);
        assert_eq!(
            table.columns,
            vec![
                pred_column("id", "integer", false, false),
                pred_column("user_id", "integer", false, true),
            ]
        );
    }

    #[test]
    fn index_membership_folds_case_on_both_sides() {
        // React lowercases the index column and the schema column before the
        // membership check, so an uppercase index spelling still marks the
        // column (and a casing mismatch cannot hide the signal).
        let mut source = schema("orders");
        source.columns = vec![column("user_id", "integer", false)];
        source.indexes = vec![index(&["USER_ID"], false)];
        let table = to_prediction_table_from_schema("orders", &source);
        assert_eq!(
            table.columns,
            vec![pred_column("user_id", "integer", false, true)]
        );
    }

    #[test]
    fn carries_declared_foreign_keys_so_ground_truth_is_not_re_predicted() {
        let mut source = schema("orders");
        source.foreign_keys = vec![foreign_key(&["user_id"], "users", &["id"])];
        let table = to_prediction_table_from_schema("orders", &source);
        assert_eq!(
            table.declared_foreign_keys,
            vec![declared_fk(&["user_id"], "users", &["id"])]
        );
    }

    #[test]
    fn does_not_alias_the_schema_arrays_into_the_prediction_input() {
        // React asserts `table.columns` is not the same array as
        // `source.columns` (and likewise for the primary key). The Rust
        // stand-in for that referential check: mutating the source after the
        // conversion must not reach the converted table, because every
        // collection is owned/cloned rather than shared.
        let mut source = schema("orders");
        let table = to_prediction_table_from_schema("orders", &source);
        source.columns.push(column("extra", "text", true));
        source.primary_keys.push("extra".to_string());
        source.indexes.push(index(&["extra"], true));
        source.foreign_keys.push(foreign_key(&["extra"], "users", &["id"]));
        source.table_name = "renamed".to_string();
        assert_eq!(table.columns, vec![pred_column("id", "integer", false, false)]);
        assert_eq!(table.primary_key, vec!["id"]);
        assert_eq!(table.unique_column_sets, vec![vec!["id"]]);
        assert!(table.declared_foreign_keys.is_empty());
        assert_eq!(table.name, "orders");
    }

    #[test]
    fn prepares_the_er_schema_pair_the_engine_infers_from() {
        // No constraint anywhere: only the naming convention links the two,
        // which is exactly the case the ER diagram has to draw. React feeds
        // this pair straight into predictRelations and finds `orders →
        // users`; the conversion half is pinned here — the tables enter with
        // the ids, names and columns the convention check reads — and the
        // inference half rides along now that the engine has landed.
        let mut users = schema("users");
        users.columns = vec![column("id", "integer", false)];
        let mut orders = schema("orders");
        orders.columns = vec![
            column("id", "integer", false),
            column("user_id", "integer", false),
        ];
        let tables = to_prediction_tables_from_schemas(&[users, orders]);
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].name, "users");
        assert_eq!(
            tables[1].columns,
            vec![
                pred_column("id", "integer", false, false),
                pred_column("user_id", "integer", false, false),
            ]
        );
        assert_eq!(tables[1].unique_column_sets, vec![vec!["id"]]);
        assert!(tables.iter().all(|t| t.declared_foreign_keys.is_empty()));

        let found = predict_relations(&tables, &PredictionOptions::default());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].from_table, "orders");
        assert_eq!(found[0].to_table, "users");
        assert_eq!(found[0].tier, PredictionTier::High);
    }

    #[test]
    fn keys_every_table_by_name_when_converting_a_whole_schema() {
        let tables = to_prediction_tables_from_schemas(&[schema("users"), schema("orders")]);
        assert_eq!(
            tables.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["users", "orders"]
        );
    }

    #[test]
    fn an_empty_schema_list_yields_no_tables() {
        assert!(to_prediction_tables_from_schemas(&[]).is_empty());
    }

    #[test]
    fn a_default_schema_converts_to_an_empty_but_well_formed_table() {
        // All-empty input stays all-empty output — in particular an empty
        // primary key is not pushed as a (zero-column) key set, and the
        // schema namespace stays unknown.
        let table = to_prediction_table_from_schema("t", &TableSchema::default());
        assert_eq!(table.id, "t");
        assert_eq!(table.name, "");
        assert_eq!(table.schema, None);
        assert!(table.columns.is_empty());
        assert!(table.primary_key.is_empty());
        assert!(table.unique_column_sets.is_empty());
        assert!(table.declared_foreign_keys.is_empty());
    }

    #[test]
    fn a_unique_index_with_no_columns_is_not_a_key() {
        let mut source = schema("orders");
        source.indexes = vec![IndexInfo {
            columns: Vec::new(),
            is_unique: true,
            ..Default::default()
        }];
        let table = to_prediction_table_from_schema("orders", &source);
        assert_eq!(table.unique_column_sets, vec![vec!["id"]]);
    }

    #[test]
    fn composite_keys_and_foreign_keys_keep_their_column_order() {
        let mut source = schema("order_lines");
        source.primary_keys = vec!["order_id".to_string(), "line_no".to_string()];
        source.indexes = vec![index(&["tenant_id", "code", "seq"], true)];
        source.foreign_keys = vec![foreign_key(
            &["order_id", "line_no"],
            "orders",
            &["id", "line"],
        )];
        let table = to_prediction_table_from_schema("order_lines", &source);
        assert_eq!(
            table.unique_column_sets,
            vec![
                vec!["order_id", "line_no"],
                vec!["tenant_id", "code", "seq"],
            ]
        );
        assert_eq!(table.primary_key, vec!["order_id", "line_no"]);
        assert_eq!(
            table.declared_foreign_keys,
            vec![declared_fk(&["order_id", "line_no"], "orders", &["id", "line"])]
        );
    }

    #[test]
    fn a_self_referential_foreign_key_is_carried_verbatim() {
        // `employees.manager_id → employees.id`: the adapter has no
        // self-reference filter of its own — the engine decides what a
        // self-reference means, and ground truth rides along untouched.
        let mut source = schema("employees");
        source.columns =
            vec![column("id", "integer", false), column("manager_id", "integer", true)];
        source.foreign_keys = vec![foreign_key(&["manager_id"], "employees", &["id"])];
        let table = to_prediction_table_from_schema("employees", &source);
        assert_eq!(
            table.declared_foreign_keys,
            vec![declared_fk(&["manager_id"], "employees", &["id"])]
        );
    }

    #[test]
    fn nullable_and_type_flags_ride_along_untouched() {
        // The adapter must not editorialize the driver-reported type spelling
        // or nullability — folding is the engine's job (normalize.rs), and it
        // needs the raw spelling for the exact-type bonus.
        let mut source = schema("mixed");
        source.columns = vec![
            column("payload", "NUMERIC(10,2)", false),
            column("note", "varchar(32)", true),
        ];
        let table = to_prediction_table_from_schema("mixed", &source);
        assert_eq!(
            table.columns,
            vec![
                pred_column("payload", "NUMERIC(10,2)", false, false),
                pred_column("note", "varchar(32)", true, false),
            ]
        );
    }
}
