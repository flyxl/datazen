//! Column (and index) comparison with source = desired.

use super::types::{
    ChangedColumnDiff, CheckConstraintSnapshot, ColumnSnapshot, TableColumnDiff, TableOptionChange,
    TableOptionsDiff,
};
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

    #[test]
    fn postgres_int4_integer_not_reported_as_changed() {
        use datazen_driver_postgres::PostgresTypeNormalizer;
        let normalizer = PostgresTypeNormalizer;
        let src = schema(vec![col("id", "int4")]);
        let tgt = schema(vec![col("id", "integer")]);
        let diff = diff_table_schemas("users", &src, &tgt, Some(&normalizer));
        assert!(diff.changed.is_empty(), "int4 vs integer should match");
    }

    #[test]
    fn mysql_int_display_width_not_reported_as_changed() {
        use datazen_driver_mysql::MysqlTypeNormalizer;
        let normalizer = MysqlTypeNormalizer;
        let src = schema(vec![col("id", "int(11)")]);
        let tgt = schema(vec![col("id", "int")]);
        let diff = diff_table_schemas("users", &src, &tgt, Some(&normalizer));
        assert!(diff.changed.is_empty(), "int(11) vs int should match");
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
}
