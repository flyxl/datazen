// Structured dependency-catalog row parsing shared by the schema command.

use super::{column_index, value_as_string, value_as_string_allow_empty};
use crate::schema_dependencies::{
    SequenceDependencyUsage, SequenceDependencyUsageKind, TypeDependencyUsage,
    TypeDependencyUsageKind,
};
use crate::schema_objects::DatabaseObject;
use crate::types::{DriverError, QueryResult};

// 返回类型是刻意设计：七元组按位置对应依赖目录查询的选列计数、对象列表、类型依赖、类型依赖支持标志、
// 序列依赖与序列依赖支持标志，是 schema 依赖命令的既有形状。收成命名结构体属于契约变更。
#[allow(clippy::type_complexity)]
pub(super) fn parse_object_dependency_catalog(
    result: &QueryResult,
    require_type_usage: bool,
    required_sequence_usage: Option<SequenceDependencyUsageKind>,
) -> Result<
    (
        Option<i64>,
        Option<i64>,
        Vec<DatabaseObject>,
        Vec<TypeDependencyUsage>,
        bool,
        Vec<SequenceDependencyUsage>,
        bool,
    ),
    DriverError,
> {
    let selected_idx = column_index(&result.columns, &["selected_count"]).ok_or_else(|| {
        DriverError::QueryFailed("Dependency catalog query missing selected_count".into())
    })?;
    let unsupported_idx =
        column_index(&result.columns, &["unsupported_count"]).ok_or_else(|| {
            DriverError::QueryFailed("Dependency catalog query missing unsupported_count".into())
        })?;
    let kind_idx = column_index(&result.columns, &["kind"]);
    let schema_idx = column_index(&result.columns, &["dependency_schema", "schema"]);
    let name_idx = column_index(&result.columns, &["name"]);
    let signature_idx = column_index(&result.columns, &["signature"]);
    let type_usage_idx = column_index(&result.columns, &["type_usage"]);
    let column_name_idx = column_index(&result.columns, &["column_name"]);
    let sequence_usage_idx = column_index(&result.columns, &["sequence_usage"]);
    let sequence_schema_idx = column_index(&result.columns, &["sequence_schema"]);
    let sequence_name_idx = column_index(&result.columns, &["sequence_name"]);
    let owner_table_schema_idx = column_index(&result.columns, &["owner_table_schema"]);
    let owner_table_name_idx = column_index(&result.columns, &["owner_table_name"]);
    let owner_column_name_idx = column_index(&result.columns, &["owner_column_name"]);
    let first = result.rows.first().ok_or_else(|| {
        DriverError::QueryFailed("Dependency catalog query returned no status row".into())
    })?;
    let selected_count = first
        .get(selected_idx)
        .and_then(Option::as_ref)
        .and_then(|value| value_as_string(Some(value)))
        .and_then(|count| count.parse::<i64>().ok());
    let unsupported_count = first
        .get(unsupported_idx)
        .and_then(Option::as_ref)
        .and_then(|value| value_as_string(Some(value)))
        .and_then(|count| count.parse::<i64>().ok());
    let Some(kind_idx) = kind_idx else {
        return Err(DriverError::QueryFailed(
            "Dependency catalog query missing kind column".into(),
        ));
    };
    let Some(schema_idx) = schema_idx else {
        return Err(DriverError::QueryFailed(
            "Dependency catalog query missing schema column".into(),
        ));
    };
    let Some(name_idx) = name_idx else {
        return Err(DriverError::QueryFailed(
            "Dependency catalog query missing name column".into(),
        ));
    };
    let mut dependencies = std::collections::BTreeMap::new();
    let mut type_dependency_usages = Vec::new();
    let mut type_usage_complete = true;
    let mut sequence_dependency_usages = Vec::new();
    let mut sequence_usage_complete = required_sequence_usage.is_none()
        || (sequence_usage_idx.is_some()
            && sequence_schema_idx.is_some()
            && sequence_name_idx.is_some()
            && owner_table_schema_idx.is_some()
            && owner_table_name_idx.is_some()
            && owner_column_name_idx.is_some());
    for row in &result.rows {
        let Some(kind) = row
            .get(kind_idx)
            .and_then(Option::as_ref)
            .and_then(|value| value_as_string(Some(value)))
        else {
            continue;
        };
        let Some(schema) = row
            .get(schema_idx)
            .and_then(Option::as_ref)
            .and_then(|value| value_as_string(Some(value)))
        else {
            return Err(DriverError::QueryFailed(
                "Dependency catalog returned an unqualified dependency".into(),
            ));
        };
        let Some(name) = row
            .get(name_idx)
            .and_then(Option::as_ref)
            .and_then(|value| value_as_string(Some(value)))
        else {
            return Err(DriverError::QueryFailed(
                "Dependency catalog returned a dependency without a name".into(),
            ));
        };
        if !matches!(
            kind.as_str(),
            "table" | "view" | "function" | "procedure" | "sequence" | "type"
        ) {
            return Err(DriverError::QueryFailed(format!(
                "Dependency catalog returned unsupported object kind: {kind}"
            )));
        }
        let signature = signature_idx.and_then(|index| {
            row.get(index)
                .and_then(Option::as_ref)
                .and_then(|value| value_as_string_allow_empty(Some(value)))
        });
        let object = DatabaseObject {
            kind: kind.clone(),
            schema: Some(schema.clone()),
            name: name.clone(),
            signature: signature.clone(),
            target_schema: None,
            target_name: None,
        };
        if kind == "type" {
            if let Some(usage_idx) = type_usage_idx {
                let usage = row
                    .get(usage_idx)
                    .and_then(Option::as_ref)
                    .and_then(|value| value_as_string(Some(value)));
                let column_name = column_name_idx.and_then(|index| {
                    row.get(index)
                        .and_then(Option::as_ref)
                        .and_then(|value| value_as_string(Some(value)))
                });
                let usage = match usage.as_deref() {
                    Some("column_type") if column_name.is_some() => {
                        Some(TypeDependencyUsageKind::ColumnType)
                    }
                    Some("expression") => Some(TypeDependencyUsageKind::Expression),
                    Some("constraint") => Some(TypeDependencyUsageKind::Constraint),
                    _ => None,
                };
                if let Some(usage) = usage {
                    type_dependency_usages.push(TypeDependencyUsage {
                        dependency: object.clone(),
                        usage,
                        column_name,
                    });
                } else {
                    type_usage_complete = false;
                }
            } else if require_type_usage {
                type_usage_complete = false;
            }
        }
        if let Some(expected_usage) = required_sequence_usage {
            let is_sequence_dependency = match expected_usage {
                SequenceDependencyUsageKind::ColumnDefault => kind == "sequence",
                SequenceDependencyUsageKind::OwnedBy => kind == "table",
            };
            let raw_usage = sequence_usage_idx.and_then(|index| {
                row.get(index)
                    .and_then(Option::as_ref)
                    .and_then(|value| value_as_string(Some(value)))
            });
            if is_sequence_dependency {
                let sequence_schema = sequence_schema_idx.and_then(|index| {
                    row.get(index)
                        .and_then(Option::as_ref)
                        .and_then(|value| value_as_string(Some(value)))
                });
                let sequence_name = sequence_name_idx.and_then(|index| {
                    row.get(index)
                        .and_then(Option::as_ref)
                        .and_then(|value| value_as_string(Some(value)))
                });
                let owner_table_schema = owner_table_schema_idx.and_then(|index| {
                    row.get(index)
                        .and_then(Option::as_ref)
                        .and_then(|value| value_as_string(Some(value)))
                });
                let owner_table_name = owner_table_name_idx.and_then(|index| {
                    row.get(index)
                        .and_then(Option::as_ref)
                        .and_then(|value| value_as_string(Some(value)))
                });
                let owner_column_name = owner_column_name_idx.and_then(|index| {
                    row.get(index)
                        .and_then(Option::as_ref)
                        .and_then(|value| value_as_string(Some(value)))
                });
                let expected_usage_name = match expected_usage {
                    SequenceDependencyUsageKind::ColumnDefault => "column_default",
                    SequenceDependencyUsageKind::OwnedBy => "owned_by",
                };
                let usage_matches = raw_usage.as_deref() == Some(expected_usage_name);
                let identities = match (
                    sequence_schema,
                    sequence_name,
                    owner_table_schema,
                    owner_table_name,
                    owner_column_name,
                ) {
                    (
                        Some(sequence_schema),
                        Some(sequence_name),
                        Some(owner_table_schema),
                        Some(owner_table_name),
                        Some(column_name),
                    ) => {
                        let sequence = DatabaseObject {
                            kind: "sequence".into(),
                            schema: Some(sequence_schema.clone()),
                            name: sequence_name.clone(),
                            signature: None,
                            target_schema: None,
                            target_name: None,
                        };
                        let owner_table = DatabaseObject {
                            kind: "table".into(),
                            schema: Some(owner_table_schema.clone()),
                            name: owner_table_name.clone(),
                            signature: None,
                            target_schema: None,
                            target_name: None,
                        };
                        let dependency_matches = match expected_usage {
                            SequenceDependencyUsageKind::ColumnDefault => {
                                object.kind == sequence.kind
                                    && object.schema == sequence.schema
                                    && object.name == sequence.name
                            }
                            SequenceDependencyUsageKind::OwnedBy => {
                                object.kind == owner_table.kind
                                    && object.schema == owner_table.schema
                                    && object.name == owner_table.name
                            }
                        };
                        dependency_matches.then_some(SequenceDependencyUsage {
                            sequence,
                            owner_table,
                            column_name,
                            usage: expected_usage,
                        })
                    }
                    _ => None,
                };
                if usage_matches {
                    if let Some(usage) = identities {
                        sequence_dependency_usages.push(usage);
                    } else {
                        sequence_usage_complete = false;
                    }
                } else {
                    sequence_usage_complete = false;
                }
            } else if raw_usage.is_some() {
                sequence_usage_complete = false;
            }
        }
        dependencies.insert((kind, schema, name, signature), object);
    }
    type_dependency_usages.sort_by(|left, right| {
        (
            &left.dependency.schema,
            &left.dependency.name,
            left.usage as u8,
            &left.column_name,
        )
            .cmp(&(
                &right.dependency.schema,
                &right.dependency.name,
                right.usage as u8,
                &right.column_name,
            ))
    });
    type_dependency_usages.dedup();
    sequence_dependency_usages.sort_by(|left, right| {
        (
            &left.sequence.schema,
            &left.sequence.name,
            &left.owner_table.schema,
            &left.owner_table.name,
            &left.column_name,
            left.usage as u8,
        )
            .cmp(&(
                &right.sequence.schema,
                &right.sequence.name,
                &right.owner_table.schema,
                &right.owner_table.name,
                &right.column_name,
                right.usage as u8,
            ))
    });
    sequence_dependency_usages.dedup();
    if required_sequence_usage == Some(SequenceDependencyUsageKind::OwnedBy)
        && sequence_dependency_usages.len() > 1
    {
        sequence_usage_complete = false;
    }
    Ok((
        selected_count,
        unsupported_count,
        dependencies.into_values().collect(),
        type_dependency_usages,
        type_usage_complete,
        sequence_dependency_usages,
        sequence_usage_complete,
    ))
}
