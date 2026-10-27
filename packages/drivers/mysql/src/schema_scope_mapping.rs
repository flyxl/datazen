//! Exact, AST-guided scope mapping for MySQL view query bodies.

use std::{collections::HashSet, ops::ControlFlow};

use datazen_driver_api::{ObjectKind, SchemaObjectScopeDependency, SchemaObjectScopeMapping};
use sqlparser::{
    ast::{ObjectName, ObjectNamePart, Query, Statement, Visit, Visitor},
    dialect::MySqlDialect,
    parser::Parser,
    tokenizer::Span,
};

pub(super) fn map_view_scope(
    kind: ObjectKind,
    source_scope: &str,
    target_scope: &str,
    definition: &str,
    dependencies: &[SchemaObjectScopeDependency],
) -> Result<Option<SchemaObjectScopeMapping>, String> {
    if kind != ObjectKind::View {
        return Ok(None);
    }
    if source_scope.trim().is_empty() || target_scope.trim().is_empty() {
        return Err("MySQL view scope mapping requires exact non-empty database names".into());
    }
    if source_scope == target_scope {
        return Err(
            "MySQL view scope mapping requires distinct source and target databases".into(),
        );
    }

    validate_dependency_pairs(source_scope, target_scope, dependencies)?;
    let statements = Parser::parse_sql(&MySqlDialect {}, definition)
        .map_err(|error| format!("MySQL view definition cannot be parsed safely: {error}"))?;
    if statements.len() != 1 || !matches!(statements.first(), Some(Statement::Query(_))) {
        return Err("MySQL view scope mapping supports exactly one SELECT query body".into());
    }

    let mut mapper = ViewScopeMapper {
        source_scope,
        target_scope,
        dependencies,
        cte_names: HashSet::new(),
        patches: Vec::new(),
    };
    if let ControlFlow::Break(reason) = statements[0].visit(&mut mapper) {
        return Err(reason);
    }
    let mut patches: Vec<(usize, usize, String)> = mapper
        .patches
        .into_iter()
        .map(|(span, replacement)| {
            let start = location_to_byte(definition, span.start)
                .ok_or_else(|| "MySQL parser returned an invalid relation span".to_string())?;
            let end = location_to_byte(definition, span.end)
                .ok_or_else(|| "MySQL parser returned an invalid relation span".to_string())?;
            Ok::<_, String>((start, end, replacement))
        })
        .collect::<Result<Vec<_>, String>>()?;
    patches.sort_by_key(|(start, _, _)| *start);
    let mut previous_end = 0;
    for (start, end, _) in &patches {
        if *start < previous_end || *start >= *end || *end > definition.len() {
            return Err("MySQL parser returned overlapping or invalid relation spans".into());
        }
        previous_end = *end;
    }

    let mut rewritten = definition.to_owned();
    for (start, end, replacement) in patches.into_iter().rev() {
        if !rewritten.is_char_boundary(start) || !rewritten.is_char_boundary(end) {
            return Err("MySQL parser returned a non-UTF-8 relation span".into());
        }
        rewritten.replace_range(start..end, &replacement);
    }

    Ok(Some(SchemaObjectScopeMapping {
        definition: rewritten,
        dependencies: dependencies
            .iter()
            .map(|dependency| dependency.target.clone())
            .collect(),
    }))
}

fn validate_dependency_pairs(
    source_scope: &str,
    target_scope: &str,
    dependencies: &[SchemaObjectScopeDependency],
) -> Result<(), String> {
    for dependency in dependencies {
        if !matches!(
            ObjectKind::parse(&dependency.source.kind),
            Some(ObjectKind::Table | ObjectKind::View)
        ) || dependency.source.kind != dependency.target.kind
            || dependency.source.name.trim().is_empty()
            || dependency.target.name.trim().is_empty()
            || dependency.source.signature.is_some()
            || dependency.target.signature.is_some()
            || dependency.source.target_name.is_some()
            || dependency.target.target_name.is_some()
            || dependency.source.target_schema.is_some()
            || dependency.target.target_schema.is_some()
        {
            return Err(
                "MySQL view mapping only supports exact table/view dependency identities".into(),
            );
        }
        match dependency.source.schema.as_deref() {
            Some(schema) if schema == source_scope => {
                if dependency.target.schema.as_deref() != Some(target_scope) {
                    return Err(
                        "Local MySQL view dependencies must map to the target database".into(),
                    );
                }
            }
            Some(schema) => {
                if dependency.target != dependency.source || schema.trim().is_empty() {
                    return Err(
                        "External MySQL view dependencies must retain their exact identity".into(),
                    );
                }
            }
            None => {
                return Err(
                    "MySQL view dependency catalog omitted an exact database identity".into(),
                );
            }
        }
    }
    Ok(())
}

struct ViewScopeMapper<'a> {
    source_scope: &'a str,
    target_scope: &'a str,
    dependencies: &'a [SchemaObjectScopeDependency],
    cte_names: HashSet<String>,
    patches: Vec<(Span, String)>,
}

impl Visitor for ViewScopeMapper<'_> {
    type Break = String;

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
        if let Some(with) = &query.with {
            self.cte_names.extend(
                with.cte_tables
                    .iter()
                    .map(|cte| cte.alias.name.value.to_ascii_lowercase()),
            );
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
        match relation.0.as_slice() {
            [ObjectNamePart::Identifier(table)] => {
                if table.value.eq_ignore_ascii_case("dual") {
                    return ControlFlow::Continue(());
                }
                if self.cte_names.contains(&table.value.to_ascii_lowercase()) {
                    if self.dependencies.iter().any(|dependency| {
                        dependency.source.schema.as_deref() == Some(self.source_scope)
                            && dependency.source.name.eq_ignore_ascii_case(&table.value)
                    }) {
                        return ControlFlow::Break(format!(
                            "CTE `{}` collides with a catalog-proven source relation; view scope mapping is ambiguous",
                            table.value
                        ));
                    }
                    return ControlFlow::Continue(());
                }
                let candidates = self
                    .dependencies
                    .iter()
                    .filter(|dependency| {
                        dependency.source.schema.as_deref() == Some(self.source_scope)
                            && dependency.source.name == table.value
                    })
                    .collect::<Vec<_>>();
                if candidates.len() > 1 {
                    return ControlFlow::Break(format!(
                        "Unqualified MySQL view relation `{}` is ambiguous in the dependency catalog",
                        table.value
                    ));
                }
                if let Some(dependency) = candidates.first() {
                    if dependency.source.name != dependency.target.name {
                        self.patches
                            .push((table.span, quote_mysql_identifier(&dependency.target.name)));
                    }
                    return ControlFlow::Continue(());
                }
                ControlFlow::Break(format!(
                    "Unqualified MySQL view relation `{}` is not present in the complete source dependency catalog",
                    table.value
                ))
            }
            [ObjectNamePart::Identifier(schema), ObjectNamePart::Identifier(table)] => {
                let Some(dependency) = self.dependencies.iter().find(|dependency| {
                    dependency.source.schema.as_deref() == Some(schema.value.as_str())
                        && dependency.source.name == table.value
                }) else {
                    return ControlFlow::Break(format!(
                        "Qualified MySQL view relation `{}.{}` is not present in the complete dependency catalog",
                        schema.value, table.value
                    ));
                };
                if schema.value == self.source_scope {
                    self.patches
                        .push((schema.span, quote_mysql_identifier(self.target_scope)));
                    if dependency.source.name != dependency.target.name {
                        self.patches
                            .push((table.span, quote_mysql_identifier(&dependency.target.name)));
                    }
                }
                ControlFlow::Continue(())
            }
            [] => {
                ControlFlow::Break("MySQL parser returned a relation without an identifier".into())
            }
            _ => ControlFlow::Break(
                "Three-part and non-identifier MySQL view relations cannot be scope-mapped safely"
                    .into(),
            ),
        }
    }
}

fn quote_mysql_identifier(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

fn location_to_byte(sql: &str, location: sqlparser::tokenizer::Location) -> Option<usize> {
    if location.line == 0 || location.column == 0 {
        return None;
    }
    let mut line = 1_u64;
    let mut column = 1_u64;
    for (index, character) in sql.char_indices() {
        if (line, column) == (location.line, location.column) {
            return Some(index);
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    if (line, column) == (location.line, location.column) {
        Some(sql.len())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::migration::MysqlMigrationRenderer;
    use datazen_driver_api::{
        DatabaseObject, MigrationOperation, MigrationRenderer, MigrationView,
    };

    fn dependency(
        source_schema: &str,
        source_name: &str,
        target_schema: &str,
        target_name: &str,
    ) -> SchemaObjectScopeDependency {
        let object = |schema: &str, name: &str| DatabaseObject {
            kind: "table".into(),
            schema: Some(schema.into()),
            name: name.into(),
            signature: None,
            target_schema: None,
            target_name: None,
        };
        SchemaObjectScopeDependency {
            source: object(source_schema, source_name),
            target: object(target_schema, target_name),
        }
    }

    #[test]
    fn maps_only_catalog_proven_local_relation_tokens_and_preserves_comments_and_literals() {
        let dependencies = vec![dependency("source_db", "items", "target_db", "items")];
        let sql = "SELECT 'source_db.items' AS label /* source_db.items */ FROM `source_db`.`items` -- source_db.items\n";
        let mapped = map_view_scope(
            ObjectKind::View,
            "source_db",
            "target_db",
            sql,
            &dependencies,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            mapped.definition,
            "SELECT 'source_db.items' AS label /* source_db.items */ FROM `target_db`.`items` -- source_db.items\n"
        );
        assert_eq!(mapped.dependencies[0].schema.as_deref(), Some("target_db"));
    }

    #[test]
    fn maps_unqualified_selected_table_rename_without_touching_columns_or_aliases() {
        let dependencies = vec![dependency(
            "source_db",
            "old_items",
            "target_db",
            "new_items",
        )];
        let mapped = map_view_scope(
            ObjectKind::View,
            "source_db",
            "target_db",
            "SELECT old_items.name FROM old_items",
            &dependencies,
        )
        .unwrap()
        .unwrap();
        assert_eq!(mapped.definition, "SELECT old_items.name FROM `new_items`");
    }

    #[test]
    fn preserves_catalog_proven_external_qualified_relations() {
        let dependencies = vec![dependency("archive_db", "items", "archive_db", "items")];
        let mapped = map_view_scope(
            ObjectKind::View,
            "source_db",
            "target_db",
            "SELECT id FROM `archive_db`.`items`",
            &dependencies,
        )
        .unwrap()
        .unwrap();
        assert_eq!(mapped.definition, "SELECT id FROM `archive_db`.`items`");
        assert_eq!(mapped.dependencies, vec![dependencies[0].source.clone()]);
    }

    #[test]
    fn rejects_unproven_or_unsupported_relations_and_non_query_bodies() {
        let dependencies = vec![dependency("source_db", "items", "target_db", "items")];
        for sql in [
            "SELECT id FROM `other_db`.`items`",
            "SELECT id FROM `source_db`.`missing`",
            "SELECT id FROM source_db.schema.items",
            "SELECT 1; SELECT 2",
        ] {
            assert!(
                map_view_scope(
                    ObjectKind::View,
                    "source_db",
                    "target_db",
                    sql,
                    &dependencies
                )
                .is_err(),
                "{sql}"
            );
        }
        assert!(map_view_scope(
            ObjectKind::Function,
            "source_db",
            "target_db",
            "SELECT 1",
            &dependencies
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn rejects_cte_name_collision_with_a_catalog_dependency() {
        let dependencies = vec![dependency("source_db", "items", "target_db", "items")];
        let error = map_view_scope(
            ObjectKind::View,
            "source_db",
            "target_db",
            "WITH items AS (SELECT 1 AS id) SELECT id FROM items",
            &dependencies,
        )
        .unwrap_err();
        assert!(error.contains("CTE `items` collides"));
    }

    #[test]
    fn rejects_unqualified_relation_missing_from_complete_dependency_catalog() {
        let dependencies = vec![dependency("source_db", "items", "target_db", "items")];
        let error = map_view_scope(
            ObjectKind::View,
            "source_db",
            "target_db",
            "SELECT id FROM missing_items",
            &dependencies,
        )
        .unwrap_err();

        assert!(error.contains("missing_items"));
        assert!(error.contains("complete source dependency catalog"));
    }

    #[test]
    fn mapped_view_renders_target_qualified_ddl_and_rollback() {
        let dependencies = vec![SchemaObjectScopeDependency {
            source: DatabaseObject {
                kind: "table".into(),
                schema: Some("source_db".into()),
                name: "items".into(),
                signature: None,
                target_schema: None,
                target_name: None,
            },
            target: DatabaseObject {
                kind: "table".into(),
                schema: Some("target_db".into()),
                name: "items".into(),
                signature: None,
                target_schema: None,
                target_name: None,
            },
        }];
        let mapped = MysqlMigrationRenderer
            .map_schema_object_scope(
                ObjectKind::View,
                "source_db",
                "target_db",
                "SELECT id FROM `source_db`.`items`",
                &dependencies,
            )
            .unwrap()
            .unwrap();
        let statement = MysqlMigrationRenderer
            .render(&MigrationOperation::CreateView {
                view: MigrationView {
                    schema: Some("target_db".into()),
                    name: "item_view".into(),
                    definition: mapped.definition,
                },
            })
            .unwrap();

        assert_eq!(
            statement.sql,
            "CREATE VIEW `target_db`.`item_view` AS SELECT id FROM `target_db`.`items`"
        );
        assert_eq!(
            statement.rollback_sql.as_deref(),
            Some("DROP VIEW `target_db`.`item_view`")
        );
    }
}
