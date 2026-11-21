use std::collections::{HashMap, HashSet};

use crate::{is_schema_grouping_schema, TableInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationSourceKind {
    Table,
    View,
    Cte,
    Subquery,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReferencedRelation {
    pub name: String,
    pub from: usize,
    pub source_kind: RelationSourceKind,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScopeRelations {
    pub relations: Vec<ReferencedRelation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatementRange {
    pub content_from: usize,
    pub content_to: usize,
}

pub fn tables_referenced_in_sql<F>(
    sql: &str,
    dialect_id: Option<&str>,
    ranges: &[StatementRange],
    scopes_for: F,
) -> Vec<String>
where
    F: Fn(&str, (usize, usize)) -> Option<Vec<ScopeRelations>>,
{
    if sql.trim().is_empty() {
        return Vec::new();
    }
    let dialect = dialect_id.unwrap_or("standard");
    let fallback = [StatementRange {
        content_from: 0,
        content_to: sql.len(),
    }];
    let effective = if ranges.is_empty() {
        &fallback[..]
    } else {
        ranges
    };

    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for range in effective {
        let scopes = match scopes_for(dialect, (range.content_from, range.content_to)) {
            Some(scopes) => scopes,
            None => continue,
        };
        let mut relations: Vec<&ReferencedRelation> = scopes
            .iter()
            .flat_map(|scope| scope.relations.iter())
            .filter(|relation| {
                matches!(
                    relation.source_kind,
                    RelationSourceKind::Table | RelationSourceKind::View
                )
            })
            .filter(|relation| !relation.name.is_empty())
            .collect();
        relations.sort_by_key(|relation| relation.from);
        for relation in relations {
            if seen.insert(relation.name.clone()) {
                names.push(relation.name.clone());
            }
        }
    }
    names
}

pub fn infer_default_table<F>(
    sql: &str,
    dialect_id: Option<&str>,
    ranges: &[StatementRange],
    scopes_for: F,
) -> Option<String>
where
    F: Fn(&str, (usize, usize)) -> Option<Vec<ScopeRelations>>,
{
    tables_referenced_in_sql(sql, dialect_id, ranges, scopes_for)
        .into_iter()
        .last()
}

pub fn infer_default_schema(tables: &[TableInfo], views: &[TableInfo]) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for item in tables.iter().chain(views.iter()) {
        let schema = match item.schema.as_deref() {
            Some(schema) if is_schema_grouping_schema(Some(schema)) => schema,
            _ => continue,
        };
        let key = schema.to_owned();
        if !counts.contains_key(&key) {
            order.push(key.clone());
        }
        *counts.entry(key).or_insert(0) += 1;
    }
    if counts.is_empty() {
        return None;
    }
    if counts.contains_key("public") {
        return Some("public".to_owned());
    }
    let mut best: Option<&String> = None;
    let mut best_count = 0usize;
    for key in &order {
        let count = counts.get(key).copied().unwrap_or(0);
        if best.is_none() || count > best_count {
            best = Some(key);
            best_count = count;
        }
    }
    best.cloned()
}

#[cfg(test)]
mod tests {
    use crate::TableType;

    use super::*;

    fn relation(name: &str, from: usize, source_kind: RelationSourceKind) -> ReferencedRelation {
        ReferencedRelation {
            name: name.to_owned(),
            from,
            source_kind,
        }
    }

    fn table(name: &str, schema: Option<&str>) -> TableInfo {
        TableInfo {
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            table_type: TableType::Table,
            row_count: None,
        }
    }

    #[test]
    fn blank_sql_yields_no_tables() {
        let names = tables_referenced_in_sql("  ", None, &[], |_, _| None);
        assert!(names.is_empty());
    }

    #[test]
    fn relations_are_sorted_by_position_and_deduplicated_across_ranges() {
        let ranges = [
            StatementRange {
                content_from: 0,
                content_to: 10,
            },
            StatementRange {
                content_from: 11,
                content_to: 20,
            },
        ];
        let names = tables_referenced_in_sql(
            "select 1; select 2",
            Some("pg"),
            &ranges,
            |dialect, span| {
                assert_eq!(dialect, "pg");
                if span == (0, 10) {
                    Some(vec![ScopeRelations {
                        relations: vec![
                            relation("orders", 8, RelationSourceKind::Table),
                            relation("cte_x", 2, RelationSourceKind::Cte),
                            relation("users", 1, RelationSourceKind::View),
                        ],
                    }])
                } else {
                    Some(vec![ScopeRelations {
                        relations: vec![
                            relation("users", 1, RelationSourceKind::Table),
                            relation("logs", 5, RelationSourceKind::Table),
                            relation("sub", 6, RelationSourceKind::Subquery),
                            relation("", 7, RelationSourceKind::Table),
                        ],
                    }])
                }
            },
        );
        assert_eq!(names, ["users", "orders", "logs"]);
    }

    #[test]
    fn failed_scope_analysis_is_skipped_and_fallback_range_spans_the_script() {
        let names = tables_referenced_in_sql("select * from t", None, &[], |dialect, span| {
            assert_eq!(dialect, "standard");
            assert_eq!(span, (0, 15));
            None
        });
        assert!(names.is_empty());
    }

    #[test]
    fn infer_default_table_is_the_last_referenced_table() {
        let value = infer_default_table("select 1", None, &[], |_, _| {
            Some(vec![ScopeRelations {
                relations: vec![
                    relation("a", 1, RelationSourceKind::Table),
                    relation("b", 2, RelationSourceKind::View),
                ],
            }])
        });
        assert_eq!(value.as_deref(), Some("b"));
        let none = infer_default_table("  ", None, &[], |_, _| None);
        assert_eq!(none, None);
    }

    #[test]
    fn infer_default_schema_prefers_public() {
        let tables = [table("a", Some("audit")), table("b", Some("public"))];
        assert_eq!(
            infer_default_schema(&tables, &[]).as_deref(),
            Some("public")
        );
    }

    #[test]
    fn infer_default_schema_uses_the_most_common_with_first_seen_ties() {
        let tables = [
            table("a", Some("sales")),
            table("b", Some("audit")),
            table("c", Some("sales")),
            table("d", Some("audit")),
        ];
        assert_eq!(infer_default_schema(&tables, &[]).as_deref(), Some("sales"));
    }

    #[test]
    fn infer_default_schema_ignores_non_grouping_schemas_and_includes_views() {
        let tables = [
            table("a", None),
            table("b", Some("CATALOG")),
            table("c", Some("SCHEMA")),
        ];
        let views = [table("v", Some("reporting"))];
        assert_eq!(
            infer_default_schema(&tables, &views).as_deref(),
            Some("reporting")
        );
        assert_eq!(infer_default_schema(&tables, &[]), None);
    }
}
