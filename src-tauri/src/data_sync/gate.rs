//! Hard gates before Compare: identical structure + identical PRIMARY KEY.

use datazen_driver_api::{TableSchema, TableType};
use serde::{Deserialize, Serialize};

use super::error::DataSyncError;
use super::types_eq::types_equivalent;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompatCode {
    NotBaseTable,
    MissingPrimaryKey,
    PrimaryKeyMismatch,
    ColumnMissing,
    ColumnExtra,
    TypeMismatch,
    NullabilityMismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompatIssue {
    pub code: CompatCode,
    pub message: String,
    pub column: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GateVerdict {
    Compatible { primary_key: Vec<String> },
    Incompatible { issues: Vec<CompatIssue> },
}

impl GateVerdict {
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible { .. })
    }

    pub fn reason_text(&self) -> String {
        match self {
            Self::Compatible { .. } => String::new(),
            Self::Incompatible { issues } => issues
                .iter()
                .map(|i| i.message.clone())
                .collect::<Vec<_>>()
                .join("; "),
        }
    }

    pub fn into_result(self) -> Result<Vec<String>, DataSyncError> {
        match self {
            Self::Compatible { primary_key } => Ok(primary_key),
            Self::Incompatible { issues } => Err(DataSyncError::incompatible(
                issues
                    .into_iter()
                    .map(|i| i.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            )),
        }
    }
}

pub fn check_base_table(table_type: &TableType, side: &str, name: &str) -> Option<CompatIssue> {
    match table_type {
        TableType::Table => None,
        other => Some(CompatIssue {
            code: CompatCode::NotBaseTable,
            column: None,
            message: format!("{side} '{name}' is {other:?}, not a base table"),
        }),
    }
}

pub fn check_table_gate(family: &str, source: &TableSchema, target: &TableSchema) -> GateVerdict {
    let mut issues = Vec::new();

    let src_pk = source.effective_primary_keys();
    let tgt_pk = target.effective_primary_keys();

    if src_pk.is_empty() || tgt_pk.is_empty() {
        issues.push(CompatIssue {
            code: CompatCode::MissingPrimaryKey,
            column: None,
            message: format!(
                "both tables must have a PRIMARY KEY (source pk={src_pk:?}, target pk={tgt_pk:?})"
            ),
        });
    } else if src_pk != tgt_pk {
        issues.push(CompatIssue {
            code: CompatCode::PrimaryKeyMismatch,
            column: None,
            message: format!(
                "primary key columns/order must match (source={src_pk:?}, target={tgt_pk:?})"
            ),
        });
    }

    let src_names: Vec<&str> = source.columns.iter().map(|c| c.name.as_str()).collect();
    let tgt_names: Vec<&str> = target.columns.iter().map(|c| c.name.as_str()).collect();

    for name in &src_names {
        if !tgt_names.iter().any(|t| t == name) {
            issues.push(CompatIssue {
                code: CompatCode::ColumnMissing,
                column: Some((*name).to_string()),
                message: format!("column '{name}' exists on source but not on target"),
            });
        }
    }
    for name in &tgt_names {
        if !src_names.iter().any(|s| s == name) {
            issues.push(CompatIssue {
                code: CompatCode::ColumnExtra,
                column: Some((*name).to_string()),
                message: format!("column '{name}' exists on target but not on source"),
            });
        }
    }

    for src_col in &source.columns {
        if let Some(tgt_col) = target.columns.iter().find(|c| c.name == src_col.name) {
            if !types_equivalent(family, &src_col.data_type, &tgt_col.data_type) {
                issues.push(CompatIssue {
                    code: CompatCode::TypeMismatch,
                    column: Some(src_col.name.clone()),
                    message: format!(
                        "column '{}' type mismatch ({} vs {})",
                        src_col.name, src_col.data_type, tgt_col.data_type
                    ),
                });
            }
            if src_col.nullable != tgt_col.nullable {
                issues.push(CompatIssue {
                    code: CompatCode::NullabilityMismatch,
                    column: Some(src_col.name.clone()),
                    message: format!(
                        "column '{}' nullability mismatch (source={} target={})",
                        src_col.name, src_col.nullable, tgt_col.nullable
                    ),
                });
            }
        }
    }

    if issues.is_empty() {
        GateVerdict::Compatible {
            primary_key: src_pk,
        }
    } else {
        GateVerdict::Incompatible { issues }
    }
}

#[cfg(test)]
mod tests {
    use datazen_driver_api::{ColumnSchema, TableSchema};

    use super::*;

    fn col(name: &str, ty: &str, nullable: bool, pk: bool) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: ty.into(),
            nullable,
            default_value: None,
            comment: None,
            is_primary_key: pk,
            is_auto_increment: false,
        }
    }

    fn schema(name: &str, cols: Vec<ColumnSchema>, pks: Vec<&str>) -> TableSchema {
        TableSchema {
            table_name: name.into(),
            columns: cols,
            primary_keys: pks.into_iter().map(str::to_string).collect(),
            indexes: vec![],
            foreign_keys: vec![],
        }
    }

    #[test]
    fn identical_mysql_tables_pass_even_if_column_order_differs() {
        let src = schema(
            "users",
            vec![
                col("id", "INT", false, true),
                col("email", "VARCHAR(64)", false, false),
                col("age", "INT", true, false),
            ],
            vec!["id"],
        );
        let tgt = schema(
            "clients",
            vec![
                col("age", "INTEGER", true, false),
                col("id", "INT(11)", false, true),
                col("email", "varchar(64)", false, false),
            ],
            vec!["id"],
        );
        let v = check_table_gate("mysql", &src, &tgt);
        assert_eq!(
            v,
            GateVerdict::Compatible {
                primary_key: vec!["id".into()]
            }
        );
        assert!(v.is_compatible());
        assert!(v.reason_text().is_empty());
        assert_eq!(v.clone().into_result().unwrap(), vec!["id"]);
    }

    #[test]
    fn missing_pk_is_incompatible() {
        let src = schema("t", vec![col("id", "INT", false, false)], vec![]);
        let tgt = schema("t", vec![col("id", "INT", false, false)], vec![]);
        let v = check_table_gate("mysql", &src, &tgt);
        assert!(!v.is_compatible());
        assert!(v.reason_text().contains("PRIMARY KEY"));
        assert!(v.into_result().is_err());
    }

    #[test]
    fn pk_from_column_flags_when_list_empty() {
        let src = schema("t", vec![col("id", "INT", false, true)], vec![]);
        let tgt = schema("t", vec![col("id", "INT", false, true)], vec![]);
        assert!(check_table_gate("mysql", &src, &tgt).is_compatible());
    }

    #[test]
    fn composite_pk_order_matters() {
        let src = schema(
            "t",
            vec![
                col("tenant_id", "INT", false, true),
                col("user_id", "INT", false, true),
            ],
            vec!["tenant_id", "user_id"],
        );
        let tgt = schema(
            "t",
            vec![
                col("tenant_id", "INT", false, true),
                col("user_id", "INT", false, true),
            ],
            vec!["user_id", "tenant_id"],
        );
        let v = check_table_gate("mysql", &src, &tgt);
        match v {
            GateVerdict::Incompatible { issues } => {
                assert_eq!(issues[0].code, CompatCode::PrimaryKeyMismatch);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn extra_and_missing_columns_listed() {
        let src = schema(
            "t",
            vec![col("id", "INT", false, true), col("a", "INT", true, false)],
            vec!["id"],
        );
        let tgt = schema(
            "t",
            vec![col("id", "INT", false, true), col("b", "INT", true, false)],
            vec!["id"],
        );
        let v = check_table_gate("mysql", &src, &tgt);
        match v {
            GateVerdict::Incompatible { issues } => {
                assert!(issues.iter().any(
                    |i| i.code == CompatCode::ColumnMissing && i.column.as_deref() == Some("a")
                ));
                assert!(
                    issues
                        .iter()
                        .any(|i| i.code == CompatCode::ColumnExtra
                            && i.column.as_deref() == Some("b"))
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn type_and_nullability_mismatch() {
        let src = schema(
            "t",
            vec![
                col("id", "INT", false, true),
                col("note", "TEXT", true, false),
                col("n", "INT", true, false),
            ],
            vec!["id"],
        );
        let tgt = schema(
            "t",
            vec![
                col("id", "BIGINT", false, true),
                col("note", "VARCHAR(20)", true, false),
                col("n", "INT", false, false),
            ],
            vec!["id"],
        );
        let v = check_table_gate("mysql", &src, &tgt);
        match v {
            GateVerdict::Incompatible { issues } => {
                assert!(issues.iter().any(|i| i.code == CompatCode::TypeMismatch));
                assert!(issues
                    .iter()
                    .any(|i| i.code == CompatCode::NullabilityMismatch));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn view_is_not_a_base_table() {
        let issue = check_base_table(&TableType::View, "source", "v").unwrap();
        assert_eq!(issue.code, CompatCode::NotBaseTable);
        assert!(check_base_table(&TableType::Table, "source", "t").is_none());
        assert!(check_base_table(&TableType::MaterializedView, "target", "m").is_some());
        assert!(check_base_table(&TableType::SystemTable, "source", "s").is_some());
    }

    #[test]
    fn indexes_do_not_affect_gate() {
        let mut src = schema(
            "t",
            vec![col("id", "INT", false, true), col("n", "INT", true, false)],
            vec!["id"],
        );
        src.indexes.push(datazen_driver_api::IndexInfo {
            name: "idx_n".into(),
            columns: vec!["n".into()],
            is_unique: false,
            is_primary: false,
            index_type: "btree".into(),
        });
        let tgt = schema(
            "t",
            vec![col("id", "INT", false, true), col("n", "INT", true, false)],
            vec!["id"],
        );
        assert!(check_table_gate("mysql", &src, &tgt).is_compatible());
    }
}
