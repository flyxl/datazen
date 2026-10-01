//! Free tier, second half: what happens when a driver is **missing** a
//! capability.
//!
//! The decision table in `real_driver_contract.rs` maps a declared capability to
//! a verdict, but every driver under test declares all five capabilities
//! `Supported`, so on its own it proves nothing about the missing branch: the
//! `else` arm never ran and the only value it compared was its own parameter.
//!
//! This file closes that gap with a real driver instance behind the branch.
//! `WithheldPreciseCancel` wraps the driver's own driver and withholds exactly
//! one protocol, declaring it absent through the trait's declaration channel
//! and refusing the call explicitly — the behaviour a driver that cannot do it
//! must have. Everything else (dialect, qualification, pagination, connections)
//! is delegated to the inner driver, so nothing here is a fake runtime
//! (`fake-runtime-fixtures.md` §10.4).
//!
//! `#[path]`-included from `real_driver_contract.rs`; never compiled alone.

use datazen_driver_api::{
    validate_schema_target, ConnectionHandle, DatabaseDriver, DriverError, QueryExecutionId,
    SchemaScope,
};

// The whole template surface, so the refusal tier can reach the same helpers the
// other tiers use. `#[path]`-included, so nothing here is a public API of the crate.
use super::*;

/// What an instance **says** about the withheld capability, folded together with
/// what it **does** to a target, as one comparable value.
///
/// Folding the declaration into the comparison is the entire point of this type.
/// `WithheldPreciseCancel` delegates `qualify_sql_target` to the inner driver,
/// and the trait documents that method as pure and stateless — so two qualified
/// statements are equal **no matter what the wrapper declares**. Two assertions
/// in this file used to compare exactly that and therefore proved nothing: a
/// qualifier that ignored its database argument, or a wrapper that quietly
/// stopped withholding, both passed them. Any statement-only comparison in a
/// refusal tier has zero discriminating power, and the only way to give it power
/// is to bind the declaration to the statement.
///
/// `declared` and `execution_cancel` are two views of the same withholding — the
/// contract's claim and the trait method that must contradict it — so a correct
/// wrapper differs from the driver it wraps in exactly these two fields.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RefusalSnapshot {
    declared: Capability,
    execution_cancel: bool,
    original: Option<String>,
    second: Option<String>,
    multi_database: bool,
    schema_level: bool,
}

impl RefusalSnapshot {
    /// One instance, read at one moment, entirely through the trait.
    fn read<D: DatabaseDriver>(
        driver: &D,
        declared: Capability,
        sql: &str,
        original: &str,
        second: &str,
        schema: Option<&str>,
    ) -> Self {
        Self {
            declared,
            execution_cancel: driver.supports_query_execution_cancel(),
            original: driver.qualify_sql_target(sql, Some(original), schema),
            second: driver.qualify_sql_target(sql, Some(second), schema),
            multi_database: driver.has_multi_database(),
            schema_level: driver.has_schema_level(),
        }
    }

    /// The same read off the wrapper, which declares for itself.
    fn read_withheld<D: DatabaseDriver>(
        wrapper: &WithheldPreciseCancel<D>,
        sql: &str,
        original: &str,
        second: &str,
        schema: Option<&str>,
    ) -> Self {
        Self::read(
            wrapper,
            wrapper.declared_capability(),
            sql,
            original,
            second,
            schema,
        )
    }
}

/// A capability the instance declares missing must produce an **explicit
/// refusal** — never a silent success, never a skip, never a `Verify` — and the
/// refusal must leave the session's original target exactly where it was.
///
/// The order matters: both the wrapper and the driver it wraps are read into a
/// [`RefusalSnapshot`] *before* the refusals and again *after* them. Comparing a
/// value with itself would prove nothing, and a capability that vanished without
/// a word is exactly the failure this dimension exists to catch — so the
/// comparison is against the **wrapped driver**, not against the wrapper again.
#[tokio::test]
async fn a_withheld_capability_is_refused_and_leaves_the_original_target_intact() {
    let contract = &crate::CONTRACT;
    let driver = WithheldPreciseCancel::new(crate::contract_driver());

    // The session starts on target A. Everything below must leave it there.
    let original = format!("{FIXTURE_PREFIX}a");
    let marker = contract.dialect.marker;
    let sql = format!("SELECT {marker} FROM {marker}");
    let schema = contract.default_schema;
    // The second fixture target, resolved up front so the "nothing else moved"
    // comparison below is against a reading taken *before* anything was refused.
    let second = format!("{FIXTURE_PREFIX}b");
    let before = RefusalSnapshot::read_withheld(&driver, &sql, &original, &second, schema);
    let inner_before = RefusalSnapshot::read(
        driver.inner(),
        contract.precise_cancel,
        &sql,
        &original,
        &second,
        schema,
    );
    assert!(
        before.original.is_some(),
        "{} must qualify a statement for its own declared target; \
         without a starting point there is nothing left to preserve",
        contract.label
    );

    // --- 1. the declaration, read off the instance rather than passed in.
    let declared = driver.declared_capability();
    assert_eq!(
        declared,
        Capability::Unsupported,
        "{} must report the withheld protocol as absent, or this test asserts nothing",
        contract.label
    );
    assert_eq!(
        verdict_for(declared, Verdict::RefuseAsUnsupported),
        Verdict::RefuseAsUnsupported,
        "a capability the driver itself reports as missing must map to the refusal verdict"
    );
    assert_ne!(
        verdict_for(declared, Verdict::RefuseAsUnsupported),
        Verdict::Verify,
        "a missing capability must never be asserted as verified against a real server"
    );

    // --- 2. the refusal itself. A fabricated handle is enough: the protocol is
    //        refused on the declared capability, before any server work, so the
    //        refusal cannot be mistaken for a connection failure.
    let handle = ConnectionHandle {
        id: format!("{FIXTURE_PREFIX}withheld"),
        pool_id: format!("{FIXTURE_PREFIX}withheld"),
    };
    let execution = QueryExecutionId::new(format!("{FIXTURE_PREFIX}unregistered"));
    let err = driver
        .cancel_query_with_execution(&handle, &execution)
        .await
        .expect_err("a capability declared missing must be refused, not honoured");
    assert_eq!(
        err_kind(&err),
        "Unsupported",
        "expected an explicit Unsupported refusal, got {}",
        err_kind(&err)
    );
    // Targeted refusal, not a generic one: the caller must be able to see *which*
    // execution could not be cancelled (CM-24 forbids papering over it).
    assert!(
        matches!(&err, DriverError::Unsupported(message) if message.contains(execution.as_str())),
        "the refusal must name the execution it could not cancel, got {}",
        err_kind(&err)
    );

    // --- 3. the command gateway refuses the same way: the withheld protocol has
    //        no command at all, and asking for one is an error rather than a
    //        no-op. The absence is asserted first, so the case cannot rot into a
    //        test that passes because the name was never tried.
    const CANCEL_COMMAND: &str = "query_execution_cancel";
    assert!(
        !driver
            .command_definitions()
            .iter()
            .any(|definition| definition.name == CANCEL_COMMAND),
        "the gateway advertises `{CANCEL_COMMAND}`; pick a name this driver really withholds"
    );
    let gateway_err = driver
        .execute_command(
            &handle,
            CANCEL_COMMAND,
            serde_json::json!({ "executionId": execution.as_str() }),
        )
        .await
        .expect_err("the gateway must refuse a command it does not advertise");
    assert_eq!(
        err_kind(&gateway_err),
        "Unsupported",
        "the command gateway must fail closed, got {}",
        err_kind(&gateway_err)
    );

    // --- 4. and the original target is untouched by the refusals. Without this,
    //        "refused" could be satisfied by a driver that quietly re-points the
    //        session and then fails for an unrelated reason.
    assert!(
        validate_schema_target(&driver, &original, schema, SchemaScope::AnySchema).is_ok(),
        "the declared target shape must stay accepted after a refusal"
    );
    let after = RefusalSnapshot::read_withheld(&driver, &sql, &original, &second, schema);
    let inner_after = RefusalSnapshot::read(
        driver.inner(),
        contract.precise_cancel,
        &sql,
        &original,
        &second,
        schema,
    );

    assert_eq!(
        before, after,
        "refusing a withheld capability must change nothing else about the instance: \
         the session's original target has to resolve exactly as it did before, or the \
         refusal only *looks* harmless. before {before:?}, after {after:?}"
    );
    assert_eq!(
        inner_before, inner_after,
        "the wrapped driver must itself be unchanged by refusals issued through the \
         wrapper: before {inner_before:?}, after {inner_after:?}"
    );

    // The two arms that carry the weight. `qualify_sql_target` is documented as
    // pure, so the wrapper's *statements* cannot reveal what it withholds; only
    // its declaration can — and it is compared against the driver's own contract
    // declaration, never against itself.
    assert_ne!(
        after.declared, contract.precise_cancel,
        "{} claims precise cancel is {:?}, and the driver it wraps is held to exactly that \
         claim — so a wrapper that declares the same thing withholds nothing",
        contract.label, contract.precise_cancel
    );
    assert_eq!(
        after,
        RefusalSnapshot {
            // Two views of the one withheld protocol, so the expectation has to
            // override two fields: the contract's claim about the wrapped driver,
            // and the trait method the wrapper is required to contradict. Both
            // must move, and nothing else may.
            declared: Capability::Unsupported,
            execution_cancel: false,
            ..inner_after.clone()
        },
        "withholding one capability must change only the two views of that capability: \
         the wrapper has to read as the driver it wraps, precise cancel overridden to \
         Unsupported/false, and identical everywhere else. Otherwise a wrapper may break \
         unrelated addressing power and the run still reads as 'only one capability was \
         withheld'. wrapper {after:?}, wrapped {inner_after:?}"
    );

    // ...and the refusals must not have narrowed what the driver can address at
    // all: a second target is still accepted, and still qualifies the way the
    // wrapped driver qualifies it. (These two targets are *not* required to
    // resolve differently — `qualify_sql_target` qualifies by schema here, so
    // asserting that they must differ would be a claim about a dialect this
    // dimension does not own.)
    assert!(
        validate_schema_target(&driver, &second, schema, SchemaScope::AnySchema).is_ok(),
        "a second fixture target must stay accepted after a refusal"
    );
    assert_eq!(
        after.second, inner_after.second,
        "the wrapper must qualify a second target exactly as the wrapped driver does"
    );
    assert_eq!(
        after.multi_database, contract.has_multi_database,
        "a refused capability must not shrink the driver's addressing power"
    );
    assert_eq!(
        after.schema_level, contract.has_schema_level,
        "a refused capability must not change the driver's namespace shape"
    );
}
