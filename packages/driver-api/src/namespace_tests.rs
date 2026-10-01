use super::*;

/// A shape with a real three-level hierarchy, the common SQL Server case.
fn three_level_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![
            NamespaceLevel::present(NamespaceLevelKind::Database, true),
            NamespaceLevel::present(NamespaceLevelKind::Catalog, true),
            NamespaceLevel::present(NamespaceLevelKind::Schema, false),
        ],
        case_rules: CaseRules {
            unquoted: CaseFolding::Preserved,
            quoted: CaseFolding::Preserved,
        },
        canonical_id_rules: CanonicalIdRules::default(),
        aliases: BTreeMap::new(),
        path_segments: Vec::new(),
    }
}

/// A shape with no catalog level, e.g. SQLite.
fn flat_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![NamespaceLevel::present(NamespaceLevelKind::Database, true)],
        ..NamespaceShape::default()
    }
}

#[test]
fn canonicalize_keeps_every_present_level() {
    let shape = three_level_shape();
    let target = NamespaceTarget {
        database: Some("sales".to_string()),
        catalog: Some("sales".to_string()),
        schema: Some("dbo".to_string()),
        path: Vec::new(),
    };

    let canonical = shape
        .canonicalize(&target)
        .expect("a complete target canonicalizes");

    assert_eq!(
        canonical.path,
        vec!["sales".to_string(), "sales".to_string(), "dbo".to_string()]
    );
    assert_eq!(canonical.requested, target);
}

#[test]
fn canonicalize_rejects_a_value_for_a_level_that_does_not_exist() {
    let shape = flat_shape();
    let target = NamespaceTarget::default()
        .with_database("main")
        .with_schema("public");

    let error = shape
        .canonicalize(&target)
        .expect_err("a schema level does not exist on this database");

    assert!(matches!(
        error,
        ResourceError::NonexistentNamespaceLevel {
            kind: NamespaceLevelKind::Schema
        }
    ));
}

#[test]
fn canonicalize_rejects_a_missing_required_level() {
    let shape = three_level_shape();
    let error = shape
        .canonicalize(&NamespaceTarget::default().with_database("sales"))
        .expect_err("catalog is required");

    assert!(matches!(
        error,
        ResourceError::MissingRequiredNamespaceLevel {
            kind: NamespaceLevelKind::Catalog
        }
    ));
}

#[test]
fn canonicalize_merges_aliases_to_a_fixed_point() {
    let mut shape = three_level_shape();
    shape
        .aliases
        .insert("sales-db".to_string(), "sales".to_string());
    shape
        .aliases
        .insert("sales".to_string(), "sales_main".to_string());

    let canonical = shape
        .canonicalize(&NamespaceTarget {
            catalog: Some("sales_main".to_string()),
            ..NamespaceTarget::default()
                .with_database("sales-db")
                .with_schema("dbo")
        })
        .expect("a chain of aliases resolves");

    // "sales-db" -> "sales" -> "sales_main". "sales_main" is not itself a key,
    // so resolution terminates instead of reporting a conflict.
    assert_eq!(canonical.path[0], "sales_main");
}

#[test]
fn canonicalize_rejects_an_alias_cycle_instead_of_picking_one() {
    let mut shape = flat_shape();
    shape.aliases.insert("a".to_string(), "b".to_string());
    shape.aliases.insert("b".to_string(), "a".to_string());

    let error = shape
        .canonicalize(&NamespaceTarget::default().with_database("a"))
        .expect_err("an alias cycle is a conflict");

    assert!(matches!(error, ResourceError::AliasConflict { .. }));
}

#[test]
fn canonicalize_allows_the_same_identifier_at_two_levels() {
    let shape = three_level_shape();
    // PostgreSQL's catalog *is* the database, so the same canonical value at
    // two levels is ordinary, not a conflict.
    let canonical = shape
        .canonicalize(&NamespaceTarget {
            catalog: Some("sales".to_string()),
            ..NamespaceTarget::default()
                .with_database("sales")
                .with_schema("sales")
        })
        .expect("repeating a name across levels is legal");

    assert_eq!(
        canonical.path,
        vec![
            "sales".to_string(),
            "sales".to_string(),
            "sales".to_string()
        ]
    );
}

#[test]
fn canonicalize_appends_driver_specific_path_segments() {
    let shape = three_level_shape();
    let canonical = shape
        .canonicalize(&NamespaceTarget {
            database: Some("sales".to_string()),
            catalog: Some("sales".to_string()),
            schema: Some("dbo".to_string()),
            path: vec!["tables".to_string(), "dbo".to_string()],
        })
        .expect("extra segments are appended in order");

    assert_eq!(canonical.path.len(), 5);
    assert_eq!(canonical.path.last().map(String::as_str), Some("dbo"));
}

#[test]
fn requirements_reject_a_missing_required_level() {
    let shape = three_level_shape();
    let requirements = TargetRequirements {
        schema: TargetLevelRequirement::Required,
        ..TargetRequirements::default()
    };
    let target = NamespaceTarget {
        catalog: Some("sales".to_string()),
        ..NamespaceTarget::default()
            .with_database("sales")
            .with_schema("dbo")
    };

    assert!(shape.validate_requirements(&target, &requirements).is_ok());

    let without_schema = NamespaceTarget {
        catalog: Some("sales".to_string()),
        database: Some("sales".to_string()),
        ..NamespaceTarget::default()
    };
    assert!(matches!(
        shape
            .validate_requirements(&without_schema, &requirements)
            .expect_err("schema is required for this operation"),
        ResourceError::MissingRequiredNamespaceLevel {
            kind: NamespaceLevelKind::Schema
        }
    ));
}

#[test]
fn requirements_reject_a_forbidden_level() {
    let shape = three_level_shape();
    let requirements = TargetRequirements {
        catalog: TargetLevelRequirement::Forbidden,
        ..TargetRequirements::default()
    };
    let target = NamespaceTarget {
        database: Some("sales".to_string()),
        ..NamespaceTarget::default().with_schema("dbo")
    };

    assert!(shape.validate_requirements(&target, &requirements).is_ok());

    let with_catalog = NamespaceTarget {
        catalog: Some("sales".to_string()),
        ..target
    };
    assert!(matches!(
        shape
            .validate_requirements(&with_catalog, &requirements)
            .expect_err("this operation must not name a catalog"),
        ResourceError::ForbiddenNamespaceLevel {
            kind: NamespaceLevelKind::Catalog
        }
    ));
}

#[test]
fn requirements_reject_a_path_segment_when_the_operation_forbids_one() {
    let shape = three_level_shape();
    let requirements = TargetRequirements {
        path_forbidden: true,
        ..TargetRequirements::default()
    };
    let target = NamespaceTarget {
        path: vec!["tables".to_string()],
        ..NamespaceTarget::default().with_database("sales")
    };

    assert!(shape.validate_requirements(&target, &requirements).is_err());
}

#[test]
fn an_empty_target_is_never_silently_completed() {
    let shape = three_level_shape();
    // The shape requires a catalog, so an empty target cannot canonicalize —
    // nothing downstream may invent one from the UI or another session.
    assert!(shape.canonicalize(&NamespaceTarget::empty()).is_err());
}

#[test]
fn case_rules_default_to_unknown_rather_than_assuming_one_behaviour() {
    let shape = NamespaceShape::default();
    assert_eq!(shape.case_rules.unquoted, CaseFolding::Unknown);
    assert_eq!(shape.case_rules.quoted, CaseFolding::Unknown);
}

#[test]
fn absent_level_is_never_required() {
    let level = NamespaceLevel::absent(NamespaceLevelKind::Catalog);
    assert!(!level.exists);
    assert!(!level.required);
}

#[test]
fn namespace_shape_serializes_in_camel_case() {
    let shape = flat_shape();
    let value = serde_json::to_value(&shape).expect("shape serializes");
    assert!(value.get("caseRules").is_some());
    assert!(value.get("canonicalIdRules").is_some());
}
