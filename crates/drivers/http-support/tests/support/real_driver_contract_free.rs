//! Free tier: real assertions that need no server.
//!
//! These are what actually proves the *declared* contract is self-consistent, and
//! they run everywhere — CI included — so they are the half that must never rot
//! into "it compiles, so it is fine". Each test asks a question that could be
//! answered wrongly by a plausible implementation: does the matrix cover every
//! required dimension, do the declarations match the dialect, does a missing
//! capability really map to a refusal, is qualification idempotent.
//!
//! Split out from the template root by responsibility: nothing in this file opens
//! a socket, so it can be read and reviewed without the live tier's 800-line
//! weight dragging it down. `#[path]`-included, never compiled alone.

#![allow(dead_code)]

use datazen_driver_api::{
    validate_schema_target, DatabaseDriver, DriverError, SchemaScope, SqlTarget,
};

use super::*;

// ---------------------------------------------------------------------------
// Free tier — runs everywhere, no server required
// ---------------------------------------------------------------------------

#[test]
fn contract_matrix_covers_every_required_dimension() {
    let contract = &crate::CONTRACT;
    let mut seen = std::collections::BTreeSet::new();
    for (id, test, _) in REQUIRED_DIMENSIONS {
        assert!(seen.insert(*id), "dimension {id} is listed twice");
        assert!(
            test.starts_with("cm"),
            "{id} owner {test} is not a cm* test"
        );
    }
    assert!(
        REQUIRED_DIMENSIONS.len() >= 12,
        "the required-dimension table must not shrink silently"
    );
    assert!(
        !contract.driver_types.is_empty(),
        "driver_types must not be empty"
    );
    assert!(
        contract.driver_types.contains(&contract.label),
        "driver_types must include the canonical label"
    );
    assert!(!contract.dialect.marker.is_empty());
}

#[test]
fn capability_declarations_match_the_contract() {
    let contract = &crate::CONTRACT;
    let driver = crate::contract_driver();

    let reported = driver.driver_type();
    assert!(
        contract.driver_types.contains(&reported.as_str()),
        "driver_type() = {reported} is not a declared value {:?}",
        contract.driver_types
    );
    assert_eq!(driver.has_schema_level(), contract.has_schema_level);
    assert_eq!(driver.has_multi_database(), contract.has_multi_database);
    assert_eq!(driver.default_schema(), contract.default_schema);
    assert_eq!(driver.ddl_atomicity(), contract.ddl_atomicity);
    assert_eq!(driver.supports_offset(), contract.supports_offset);
    assert!(
        driver.quote_char() != '\0',
        "quote_char must name a real delimiter"
    );
    assert!(
        !driver.sync_family().is_empty(),
        "sync_family must not be empty"
    );

    // A schema level without a default schema (or the reverse) is a lie the
    // Host would act on, so it is refused here rather than discovered later.
    assert_eq!(
        contract.has_schema_level,
        contract.default_schema.is_some(),
        "schema level and default schema must agree"
    );

    // `Unknown` / `Unsupported` must not be quietly upgraded to a capability the
    // driver does not actually implement.
    if !contract.precise_cancel.is_present() {
        assert!(
            !driver.supports_query_execution_cancel(),
            "{} declares precise cancel missing but advertises execution cancellation",
            contract.label
        );
    }
    // Pagination is a pure function of (limit, offset) and must stay that way.
    let syntax = driver.pagination_syntax(25, 50);
    assert_eq!(syntax, driver.pagination_syntax(25, 50));
    assert!(
        !syntax.clause.is_empty(),
        "the dialect must produce a pagination clause"
    );
    assert_eq!(
        syntax.clause.contains("OFFSET"),
        contract.supports_offset,
        "OFFSET support and the emitted clause must agree"
    );
}

/// The schema dimension, proven with no server: the shared chokepoint must
/// **refuse** the wrong shape for this driver instead of guessing.
#[test]
fn schema_target_validation_refuses_the_wrong_shape() {
    let driver = crate::contract_driver();
    let database = "dz_target";

    let declared_schema = crate::CONTRACT.default_schema;
    assert!(
        validate_schema_target(&driver, database, declared_schema, SchemaScope::AnySchema).is_ok(),
        "the declared schema shape must always be accepted in AnySchema scope"
    );
    // The declared shape is always resolvable exactly.
    assert!(
        validate_schema_target(&driver, database, declared_schema, SchemaScope::ExactSchema)
            .is_ok(),
        "the declared schema shape must be resolvable in ExactSchema scope too"
    );
    // Only a schema-level engine has something to complain about when no schema
    // is given; for a schema-less engine the database *is* the namespace.
    assert_eq!(
        validate_schema_target(&driver, database, None, SchemaScope::ExactSchema).is_ok(),
        !driver.has_schema_level(),
        "an ExactSchema target carrying no schema is refused exactly when the engine has a schema level"
    );

    if driver.has_schema_level() {
        let schema = crate::CONTRACT
            .default_schema
            .expect("schema level needs a default");
        assert!(
            validate_schema_target(&driver, database, Some(schema), SchemaScope::AnySchema).is_ok()
        );
        assert!(
            validate_schema_target(&driver, database, Some(schema), SchemaScope::ExactSchema)
                .is_ok()
        );
        // Exact resolution with no schema is ambiguous → must be refused.
        assert!(
            matches!(
                validate_schema_target(&driver, database, None, SchemaScope::ExactSchema),
                Err(DriverError::InvalidConfig(_))
            ),
            "a schema-level driver must refuse an ExactSchema target that carries no schema"
        );
    } else {
        // A schema-less engine must refuse a schema it cannot honour, in both scopes.
        for scope in [SchemaScope::AnySchema, SchemaScope::ExactSchema] {
            assert!(
                matches!(
                    validate_schema_target(&driver, database, Some("public"), scope),
                    Err(DriverError::InvalidConfig(_))
                ),
                "a schema-less driver must refuse a schema-qualified target in {scope:?}"
            );
        }
    }
    // Blank is "absent", never a schema named "".
    assert!(validate_schema_target(&driver, database, Some("   "), SchemaScope::AnySchema).is_ok());
}

/// The "missing vs supported" decision table. Every non-`Supported` verdict must
/// map to an explicit refusal — never to a silent skip, and never to a claim that
/// the capability was verified.
///
/// This test owns the *table*; the *instance* behind the missing branch is owned
/// by `refusal::a_withheld_capability_is_refused_and_leaves_the_original_target_intact`,
/// which drives a real driver that declares a capability absent and observes the
/// refusal it produces. Without that second test the `else` arm below would be
/// unreachable on every driver in the matrix, and this table alone would only ever
/// compare a verdict with the refusal it was handed.
#[test]
fn missing_capabilities_demand_an_explicit_refusal() {
    let contract = &crate::CONTRACT;
    let cases: [(&str, Capability, Verdict); 5] = [
        (
            "transactions",
            contract.transactions,
            Verdict::RefuseAsTransactionError,
        ),
        (
            "precise_cancel",
            contract.precise_cancel,
            Verdict::RefuseAsUnsupported,
        ),
        (
            "session_scoped_state",
            contract.session_scoped_state,
            Verdict::RefuseAsUnsupported,
        ),
        (
            "read_snapshots",
            contract.read_snapshots,
            Verdict::RefuseAsUnsupported,
        ),
        (
            "reset_for_reuse",
            contract.reset_for_reuse,
            Verdict::RefuseAsUnsupported,
        ),
    ];
    for (dimension, capability, refusal) in cases {
        let verdict = verdict_for(capability, refusal);
        if capability == Capability::Supported {
            assert_eq!(
                verdict,
                Verdict::Verify,
                "{dimension}: a declared-supported capability must be verified against a real server"
            );
        } else {
            assert_eq!(
                verdict, refusal,
                "{dimension}: a missing capability must produce an explicit-refusal assertion"
            );
            assert_ne!(
                verdict,
                Verdict::Verify,
                "{dimension} is declared missing but would be asserted as verified"
            );
        }
    }
    // Unknown is not the same answer as Unsupported: both refuse, neither verifies.
    assert_ne!(
        verdict_for(Capability::Unknown, Verdict::RefuseAsUnsupported),
        Verdict::Verify
    );
    assert_eq!(
        verdict_for(Capability::Unknown, Verdict::RefuseAsUnsupported),
        verdict_for(Capability::Unsupported, Verdict::RefuseAsUnsupported)
    );
    // The mapping discriminates: one refusal parameter, two declarations, two
    // different verdicts. Without this, `verdict_for` could return its own
    // parameter unconditionally and every assertion above would still pass — the
    // loop compares a verdict with the refusal it was handed, so the only thing
    // that can falsify the table is the mapping actually changing with the input.
    assert_ne!(
        verdict_for(Capability::Supported, Verdict::RefuseAsUnsupported),
        verdict_for(Capability::Unsupported, Verdict::RefuseAsUnsupported),
        "the decision table must actually depend on the declaration"
    );
    // ...and so must the transaction-error row, for the same reason.
    assert_ne!(
        verdict_for(Capability::Supported, Verdict::RefuseAsTransactionError),
        verdict_for(Capability::Unknown, Verdict::RefuseAsTransactionError),
        "the transaction-error row must actually depend on the declaration"
    );
}

/// CM-13/CM-10: target qualification is pure, idempotent, and never emits a
/// context switch — the caller-visible guarantee that a qualified name cannot
/// move the session. Provable without a server because the trait documents
/// `qualify_sql_target` as pure/stateless and idempotent.
#[test]
fn qualified_target_sql_is_idempotent_and_never_switches_context() {
    let driver = crate::contract_driver();
    let contract = &crate::CONTRACT;
    let sql = format!("SELECT marker FROM {}", contract.dialect.marker);
    let target_db = "dz_fixture_other";

    let Some(first) = driver.qualify_sql_target(&sql, Some(target_db), contract.default_schema)
    else {
        // Declared as incapable → the dimension is served by the driver's own
        // per-database routing instead, which the live tier asserts.
        assert!(
            !contract.per_database_resource,
            "{} declares a per-database resource but implements no target qualification",
            contract.label
        );
        return;
    };
    let second = driver
        .qualify_sql_target(&first, Some(target_db), contract.default_schema)
        .expect("a second pass must return Some, not give up mid-rewrite");
    assert_eq!(first, second, "target qualification must be idempotent");
    assert!(
        !first
            .trim_start()
            .to_uppercase()
            .starts_with(&contract.dialect.switch_keyword),
        "qualification must never emit a context switch: {first}"
    );
    // How the database dimension is honoured is itself part of the declared
    // semantic model, and the two branches here are why one template can serve
    // two genuinely different engines:
    if contract.per_database_resource {
        // PostgreSQL cannot name another database in one statement; the `*_at`
        // methods route to that database's own pool instead. Inlining would be
        // a lie, so the template requires the database to stay out of the SQL.
        assert!(
            !first.contains(target_db),
            "a per-database-resource engine routes by connection and must not inline the database: {first}"
        );
    } else {
        // One pool serves every database, so the only way to reach the requested
        // target is to qualify the name with it.
        assert!(
            first.contains(target_db),
            "a single-pool engine must inline the requested database: {first}"
        );
    }
    // No target, no rewrite: a completion lookup must not move context either.
    let absent = SqlTarget::new(None, None);
    assert!(!absent.is_present());
    assert_eq!(absent.schema(), None);
    assert_eq!(SqlTarget::new(Some("  "), None).database(), None);
}

/// §10.2 rules 1 and 3, statically: fixture objects are uniquely named and carry
/// the dedicated prefix, so a live run can never touch a shared or production
/// object and teardown can target exactly what it created.
#[test]
fn fixture_objects_use_the_dedicated_prefix_and_unique_names() {
    let dialect = &crate::CONTRACT.dialect;
    assert!(
        dialect.insert_marker.contains("{marker}"),
        "the A/B marker insert must carry a {{marker}} placeholder"
    );

    // The A/B marker relation is owned per live case: it is `{table}`, filled in
    // from `Fixture::name`, which carries the prefix by construction and is
    // checked for uniqueness below. Checking the *rendered* statement at
    // identifier granularity is strictly stronger than looking for the prefix
    // substring in the template: it proves both that the only relation the
    // statement touches is the one the case handed in, and that the fixed
    // free-tier name is nowhere in it. A single relation shared by every live
    // case in a crate is exactly what makes concurrent live runs race on each
    // other's `CREATE TABLE` and `DROP TABLE`.
    let mut fixture = Fixture::new("static");
    let table = fixture.name("marker");
    for statement in [
        dialect.create_marker,
        dialect.insert_marker,
        dialect.select_marker,
        dialect.drop_marker,
    ] {
        assert!(
            statement.contains("{table}"),
            "a marker statement must be table-templated so each live case owns its \
             relation: {statement}"
        );
        let rendered = render_marker(statement, &table);
        let identifiers = identifiers(&rendered);
        assert!(
            identifiers.contains(&table.to_lowercase()),
            "a rendered marker statement must name the relation it was rendered \
             for ({table}): {rendered}"
        );
        assert!(
            !identifiers.contains(&dialect.marker.to_lowercase()),
            "a marker statement still hard-wires the shared relation `{}`, so every \
             live case in this crate would create, read and drop one table: {rendered}",
            dialect.marker
        );
    }
    assert_eq!(
        render_marker(&dialect.select_marker, &dialect.marker)
            .to_lowercase()
            .contains(&dialect.marker.to_lowercase()),
        true,
        "the marker select must read the marker table by name"
    );
    for statement in [
        dialect.create_named,
        dialect.insert_named,
        dialect.select_named,
        dialect.drop_named,
        dialect.create_temp,
        dialect.insert_temp,
        dialect.select_temp,
    ] {
        assert!(
            statement.contains("{name}"),
            "per-case statement must be name-templated: {statement}"
        );
    }

    let first = fixture.name("one");
    let second = fixture.name("two");
    assert_ne!(first, second, "fixture names must be unique per case");
    for name in [&first, &second] {
        assert!(
            name.starts_with(FIXTURE_PREFIX),
            "{name} lacks the fixture prefix"
        );
        assert!(
            !name.contains(' '),
            "{name} must be usable in an unquoted identifier"
        );
        assert!(
            !name.contains(';'),
            "{name} must not break statement splitting"
        );
    }
    // Teardown is renderable without a server, and targets exactly the relation
    // this case created.
    fixture.on_drop(render_marker(dialect.drop_marker, &table));
    assert_eq!(
        fixture.drops.len(),
        1,
        "teardown must be registered explicitly"
    );
    assert_eq!(
        fixture.drops[0],
        render_marker(dialect.drop_marker, &table),
        "the registered teardown must be the drop of this case\'s own relation"
    );
    // Teardown for a *second session*\'s target is a separate list, because those
    // objects are unreachable from this session\'s handle: registering one on this
    // handle would drop nothing there and survive every run.
    fixture.on_drop_extra(render(&dialect.drop_named, &second));
    assert_eq!(
        fixture.drops_extra.len(),
        1,
        "a second session\'s teardown must be registered explicitly"
    );
    assert_eq!(
        fixture.drops.len(),
        1,
        "a second session\'s teardown must not be queued on this session\'s handle"
    );
}
