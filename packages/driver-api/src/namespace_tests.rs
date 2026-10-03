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

/// A shape that folds unquoted identifiers, e.g. PostgreSQL.
///
/// The two inner levels are declared *optional* so that a database-only target
/// canonicalizes; these tests are about case folding, not about which levels a
/// shape requires.
fn lowercasing_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![
            NamespaceLevel::present(NamespaceLevelKind::Database, true),
            NamespaceLevel::present(NamespaceLevelKind::Catalog, false),
            NamespaceLevel::present(NamespaceLevelKind::Schema, false),
        ],
        case_rules: CaseRules {
            unquoted: CaseFolding::Lowercases,
            quoted: CaseFolding::Preserved,
        },
        canonical_id_rules: CanonicalIdRules::default(),
        aliases: BTreeMap::new(),
        path_segments: Vec::new(),
    }
}

#[test]
fn an_unquoted_identifier_is_folded_by_the_drivers_case_rule() {
    let shape = lowercasing_shape();
    let canonical = shape
        .canonicalize(&NamespaceTarget::default().with_database("Sales"))
        .expect("a mixed-case unquoted name is valid input");

    // The assertion is on a *changed* value. Had folding been left out, this
    // would still have produced "Sales" and this test would have passed while
    // the rule did nothing.
    assert_eq!(canonical.path, vec!["sales".to_string()]);
}

#[test]
fn spellings_differing_only_in_case_reach_one_canonical_identity() {
    let shape = lowercasing_shape();
    let identities: Vec<Vec<String>> = ["Sales", "SALES", "sAlEs", "sales"]
        .iter()
        .map(|spelling| {
            shape
                .canonicalize(&NamespaceTarget::default().with_database(*spelling))
                .expect("each spelling names a database")
                .path
        })
        .collect();

    for (spelling, identity) in ["Sales", "SALES", "sAlEs", "sales"].iter().zip(&identities) {
        assert_eq!(
            identity,
            &vec!["sales".to_string()],
            "{spelling} must land on the same identity as every other spelling"
        );
    }
}

#[test]
fn a_quoted_identifier_keeps_its_case_and_loses_its_quotes() {
    let shape = lowercasing_shape();

    let quoted = shape
        .canonicalize(&NamespaceTarget::default().with_database("\"Sales\""))
        .expect("a quoted name is valid input");

    // Two things are asserted at once: the case survived (the *quoted* rule
    // applies), and the quotes did not (the canonical path is bare, so a
    // driver re-quotes when it builds SQL). An implementation that ignored the
    // quoted rule would produce "sales" here and fail.
    assert_eq!(quoted.path, vec!["Sales".to_string()]);

    // A quoted identifier is a different object from the unquoted spelling of
    // the same letters, and canonicalization must not merge them.
    let unquoted = shape
        .canonicalize(&NamespaceTarget::default().with_database("Sales"))
        .expect("the unquoted spelling is valid input");
    assert_ne!(quoted.path, unquoted.path);
}

#[test]
fn a_doubled_quote_inside_an_identifier_is_unescaped() {
    let shape = lowercasing_shape();
    let canonical = shape
        .canonicalize(&NamespaceTarget::default().with_database("\"Odd\"\"Name\""))
        .expect("an escaped quote is valid input");

    assert_eq!(canonical.path, vec!["Odd\"Name".to_string()]);
}

#[test]
fn an_alias_is_reached_through_a_differently_cased_spelling() {
    let mut shape = lowercasing_shape();
    shape
        .aliases
        .insert("sales".to_string(), "sales_main".to_string());

    let canonical = shape
        .canonicalize(&NamespaceTarget::default().with_database("SALES"))
        .expect("the spelling folds onto the alias key");

    // Folding has to happen *before* the alias lookup: looking up "SALES" would
    // miss the "sales" entry and the alias would silently not apply.
    assert_eq!(canonical.path, vec!["sales_main".to_string()]);
}

#[test]
fn an_unknown_case_rule_folds_nothing_rather_than_guessing() {
    let shape = flat_shape();
    let canonical = shape
        .canonicalize(&NamespaceTarget::default().with_database("Sales"))
        .expect("a driver that declared no rule still canonicalizes");

    // `Unknown` is the driver's admission that it has not declared its rule.
    // Folding anyway would invent an identity, so the caller's spelling is
    // kept — recorded here as a decision, not left as an untested default.
    assert_eq!(canonical.path, vec!["Sales".to_string()]);
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
fn canonicalize_rejects_an_empty_field_value() {
    for kind in [
        NamespaceLevelKind::Database,
        NamespaceLevelKind::Catalog,
        NamespaceLevelKind::Schema,
    ] {
        let shape = three_level_shape();
        let target = NamespaceTarget {
            database: Some("sales".to_string()),
            catalog: Some("sales".to_string()),
            schema: Some("dbo".to_string()),
            path: Vec::new(),
        };
        let target = match kind {
            NamespaceLevelKind::Database => NamespaceTarget {
                database: Some(String::new()),
                ..target
            },
            NamespaceLevelKind::Catalog => NamespaceTarget {
                catalog: Some(String::new()),
                ..target
            },
            NamespaceLevelKind::Schema => NamespaceTarget {
                schema: Some(String::new()),
                ..target
            },
        };

        let error = shape
            .canonicalize(&target)
            .expect_err("an empty string is a value, not an absence");

        // The reason must name the offending level, otherwise a caller cannot
        // tell which field to fix.
        let ResourceError::NamespaceTargetRejected { reason } = error else {
            panic!("empty field must be rejected as itself, got {error:?}");
        };
        assert!(
            reason.contains("empty string") && reason.contains(&format!("{kind:?}")),
            "reason must name the level and the rule, got {reason:?}"
        );
    }
}

#[test]
fn canonicalize_rejects_an_empty_field_before_the_level_check() {
    // `flat_shape` has no catalog level, so `catalog: Some("public")` is rejected
    // as a non-existent level. The empty string must be rejected for the
    // different, more specific reason — otherwise this test would still pass if
    // the empty check were deleted, and the defect would come back unobserved.
    let shape = flat_shape();

    let non_empty = shape
        .canonicalize(
            &NamespaceTarget::default()
                .with_database("main")
                .with_schema("public"),
        )
        .expect_err("flat_shape has no schema level");
    assert!(matches!(
        non_empty,
        ResourceError::NonexistentNamespaceLevel {
            kind: NamespaceLevelKind::Schema
        }
    ));

    let empty = shape
        .canonicalize(&NamespaceTarget {
            database: Some("main".to_string()),
            catalog: Some(String::new()),
            ..NamespaceTarget::default()
        })
        .expect_err("an empty catalog is rejected");
    assert!(
        matches!(empty, ResourceError::NamespaceTargetRejected { .. }),
        "an empty value must be rejected as an empty value, not as a missing level: {empty:?}"
    );
}

#[test]
fn canonicalize_rejects_an_empty_path_segment() {
    let shape = three_level_shape();
    let error = shape
        .canonicalize(&NamespaceTarget {
            database: Some("sales".to_string()),
            catalog: Some("sales".to_string()),
            schema: Some("dbo".to_string()),
            path: vec!["tables".to_string(), String::new()],
        })
        .expect_err("an empty segment would become an empty canonical identifier");

    let ResourceError::NamespaceTargetRejected { reason } = error else {
        panic!("empty path segment must be rejected, got {error:?}");
    };
    assert_eq!(reason, "namespace path segment 1 must not be empty");
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
