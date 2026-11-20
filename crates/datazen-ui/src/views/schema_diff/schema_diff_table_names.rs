//! Port of `packages/frontend/src/windows/schema-diff/schemaDiffTableNames.ts`
//! — the table inventory merge and the enabled-set selectors (T372, the
//! shard's historical T374). The merge subset first landed as T351's seam
//! for the discovery run; this is the full file:
//!
//! - `tableDiffHasChanges` → [`table_diff_has_changes`]
//! - `qualifySchemaDiffTableName` → [`qualify_schema_diff_table_name`]
//! - `filterTablesForSchema` → [`filter_tables_for_schema`]
//! - `SchemaDiffTablePick` → [`SchemaDiffTablePickVm`]
//! - `enabledTableNames` → [`enabled_table_names`]
//! - `enabledSourceTableNames` → [`enabled_source_table_names`]
//! - `enabledTargetOnlyTableNames` → [`enabled_target_only_table_names`]
//! - `mergeSchemaDiffTablePicks` → [`merge_schema_diff_table_picks`]
//!
//! - `SchemaDiffTablePick` → [`SchemaDiffTablePickVm`]
//! - `filterTablesForSchema` → [`filter_tables_for_schema`]
//! - `qualifySchemaDiffTableName` → [`qualify_schema_diff_table_name`]
//! - `mergeSchemaDiffTablePicks` → [`merge_schema_diff_table_picks`]
//!
//! The merge rules (behavior, not implementation detail):
//!
//! - The two endpoint inventories merge using the relation name in the
//!   selected schema as the logical identity; each endpoint retains its own
//!   exact qualified selector for IPC, which matters when the dialects use
//!   different schema qualification rules.
//! - `listTables` includes empty-name sentinels for schemas with no
//!   relations — they stay available to schema discovery but are never
//!   presented as tables.
//! - A pick is `enabled` unless it is target-only (React's
//!   `origin !== 'target-only'`); the display name is the source selector,
//!   else the target selector, else the bare relation.
//! - Picks sort by `name.localeCompare` — approximated by lowercased byte
//!   order (the backup-grouping `compareStrings` convention), with JS's
//!   stable sort (`sort_by` is stable too).
//!
//! `TableInfo` / `TableType` (`types/connection.ts`) ride along as the
//! `*Vm` wire projections (the host converts; `datazen-ui` does not depend
//! on the client crate).

// The production consumer is the discovery run (T351) and the schema-diff
// wizard track; until the window state lands, the inline tests are the
// only callers, so the dead-code lint is silenced here rather than in a
// crate-wide allow.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

/// React `TableType` (`types/connection.ts:98`) — the relation kinds the
/// catalog lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableTypeVm {
    Table,
    View,
    MaterializedView,
    SystemTable,
}

impl TableTypeVm {
    /// The wire spelling (`relation.kind`).
    pub(crate) fn wire_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::MaterializedView => "materializedView",
            Self::SystemTable => "systemTable",
        }
    }
}

/// React `TableInfo` (`types/connection.ts:113`) — one `listTables` row.
/// `row_count` is part of the wire shape; the pairing never reads it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TableInfoVm {
    pub(crate) name: String,
    /// `schema?: string | null` — null on the wire for schema-less engines.
    pub(crate) schema: Option<String>,
    pub(crate) table_type: TableTypeVm,
    pub(crate) row_count: Option<u64>,
}

/// React `SchemaDiffTablePick['origin']` — the stable UI identity for
/// source/target metadata rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TablePickOriginVm {
    Both,
    SourceOnly,
    TargetOnly,
}

impl TablePickOriginVm {
    /// The React literal spellings.
    pub(crate) fn wire_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::SourceOnly => "source-only",
            Self::TargetOnly => "target-only",
        }
    }
}

/// React `SchemaDiffTablePick` (`schemaDiffTableNames.ts:41`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SchemaDiffTablePickVm {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) origin: TablePickOriginVm,
    /// The exact selector used when reading the source endpoint.
    pub(crate) source_name: Option<String>,
    /// The exact selector used when reading the target endpoint.
    pub(crate) target_name: Option<String>,
}

/// `table.schema?.trim()` — trimmed, and the falsy results (`null`,
/// `undefined`, `''`) fold to `None`.
fn trimmed_schema(schema: &Option<String>) -> Option<&str> {
    schema
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// React `filterTablesForSchema` (`schemaDiffTableNames.ts:28`): only real
/// tables of the active schema reach the pairing. `active_schema` keeps
/// React's `string | undefined` truthiness — `''` is no filter at all, and
/// a relation without a (non-empty) schema passes every filter.
pub(crate) fn filter_tables_for_schema<'a>(
    tables: &'a [TableInfoVm],
    active_schema: &str,
) -> Vec<&'a TableInfoVm> {
    tables
        .iter()
        .filter(|table| {
            // listTables includes empty-name sentinels for schemas with no
            // relations. Keep those available to schema discovery, but never
            // present them as tables.
            if table.table_type != TableTypeVm::Table || table.name.is_empty() {
                return false;
            }
            match trimmed_schema(&table.schema) {
                // JS `if (activeSchema && schema)` needs both truthy; a
                // schema-less relation is kept under any active schema.
                None => true,
                Some(schema) if !active_schema.is_empty() => schema == active_schema,
                Some(_) => true,
            }
        })
        .collect()
}

/// React `qualifySchemaDiffTableName` (`schemaDiffTableNames.ts:18`): the
/// table identifier passed to Schema Diff IPC — schema-qualified when the
/// schema is set and is (or there is no) active schema.
pub(crate) fn qualify_schema_diff_table_name(
    table: &TableInfoVm,
    active_schema: &str,
) -> String {
    if let Some(schema) = trimmed_schema(&table.schema) {
        if active_schema.is_empty() || schema == active_schema {
            return format!("{schema}.{}", table.name);
        }
    }
    table.name.clone()
}

/// React `mergeSchemaDiffTablePicks` (`schemaDiffTableNames.ts:74`): merge
/// the two endpoint inventories using the relation name in the selected
/// schema as the logical identity (see the module docs for the rules).
pub(crate) fn merge_schema_diff_table_picks(
    source_tables: &[TableInfoVm],
    target_tables: &[TableInfoVm],
    source_schema: &str,
    target_schema: &str,
) -> Vec<SchemaDiffTablePickVm> {
    let source = filter_tables_for_schema(source_tables, source_schema);
    let target = filter_tables_for_schema(target_tables, target_schema);
    // Relation-name keyed maps; a duplicate name keeps its first insertion
    // position and its last value (JS Map.set), which only matters for the
    // stable sort's tie-break.
    let mut seen: HashSet<&str> = HashSet::new();
    let mut relations: Vec<&str> = Vec::new();
    let mut source_by_relation: HashMap<&str, &TableInfoVm> = HashMap::new();
    for table in &source {
        source_by_relation.insert(table.name.as_str(), table);
        if seen.insert(table.name.as_str()) {
            relations.push(table.name.as_str());
        }
    }
    let mut target_by_relation: HashMap<&str, &TableInfoVm> = HashMap::new();
    for table in &target {
        target_by_relation.insert(table.name.as_str(), table);
        if seen.insert(table.name.as_str()) {
            relations.push(table.name.as_str());
        }
    }
    let mut picks: Vec<SchemaDiffTablePickVm> = relations
        .into_iter()
        .map(|relation| {
            let source_table = source_by_relation.get(relation).copied();
            let target_table = target_by_relation.get(relation).copied();
            let source_name =
                source_table.map(|table| qualify_schema_diff_table_name(table, source_schema));
            let target_name =
                target_table.map(|table| qualify_schema_diff_table_name(table, target_schema));
            let origin = match (source_table, target_table) {
                (Some(_), Some(_)) => TablePickOriginVm::Both,
                (Some(_), None) => TablePickOriginVm::SourceOnly,
                (None, _) => TablePickOriginVm::TargetOnly,
            };
            SchemaDiffTablePickVm {
                name: source_name
                    .clone()
                    .or(target_name.clone())
                    .unwrap_or_else(|| relation.to_owned()),
                enabled: origin != TablePickOriginVm::TargetOnly,
                origin,
                source_name,
                target_name,
            }
        })
        .collect();
    // `name.localeCompare` ≈ lowercased byte order (the backup-grouping
    // convention); JS sort is stable and so is `sort_by`.
    picks.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    picks
}

#[cfg(test)]
/// React `tableDiffHasChanges`: a target-only table always counts as a
/// change; otherwise any missing/extra/changed column, any check
/// constraint delta or a table-options delta does. `missingOnTarget ??
/// added` / `extraOnTarget ?? removed` prefer the explicit fields over
/// the legacy aliases.
pub(crate) fn table_diff_has_changes(diff: &crate::components::schema::schema_diff_panel::TableSchemaDiffVm) -> bool {
    if diff.target_only {
        return true;
    }
    let missing = diff.missing_on_target.as_deref().unwrap_or(&diff.added);
    let extra = diff.extra_on_target.as_deref().unwrap_or(&diff.removed);
    !missing.is_empty()
        || !extra.is_empty()
        || !diff.changed.is_empty()
        || diff.missing_check_constraints.as_ref().map_or(false, |c| !c.is_empty())
        || diff.extra_check_constraints.as_ref().map_or(false, |c| !c.is_empty())
        || diff.table_options.is_some()
}

/// React `enabledTableNames`: the enabled picks' bare names.
pub(crate) fn enabled_table_names(picks: &[SchemaDiffTablePickVm]) -> Vec<String> {
    picks.iter().filter(|row| row.enabled).map(|row| row.name.clone()).collect()
}

/// React `enabledSourceTableNames`: enabled non-target-only picks, by
/// their exact source selector (`sourceName ?? name`).
pub(crate) fn enabled_source_table_names(picks: &[SchemaDiffTablePickVm]) -> Vec<String> {
    picks
        .iter()
        .filter(|row| row.enabled)
        .map(|row| row.source_name.clone().unwrap_or_else(|| row.name.clone()))
        .collect()
}

/// React `enabledTargetOnlyTableNames`: enabled target-only picks, by
/// their exact target selector (`targetName ?? name`).
pub(crate) fn enabled_target_only_table_names(picks: &[SchemaDiffTablePickVm]) -> Vec<String> {
    picks
        .iter()
        .filter(|row| row.enabled && row.origin == TablePickOriginVm::TargetOnly)
        .map(|row| row.target_name.clone().unwrap_or_else(|| row.name.clone()))
        .collect()
}

pub(crate) mod tests {
    use super::*;

    fn table(schema: Option<&str>, name: &str) -> TableInfoVm {
        TableInfoVm {
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            table_type: TableTypeVm::Table,
            row_count: None,
        }
    }

    fn relation(kind: TableTypeVm, schema: Option<&str>, name: &str) -> TableInfoVm {
        TableInfoVm { table_type: kind, ..table(schema, name) }
    }

    /// The schema filter keeps only real tables, drops the empty-name
    /// sentinels, matches the active schema when one is set, and keeps
    /// schema-less relations under any filter.
    #[test]
    fn schema_filter_keeps_tables_of_the_active_schema() {
        let tables = vec![
            table(Some("public"), "orders"),
            relation(TableTypeVm::MaterializedView, Some("public"), "mv"),
            relation(TableTypeVm::SystemTable, Some("public"), "sys"),
            table(Some("sales"), "deals"),
            table(None, "global"),
            relation(TableTypeVm::Table, Some("public"), ""),
        ];
        let names = |active: &str| {
            filter_tables_for_schema(&tables, active)
                .iter()
                .map(|table| table.name.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(names("public"), vec!["orders", "global"]);
        assert_eq!(names("sales"), vec!["deals", "global"]);
        // '' is no filter at all (JS truthiness).
        assert_eq!(names(""), vec!["orders", "deals", "global"]);
    }

    /// Qualification: schema-qualified when the schema is set and matches
    /// (or there is no active schema); bare otherwise; whitespace trims.
    #[test]
    fn qualification_follows_the_active_schema() {
        assert_eq!(
            qualify_schema_diff_table_name(&table(Some("public"), "orders"), "public"),
            "public.orders"
        );
        assert_eq!(
            qualify_schema_diff_table_name(&table(Some("public"), "orders"), ""),
            "public.orders"
        );
        assert_eq!(
            qualify_schema_diff_table_name(&table(Some("sales"), "deals"), "public"),
            "deals"
        );
        assert_eq!(qualify_schema_diff_table_name(&table(None, "orders"), "public"), "orders");
        assert_eq!(
            qualify_schema_diff_table_name(&table(Some("  public  "), "orders"), "public"),
            "public.orders"
        );
        // A whitespace-only schema is falsy after trim: no qualification.
        assert_eq!(qualify_schema_diff_table_name(&table(Some("   "), "orders"), ""), "orders");
    }

    /// The left/right pairing: both / source-only / target-only origins,
    /// the enabled fold, the name fallback order, and each side keeping its
    /// own exact selector.
    #[test]
    fn pairing_rules_merge_both_sides() {
        let picks = merge_schema_diff_table_picks(
            &[table(Some("public"), "orders"), table(Some("public"), "users")],
            &[table(None, "orders"), table(Some("reports"), "report_lines")],
            "public",
            "",
        );
        assert_eq!(
            picks,
            vec![
                // Both sides: the source-qualified name is the pick name,
                // the target keeps its exact (bare, schema-less) selector.
                SchemaDiffTablePickVm {
                    name: "public.orders".to_owned(),
                    enabled: true,
                    origin: TablePickOriginVm::Both,
                    source_name: Some("public.orders".to_owned()),
                    target_name: Some("orders".to_owned()),
                },
                SchemaDiffTablePickVm {
                    name: "public.users".to_owned(),
                    enabled: true,
                    origin: TablePickOriginVm::SourceOnly,
                    source_name: Some("public.users".to_owned()),
                    target_name: None,
                },
                // Target-only: disabled, qualified with its own schema
                // because the target has no active schema.
                SchemaDiffTablePickVm {
                    name: "reports.report_lines".to_owned(),
                    enabled: false,
                    origin: TablePickOriginVm::TargetOnly,
                    source_name: None,
                    target_name: Some("reports.report_lines".to_owned()),
                },
            ]
        );
    }

    /// The pick sort is locale-like (case-insensitive) and stable, and a
    /// duplicate relation name within one side keeps the last row's
    /// selector under the first occurrence's position.
    #[test]
    fn picks_sort_locale_like_and_duplicates_keep_the_last_row() {
        let source = vec![
            table(Some("s"), "Zebra"),
            table(None, "apple"),
            table(Some("a"), "dup"),
            table(Some("b"), "dup"),
        ];
        let target = vec![table(None, "BAZ")];
        let picks = merge_schema_diff_table_picks(&source, &target, "", "");
        let names: Vec<&str> = picks.iter().map(|pick| pick.name.as_str()).collect();
        assert_eq!(names, vec!["apple", "b.dup", "BAZ", "s.Zebra"]);
        // 'dup' resolved from the last source row (schema b).
        let dup = &picks[1];
        assert_eq!(dup.source_name.as_deref(), Some("b.dup"));
        assert_eq!(dup.origin, TablePickOriginVm::SourceOnly);
        assert!(dup.enabled);

        // A stable tie: two different relations whose picks share a name
        // keep the source-first insertion order.
        let picks =
            merge_schema_diff_table_picks(&[table(Some("s"), "dup")], &[table(None, "s.dup")], "", "");
        assert_eq!(picks[0].origin, TablePickOriginVm::SourceOnly);
        assert_eq!(picks[1].origin, TablePickOriginVm::TargetOnly);
        assert_eq!(picks[0].name, "s.dup");
        assert_eq!(picks[1].name, "s.dup");
    }

    // ---- T372: the change predicate and the enabled-set selectors -----

    use crate::components::schema::schema_diff_panel::{
        CheckConstraintDiffVm, ColumnDiffVm, TableOptionsDiffVm, TableSchemaDiffVm,
    };

    fn diff() -> TableSchemaDiffVm {
        TableSchemaDiffVm {
            table: "users".to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn has_changes_detects_every_delta_kind() {
        // Clean diff: no changes.
        assert!(!table_diff_has_changes(&diff()));
        // Target-only: always a change.
        let mut target_only = diff();
        target_only.target_only = true;
        assert!(table_diff_has_changes(&target_only));
        // Missing columns (either field).
        let mut missing = diff();
        missing.added = vec![ColumnDiffVm::new("email", "text", true, false)];
        assert!(table_diff_has_changes(&missing));
        let mut missing_explicit = diff();
        missing_explicit.missing_on_target = Some(vec![ColumnDiffVm::new("email", "text", true, false)]);
        assert!(table_diff_has_changes(&missing_explicit));
        // Extra columns through the legacy alias alone.
        let mut extra = diff();
        extra.removed = vec![ColumnDiffVm::new("old", "text", true, false)];
        assert!(table_diff_has_changes(&extra));
        // Changed columns.
        let mut changed = diff();
        changed.changed = vec![crate::components::schema::schema_diff_panel::ChangedColumnDiffVm {
            name: "email".to_owned(),
            ..Default::default()
        }];
        assert!(table_diff_has_changes(&changed));
        // Check constraints (either side).
        let mut checks = diff();
        checks.missing_check_constraints = Some(vec![CheckConstraintDiffVm {
            name: "c".to_owned(),
            expression: "x > 0".to_owned(),
        }]);
        assert!(table_diff_has_changes(&checks));
        let mut extra_checks = diff();
        extra_checks.extra_check_constraints = Some(Vec::new());
        assert!(!table_diff_has_changes(&extra_checks), "empty list is no change");
        // Table options presence.
        let mut options = diff();
        options.table_options = Some(TableOptionsDiffVm::default());
        assert!(table_diff_has_changes(&options));
    }

    #[test]
    fn enabled_selectors_split_by_origin() {
        let picks = vec![
            SchemaDiffTablePickVm {
                name: "alpha".to_owned(),
                enabled: true,
                origin: TablePickOriginVm::Both,
                source_name: Some("s.alpha".to_owned()),
                target_name: Some("t.alpha".to_owned()),
            },
            SchemaDiffTablePickVm {
                name: "beta".to_owned(),
                enabled: false,
                origin: TablePickOriginVm::Both,
                source_name: Some("s.beta".to_owned()),
                target_name: Some("t.beta".to_owned()),
            },
            SchemaDiffTablePickVm {
                name: "gamma".to_owned(),
                enabled: true,
                origin: TablePickOriginVm::SourceOnly,
                source_name: Some("s.gamma".to_owned()),
                target_name: None,
            },
            SchemaDiffTablePickVm {
                name: "archive".to_owned(),
                enabled: true,
                origin: TablePickOriginVm::TargetOnly,
                source_name: None,
                target_name: Some("t.archive".to_owned()),
            },
        ];
        assert_eq!(enabled_table_names(&picks), ["alpha", "gamma", "archive"]);
        // Source selectors exclude target-only rows; disabled rows drop.
        assert_eq!(enabled_source_table_names(&picks), ["s.alpha", "s.gamma"]);
        // Target-only selectors fall back to the bare name when the
        // selector is absent.
        assert_eq!(enabled_target_only_table_names(&picks), ["t.archive"]);
        let bare = vec![SchemaDiffTablePickVm {
            name: "bare".to_owned(),
            enabled: true,
            origin: TablePickOriginVm::TargetOnly,
            source_name: None,
            target_name: None,
        }];
        assert_eq!(enabled_target_only_table_names(&bare), ["bare"]);
    }
}
