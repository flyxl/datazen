//! Column (and index) comparison with source = desired.

use super::types::{
    ChangedColumnDiff, CheckConstraintSnapshot, ColumnChange, ColumnSnapshot, TableColumnDiff,
    TableOptionChange, TableOptionsDiff,
};
use datazen_driver_api::sync::{IRColumn, IRTable, IRType};
use datazen_driver_api::TypeNormalizer;
use datazen_driver_api::{CheckConstraint, ColumnSchema, IndexInfo, TableSchema};
use std::collections::HashMap;

pub fn column_snapshot(col: &ColumnSchema) -> ColumnSnapshot {
    ColumnSnapshot {
        name: col.name.clone(),
        data_type: col.data_type.clone(),
        nullable: col.nullable,
        default_value: col.default_value.clone(),
        comment: col.comment.clone(),
        is_primary_key: col.is_primary_key,
        is_auto_increment: col.is_auto_increment,
    }
}

/// Diff table schemas. **Source is the desired state.**
pub fn diff_table_schemas(
    table: &str,
    src: &TableSchema,
    tgt: &TableSchema,
    normalizer: Option<&dyn TypeNormalizer>,
) -> TableColumnDiff {
    let src_map: HashMap<&str, &ColumnSchema> =
        src.columns.iter().map(|c| (c.name.as_str(), c)).collect();
    let tgt_map: HashMap<&str, &ColumnSchema> =
        tgt.columns.iter().map(|c| (c.name.as_str(), c)).collect();

    let mut missing_on_target = Vec::new();
    let mut extra_on_target = Vec::new();
    let mut changed = Vec::new();

    for col in &src.columns {
        if !tgt_map.contains_key(col.name.as_str()) {
            missing_on_target.push(column_snapshot(col));
        }
    }

    for col in &tgt.columns {
        if !src_map.contains_key(col.name.as_str()) {
            extra_on_target.push(column_snapshot(col));
        }
    }

    for col in &src.columns {
        if let Some(tgt_col) = tgt_map.get(col.name.as_str()) {
            let mut changes = Vec::new();
            let types_equal = match normalizer {
                Some(n) => n.normalize_type(&col.data_type) == n.normalize_type(&tgt_col.data_type),
                None => col.data_type == tgt_col.data_type,
            };
            if !types_equal {
                changes.push(super::types::ColumnChange::DataType);
            }
            if col.nullable != tgt_col.nullable {
                changes.push(super::types::ColumnChange::Nullable);
            }
            if col.is_primary_key != tgt_col.is_primary_key {
                changes.push(super::types::ColumnChange::PrimaryKey);
            }
            if col.default_value != tgt_col.default_value {
                changes.push(super::types::ColumnChange::Default);
            }
            if col.comment != tgt_col.comment {
                changes.push(super::types::ColumnChange::Comment);
            }
            if col.is_auto_increment != tgt_col.is_auto_increment {
                changes.push(super::types::ColumnChange::AutoIncrement);
            }
            if !changes.is_empty() {
                changed.push(ChangedColumnDiff {
                    name: col.name.clone(),
                    source: column_snapshot(col),
                    target: column_snapshot(tgt_col),
                    changes,
                });
            }
        }
    }

    let (missing_check_constraints, extra_check_constraints) =
        diff_check_constraints(&src.check_constraints, &tgt.check_constraints);

    let mut table_option_changes = Vec::new();
    if src.table_options.comment != tgt.table_options.comment {
        table_option_changes.push(TableOptionChange::Comment);
    }
    if src.table_options.engine != tgt.table_options.engine {
        table_option_changes.push(TableOptionChange::Engine);
    }
    if src.table_options.charset != tgt.table_options.charset {
        table_option_changes.push(TableOptionChange::Charset);
    }
    let table_options = (!table_option_changes.is_empty()).then(|| TableOptionsDiff {
        source: src.table_options.clone(),
        target: tgt.table_options.clone(),
        changes: table_option_changes,
    });

    TableColumnDiff {
        table: table.to_string(),
        added: missing_on_target.clone(),
        removed: extra_on_target.clone(),
        missing_on_target,
        extra_on_target,
        changed,
        missing_check_constraints,
        extra_check_constraints,
        table_options,
    }
}

pub fn diff_check_constraints(
    source: &[CheckConstraint],
    target: &[CheckConstraint],
) -> (Vec<CheckConstraintSnapshot>, Vec<CheckConstraintSnapshot>) {
    let source_by_name = source
        .iter()
        .map(|constraint| (constraint.name.as_str(), constraint))
        .collect::<HashMap<_, _>>();
    let target_by_name = target
        .iter()
        .map(|constraint| (constraint.name.as_str(), constraint))
        .collect::<HashMap<_, _>>();
    let mut missing = Vec::new();
    let mut extra = Vec::new();

    for (name, source_constraint) in &source_by_name {
        match target_by_name.get(name) {
            None => missing.push(CheckConstraintSnapshot {
                name: source_constraint.name.clone(),
                expression: source_constraint.expression.clone(),
            }),
            Some(target_constraint)
                if source_constraint.expression.trim() != target_constraint.expression.trim() =>
            {
                extra.push(CheckConstraintSnapshot {
                    name: target_constraint.name.clone(),
                    expression: target_constraint.expression.clone(),
                });
                missing.push(CheckConstraintSnapshot {
                    name: source_constraint.name.clone(),
                    expression: source_constraint.expression.clone(),
                });
            }
            Some(_) => {}
        }
    }
    for (name, target_constraint) in &target_by_name {
        if !source_by_name.contains_key(name) {
            extra.push(CheckConstraintSnapshot {
                name: target_constraint.name.clone(),
                expression: target_constraint.expression.clone(),
            });
        }
    }
    missing.sort_by(|left, right| left.name.cmp(&right.name));
    extra.sort_by(|left, right| left.name.cmp(&right.name));
    (missing, extra)
}

#[derive(Debug, Clone)]
pub struct IndexDiff {
    pub missing_on_target: Vec<IndexInfo>,
    pub extra_on_target: Vec<IndexInfo>,
}

fn index_definition_equal(a: &IndexInfo, b: &IndexInfo) -> bool {
    a.is_unique == b.is_unique
        && a.index_type.eq_ignore_ascii_case(&b.index_type)
        && a.columns == b.columns
}

pub fn diff_indexes(src: &TableSchema, tgt: &TableSchema) -> IndexDiff {
    let src_by_name: HashMap<&str, &IndexInfo> =
        src.indexes.iter().map(|i| (i.name.as_str(), i)).collect();
    let tgt_by_name: HashMap<&str, &IndexInfo> =
        tgt.indexes.iter().map(|i| (i.name.as_str(), i)).collect();

    let mut missing_on_target = Vec::new();
    let mut extra_on_target = Vec::new();

    for (name, src_idx) in &src_by_name {
        if src_idx.is_primary {
            continue;
        }
        match tgt_by_name.get(name) {
            None => missing_on_target.push((*src_idx).clone()),
            Some(tgt_idx) if tgt_idx.is_primary => {}
            Some(tgt_idx) if !index_definition_equal(src_idx, tgt_idx) => {
                extra_on_target.push((*tgt_idx).clone());
                missing_on_target.push((*src_idx).clone());
            }
            Some(_) => {}
        }
    }

    for (name, tgt_idx) in &tgt_by_name {
        if tgt_idx.is_primary {
            continue;
        }
        if !src_by_name.contains_key(name) {
            extra_on_target.push((*tgt_idx).clone());
        }
    }

    IndexDiff {
        missing_on_target,
        extra_on_target,
    }
}

// ---------------------------------------------------------------- IR 对比

/// Stable display string for IR types in schema-diff snapshots.
pub fn format_ir_type(t: &IRType) -> String {
    match t {
        IRType::Bool => "Bool".into(),
        IRType::Int8 => "Int8".into(),
        IRType::Int16 => "Int16".into(),
        IRType::Int32 => "Int32".into(),
        IRType::Int64 => "Int64".into(),
        IRType::Float32 => "Float32".into(),
        IRType::Float64 => "Float64".into(),
        IRType::Decimal { precision, scale } => format!("Decimal({precision},{scale})"),
        IRType::Char { length } => format!("Char({length})"),
        IRType::Varchar { length } => format!("Varchar({length:?})"),
        IRType::Text => "Text".into(),
        IRType::Binary { length } => format!("Binary({length:?})"),
        IRType::Blob => "Blob".into(),
        IRType::Date => "Date".into(),
        IRType::Time { with_timezone } => format!("Time(tz={with_timezone})"),
        IRType::Timestamp { with_timezone } => format!("Timestamp(tz={with_timezone})"),
        IRType::Json => "Json".into(),
        IRType::Uuid => "Uuid".into(),
        IRType::Bit { length } => format!("Bit({length})"),
        IRType::Other(s) => format!("Other({s})"),
    }
}

fn ir_column_snapshot(col: &IRColumn) -> ColumnSnapshot {
    ColumnSnapshot {
        name: col.name.clone(),
        data_type: format_ir_type(&col.ir_type),
        nullable: col.nullable,
        default_value: None,
        comment: None,
        is_primary_key: col.is_primary_key,
        is_auto_increment: false,
    }
}

/// Compare schemas using IR types so dialect aliases that map to the same [`IRType`] are not reported as dataType changes.
pub fn diff_table_schemas_ir(table: &str, src_ir: &IRTable, tgt_ir: &IRTable) -> TableColumnDiff {
    let src_map: HashMap<&str, &IRColumn> = src_ir
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let tgt_map: HashMap<&str, &IRColumn> = tgt_ir
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let mut missing_on_target = Vec::new();
    let mut extra_on_target = Vec::new();
    let mut changed = Vec::new();
    for col in &src_ir.columns {
        if !tgt_map.contains_key(col.name.as_str()) {
            missing_on_target.push(ir_column_snapshot(col));
        }
    }
    for col in &tgt_ir.columns {
        if !src_map.contains_key(col.name.as_str()) {
            extra_on_target.push(ir_column_snapshot(col));
        }
    }
    for col in &src_ir.columns {
        if let Some(tgt_col) = tgt_map.get(col.name.as_str()) {
            let mut changes = Vec::new();
            if col.ir_type != tgt_col.ir_type {
                changes.push(ColumnChange::DataType);
            }
            if col.nullable != tgt_col.nullable {
                changes.push(ColumnChange::Nullable);
            }
            if col.is_primary_key != tgt_col.is_primary_key {
                changes.push(ColumnChange::PrimaryKey);
            }
            if !changes.is_empty() {
                changed.push(ChangedColumnDiff {
                    name: col.name.clone(),
                    source: ir_column_snapshot(col),
                    target: ir_column_snapshot(tgt_col),
                    changes,
                });
            }
        }
    }
    TableColumnDiff {
        table: table.to_string(),
        added: missing_on_target.clone(),
        removed: extra_on_target.clone(),
        missing_on_target,
        extra_on_target,
        changed,
        missing_check_constraints: Vec::new(),
        extra_check_constraints: Vec::new(),
        table_options: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::{CheckConstraint, ColumnSchema, TableOptions};

    fn col(name: &str, ty: &str) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: ty.into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }

    fn schema(cols: Vec<ColumnSchema>) -> TableSchema {
        TableSchema {
            table_name: "users".into(),
            columns: cols,
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        }
    }

    #[test]
    fn source_column_missing_on_target_is_add() {
        let src = schema(vec![col("id", "int"), col("email", "varchar")]);
        let tgt = schema(vec![col("id", "int")]);
        let diff = diff_table_schemas("users", &src, &tgt, None);
        assert_eq!(diff.missing_on_target.len(), 1);
        assert_eq!(diff.missing_on_target[0].name, "email");
        assert_eq!(diff.added.len(), 1);
        assert!(diff.extra_on_target.is_empty());
    }

    #[test]
    fn column_default_and_auto_increment_changes_are_detected() {
        let mut src_col = col("id", "int");
        src_col.default_value = Some("0".into());
        src_col.is_auto_increment = true;
        let mut tgt_col = col("id", "int");
        tgt_col.default_value = Some("1".into());
        let src = schema(vec![src_col]);
        let tgt = schema(vec![tgt_col]);
        let diff = diff_table_schemas("users", &src, &tgt, None);
        assert_eq!(diff.changed.len(), 1);
        assert!(diff.changed[0]
            .changes
            .contains(&super::super::types::ColumnChange::Default));
        assert!(diff.changed[0]
            .changes
            .contains(&super::super::types::ColumnChange::AutoIncrement));
    }

    #[test]
    fn target_only_column_is_drop() {
        let src = schema(vec![col("id", "int")]);
        let tgt = schema(vec![col("id", "int"), col("legacy", "text")]);
        let diff = diff_table_schemas("users", &src, &tgt, None);
        assert_eq!(diff.extra_on_target.len(), 1);
        assert_eq!(diff.extra_on_target[0].name, "legacy");
        assert_eq!(diff.removed.len(), 1);
    }

    fn index(name: &str, columns: &[&str], unique: bool) -> IndexInfo {
        IndexInfo {
            name: name.into(),
            columns: columns.iter().map(|c| (*c).into()).collect(),
            is_unique: unique,
            is_primary: false,
            index_type: "btree".into(),
        }
    }

    #[test]
    fn index_column_change_is_drop_and_create() {
        let mut src = schema(vec![col("id", "int"), col("email", "varchar")]);
        src.indexes.push(index("idx_email", &["email"], false));
        let mut tgt = schema(vec![col("id", "int"), col("email", "varchar")]);
        tgt.indexes
            .push(index("idx_email", &["id", "email"], false));
        let diff = diff_indexes(&src, &tgt);
        assert_eq!(diff.extra_on_target.len(), 1);
        assert_eq!(diff.extra_on_target[0].columns, vec!["id", "email"]);
        assert_eq!(diff.missing_on_target.len(), 1);
        assert_eq!(diff.missing_on_target[0].columns, vec!["email"]);
    }

    #[test]
    fn index_unique_change_is_drop_and_create() {
        let mut src = schema(vec![col("id", "int")]);
        src.indexes.push(index("idx_id", &["id"], true));
        let mut tgt = schema(vec![col("id", "int")]);
        tgt.indexes.push(index("idx_id", &["id"], false));
        let diff = diff_indexes(&src, &tgt);
        assert_eq!(diff.extra_on_target.len(), 1);
        assert!(!diff.extra_on_target[0].is_unique);
        assert_eq!(diff.missing_on_target.len(), 1);
        assert!(diff.missing_on_target[0].is_unique);
    }

    #[test]
    fn index_prefix_length_difference_is_not_treated_as_equal() {
        let mut src = schema(vec![col("id", "int"), col("region", "text")]);
        src.indexes
            .push(index("idx_region", &["region(10)"], false));
        let mut tgt = schema(vec![col("id", "int"), col("region", "text")]);
        tgt.indexes
            .push(index("idx_region", &["region(255)"], false));
        let diff = diff_indexes(&src, &tgt);
        assert_eq!(diff.missing_on_target[0].columns, vec!["region(10)"]);
        assert_eq!(diff.extra_on_target[0].columns, vec!["region(255)"]);
    }

    #[test]
    fn index_type_difference_is_not_treated_as_equal() {
        let mut src = schema(vec![col("id", "int")]);
        src.indexes.push(index("idx_id", &["id"], false));
        let mut tgt = schema(vec![col("id", "int")]);
        let mut hash = index("idx_id", &["id"], false);
        hash.index_type = "hash".into();
        tgt.indexes.push(hash);

        let diff = diff_indexes(&src, &tgt);
        assert_eq!(diff.missing_on_target.len(), 1);
        assert_eq!(diff.extra_on_target.len(), 1);
    }

    #[test]
    fn check_constraints_are_diffed_by_name_without_changing_expression_semantics() {
        let mut src = schema(vec![col("id", "int")]);
        src.check_constraints.push(CheckConstraint {
            name: "users_age_check".into(),
            expression: "age >= 0".into(),
        });
        let mut tgt = schema(vec![col("id", "int")]);
        tgt.check_constraints.push(CheckConstraint {
            name: "users_age_check".into(),
            expression: "  age >= 0  ".into(),
        });
        assert!(diff_table_schemas("users", &src, &tgt, None)
            .missing_check_constraints
            .is_empty());

        tgt.check_constraints[0].expression = "age > 0".into();
        let diff = diff_table_schemas("users", &src, &tgt, None);
        assert_eq!(diff.missing_check_constraints[0].name, "users_age_check");
        assert_eq!(diff.extra_check_constraints[0].expression, "age > 0");

        src.check_constraints[0].expression = "name = 'A B'".into();
        tgt.check_constraints[0].expression = "name = 'a b'".into();
        let diff = diff_table_schemas("users", &src, &tgt, None);
        assert_eq!(diff.missing_check_constraints[0].expression, "name = 'A B'");
        assert_eq!(diff.extra_check_constraints[0].expression, "name = 'a b'");

        src.check_constraints[0].expression = "name = 'a  b'".into();
        tgt.check_constraints[0].expression = "name = 'a b'".into();
        let diff = diff_table_schemas("users", &src, &tgt, None);
        assert_eq!(
            diff.missing_check_constraints[0].expression,
            "name = 'a  b'"
        );
        assert_eq!(diff.extra_check_constraints[0].expression, "name = 'a b'");
    }

    #[test]
    fn table_options_are_diffed_without_fabricating_unsupported_values() {
        let mut src = schema(vec![col("id", "int")]);
        src.table_options = TableOptions {
            comment: Some("orders".into()),
            engine: Some("InnoDB".into()),
            charset: Some("utf8mb4".into()),
            ..TableOptions::default()
        };
        let mut tgt = schema(vec![col("id", "int")]);
        tgt.table_options = TableOptions {
            comment: None,
            engine: Some("InnoDB".into()),
            charset: Some("latin1".into()),
            ..TableOptions::default()
        };
        let diff = diff_table_schemas("orders", &src, &tgt, None);
        let options = diff.table_options.expect("table option diff");
        assert_eq!(options.source.comment.as_deref(), Some("orders"));
        assert_eq!(options.target.charset.as_deref(), Some("latin1"));
        assert!(options.changes.contains(&TableOptionChange::Comment));
        assert!(options.changes.contains(&TableOptionChange::Charset));
        assert!(!options.changes.contains(&TableOptionChange::Engine));
    }

    // ------------------------------------------------ IR 对比（F4c-3 自桌面
    // `commands/sync/tests.rs` 逐字迁移：断言的是 `diff_table_schemas_ir` /
    // `format_ir_type` 的行为，属算法测试，按边界方案 §3「算法测试留在
    // modules」的纪律落位；两个 raw-diff 测试同批归位。

    fn ir_col(name: &str, ir_type: IRType, nullable: bool, is_primary_key: bool) -> IRColumn {
        IRColumn {
            name: name.into(),
            ir_type,
            nullable,
            default_expr: None,
            is_primary_key,
            is_auto_increment: false,
            comment: None,
        }
    }

    #[test]
    fn ir_diff_treats_equivalent_varchar_as_same() {
        // Postgres `character varying(100)` and MySQL `varchar(100)` both map to
        // IRType::Varchar { length: Some(100) } — must not report dataType change.
        let src = IRTable {
            name: "t".into(),
            columns: vec![ir_col(
                "name",
                IRType::Varchar { length: Some(100) },
                true,
                false,
            )],
            primary_keys: vec![],
            table_options: None,
        };
        let tgt = IRTable {
            name: "t".into(),
            columns: vec![ir_col(
                "name",
                IRType::Varchar { length: Some(100) },
                true,
                false,
            )],
            primary_keys: vec![],
            table_options: None,
        };

        let diff = diff_table_schemas_ir("t", &src, &tgt);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn ir_diff_detects_columns_missing_on_target() {
        let src = IRTable {
            name: "t".into(),
            columns: vec![
                ir_col("id", IRType::Int64, false, true),
                ir_col("email", IRType::Text, true, false),
            ],
            primary_keys: vec!["id".into()],
            table_options: None,
        };
        let tgt = IRTable {
            name: "t".into(),
            columns: vec![ir_col("id", IRType::Int64, false, true)],
            primary_keys: vec!["id".into()],
            table_options: None,
        };

        let diff = diff_table_schemas_ir("t", &src, &tgt);
        assert_eq!(diff.missing_on_target.len(), 1);
        assert_eq!(diff.missing_on_target[0].name, "email");
        assert_eq!(diff.added.len(), 1);
        assert!(diff.extra_on_target.is_empty());
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn ir_diff_detects_type_nullable_and_pk_changes() {
        let src = IRTable {
            name: "t".into(),
            columns: vec![
                ir_col("id", IRType::Int32, false, true),
                ir_col("name", IRType::Varchar { length: Some(50) }, true, false),
            ],
            primary_keys: vec!["id".into()],
            table_options: None,
        };
        let tgt = IRTable {
            name: "t".into(),
            columns: vec![
                ir_col("id", IRType::Int64, false, false),
                ir_col("name", IRType::Varchar { length: Some(50) }, false, false),
                ir_col("extra", IRType::Text, true, false),
            ],
            primary_keys: vec![],
            table_options: None,
        };

        let diff = diff_table_schemas_ir("t", &src, &tgt);
        assert!(
            diff.added.is_empty(),
            "source columns are all present on target"
        );
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].name, "extra");
        assert_eq!(diff.removed[0].data_type, "Text");
        assert_eq!(diff.changed.len(), 2);

        let id = diff.changed.iter().find(|c| c.name == "id").unwrap();
        assert!(id.changes.contains(&"dataType".into()));
        assert!(id.changes.contains(&"isPrimaryKey".into()));
        assert_eq!(id.source.data_type, "Int32");
        assert_eq!(id.target.data_type, "Int64");

        let name = diff.changed.iter().find(|c| c.name == "name").unwrap();
        assert_eq!(name.changes, vec!["nullable".to_string()]);
    }

    #[test]
    fn format_ir_type_is_stable() {
        assert_eq!(
            format_ir_type(&IRType::Varchar { length: Some(255) }),
            "Varchar(Some(255))"
        );
        assert_eq!(
            format_ir_type(&IRType::Varchar { length: None }),
            "Varchar(None)"
        );
        assert_eq!(
            format_ir_type(&IRType::Decimal {
                precision: 10,
                scale: 2
            }),
            "Decimal(10,2)"
        );
    }

    #[test]
    fn raw_diff_still_flags_native_string_mismatch() {
        let src = schema(vec![col("name", "character varying(100)")]);
        let tgt = schema(vec![col("name", "varchar(100)")]);

        let diff = diff_table_schemas("users", &src, &tgt, None);
        assert_eq!(diff.changed.len(), 1);
        assert!(diff.changed[0].changes.contains(&"dataType".into()));
    }

    #[test]
    fn diff_table_schemas_detects_added_removed_changed() {
        let mut src = schema(vec![
            col("id", "integer"),
            col("name", "text"),
            col("legacy", "text"),
        ]);
        src.primary_keys = vec!["id".into()];
        let mut tgt = schema(vec![
            col("id", "integer"),
            col("name", "varchar"),
            col("email", "text"),
        ]);
        tgt.primary_keys = vec!["id".into()];

        // Source = desired state: columns on src missing from tgt are "added" (to target).
        let diff = diff_table_schemas("users", &src, &tgt, None);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].name, "legacy");
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].name, "email");
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].name, "name");
        assert!(diff.changed[0].changes.contains(&"dataType".into()));
    }
}
