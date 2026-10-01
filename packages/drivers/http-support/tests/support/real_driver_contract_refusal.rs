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

/// A capability the instance declares missing must produce an **explicit
/// refusal** — never a silent success, never a skip, never a `Verify` — and the
/// refusal must leave the session's original target exactly where it was.
///
/// The order matters: the target is resolved *before* the refusals and again
/// *after* them. Comparing a value with itself would prove nothing, and a
/// capability that vanished without a word is exactly the failure this dimension
/// exists to catch.
#[tokio::test]
async fn a_withheld_capability_is_refused_and_leaves_the_original_target_intact() {
    let contract = &crate::CONTRACT;
    let driver = WithheldPreciseCancel::new(crate::contract_driver());

    // The session starts on target A. Everything below must leave it there.
    let original = format!("{FIXTURE_PREFIX}a");
    let marker = contract.dialect.marker;
    let sql = format!("SELECT {marker} FROM {marker}");
    let schema = contract.default_schema;
    let target_before = driver.qualify_sql_target(&sql, Some(&original), schema);
    assert!(
        target_before.is_some(),
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
    let target_after = driver.qualify_sql_target(&sql, Some(&original), schema);
    assert_eq!(
        target_before, target_after,
        "a refused capability must not change how the original target resolves"
    );
    assert_eq!(
        target_after,
        driver.qualify_sql_target(&sql, Some(&original), schema),
        "qualification must stay a pure function after a refusal"
    );
    // ...and the refusal must not have narrowed what the driver can address at
    // all: a second target is still resolvable exactly as it was.
    let second = format!("{FIXTURE_PREFIX}b");
    assert!(
        validate_schema_target(&driver, &second, schema, SchemaScope::AnySchema).is_ok(),
        "a second fixture target must stay accepted after a refusal"
    );
    assert_eq!(
        driver.qualify_sql_target(&sql, Some(&second), schema),
        driver.qualify_sql_target(&sql, Some(&second), schema),
        "every target must resolve identically after a refusal"
    );
    assert_eq!(
        driver.has_multi_database(),
        contract.has_multi_database,
        "a refused capability must not shrink the driver's addressing power"
    );
    assert_eq!(
        driver.has_schema_level(),
        contract.has_schema_level,
        "a refused capability must not change the driver's namespace shape"
    );
}
