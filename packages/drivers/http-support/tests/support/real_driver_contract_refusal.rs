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
    validate_schema_target, ConnectionHandle, DatabaseDriver, DdlAtomicity, DriverError,
    QueryExecutionId, SchemaScope, Value,
};
use std::fs;

// The whole template surface, so the refusal tier can reach the same helpers the
// other tiers use. `#[path]`-included, so nothing here is a public API of the crate.
use super::*;

/// The identifier both instances quote in [`RefusalSnapshot`]. Fixed, so the
/// comparison reads the same input twice instead of comparing whatever a caller
/// happened to pass — and it exercises `quote_ident`, which returns the name
/// unchanged for something that needs no quoting.
const PROBE_IDENT: &str = "dz_refusal_probe";

/// The value both instances turn into a literal. It carries an embedded
/// apostrophe on purpose: "returns some string" is not the claim "escapes a quote
/// in a string literal".
const LITERAL_PROBE: &str = "o'brien";

/// The header of the wrapper's trait impl, used by the source-level coverage
/// test below. Spelled out here rather than derived from the type, because the
/// test has to find the block in the *file*, not in the type system.
const WRAPPER_IMPL_HEADER: &str =
    "impl<D: DatabaseDriver> DatabaseDriver for WithheldPreciseCancel<D>";

/// Every trait method the wrapper implements that needs **no server and no
/// connection handle** — which is exactly the set [`RefusalSnapshot`] has to
/// read, because a tier that runs with no server can call nothing else.
///
/// This list and [`SERVER_REQUIRED_TRAIT_METHODS`] partition the wrapper's trait
/// impl; `the_wrapper_trait_surface_is_fully_classified` fails the run if they
/// stop doing so, so a method added to the wrapper cannot slip past both.
pub const SERVERLESS_TRAIT_METHODS: &[&str] = &[
    "supports_query_execution_cancel",
    "driver_type",
    "sync_family",
    "quote_char",
    "quote_ident",
    "ddl_atomicity",
    "format_sql_literal",
    "supports_offset",
    "supports_explain",
    "command_definitions",
    "qualify_sql_target",
    "has_multi_database",
    "has_schema_level",
];

/// The methods the snapshot does **not** read, each with why. They all take a
/// live connection, and this tier never opens one, so nothing compares their
/// outputs — a wrapper that broke one of them would pass every test in this
/// file. That gap is stated in the test name and in the docs; it is a ceiling of
/// what the snapshot proves, not a property it has.
pub const SERVER_REQUIRED_TRAIT_METHODS: &[(&str, &str)] = &[
    ("cancel_query_with_execution", "refused and asserted directly in this file, not through the snapshot; observing it through a snapshot would need a live handle"),
    ("connect", "opens the connection a no-server tier has none of"),
    ("test_connection", "needs the connection it would be testing"),
    ("disconnect", "needs a connection to close"),
    ("get_databases", "needs a live session to list databases"),
    ("get_tables", "needs a live session to list tables"),
    ("get_table_schema", "needs a live session to read a schema"),
    ("query", "needs a live session to send a statement"),
    ("query_multi", "needs a live session to send statements"),
    ("query_with_params", "needs a live session to send a bound statement"),
    ("execute", "needs a live session to execute"),
    ("cancel_query", "needs a live execution to cancel"),
];

/// One Driver Command as the snapshot compares it: the identity fields a wrapper
/// could plausibly corrupt, and nothing that needs a server to read.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandRow {
    id: String,
    name: String,
    permissions: Vec<String>,
}

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
///
/// The remaining fields exist so "identical everywhere else" is not a claim the
/// type cannot back. This type once held six fields while the wrapper implements
/// 25 trait methods, so 19 methods had no observation at all and a wrapper that
/// appended `!` to every identifier, or inverted `supports_offset`, went green
/// through this whole file. Every field here reads one of
/// [`SERVERLESS_TRAIT_METHODS`]; what is left over is named, with reasons, in
/// [`SERVER_REQUIRED_TRAIT_METHODS`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct RefusalSnapshot {
    declared: Capability,
    execution_cancel: bool,
    original: Option<String>,
    second: Option<String>,
    multi_database: bool,
    schema_level: bool,
    driver_type: String,
    sync_family: String,
    quote_char: char,
    quoted_ident: String,
    literal_absent: String,
    literal_quote: String,
    ddl_atomicity: DdlAtomicity,
    supports_offset: bool,
    supports_explain: bool,
    commands: Vec<CommandRow>,
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
        let mut commands: Vec<CommandRow> = driver
            .command_definitions()
            .into_iter()
            .map(|d| CommandRow {
                id: d.id,
                name: d.name,
                permissions: d.permissions,
            })
            .collect();
        commands.sort_by(|a, b| a.id.cmp(&b.id));
        Self {
            declared,
            execution_cancel: driver.supports_query_execution_cancel(),
            original: driver.qualify_sql_target(sql, Some(original), schema),
            second: driver.qualify_sql_target(sql, Some(second), schema),
            multi_database: driver.has_multi_database(),
            schema_level: driver.has_schema_level(),
            driver_type: driver.driver_type(),
            sync_family: driver.sync_family(),
            quote_char: driver.quote_char(),
            quoted_ident: driver.quote_ident(PROBE_IDENT),
            literal_absent: driver.format_sql_literal(&None),
            literal_quote: driver
                .format_sql_literal(&Some(Value::String(LITERAL_PROBE.to_string()))),
            ddl_atomicity: driver.ddl_atomicity(),
            supports_offset: driver.supports_offset(),
            supports_explain: driver.supports_explain(),
            commands,
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

    /// One line per field, for a failure message.
    ///
    /// `{:?}` on the whole snapshot lists all 21 Driver Commands twice, which
    /// buries the single field that actually diverged — the field list *is* the
    /// claim being tested, so it is what a reader needs to see.
    fn summary(&self) -> String {
        format!(
            "declared={:?} execution_cancel={} driver_type={:?} sync_family={:?} quote_char={:?} \
             quoted_ident={:?} literal_absent={:?} literal_quote={:?} ddl_atomicity={:?} \
             supports_offset={} supports_explain={} commands=[{}] original={:?} second={:?} \
             multi_database={} schema_level={}",
            self.declared,
            self.execution_cancel,
            self.driver_type,
            self.sync_family,
            self.quote_char,
            self.quoted_ident,
            self.literal_absent,
            self.literal_quote,
            self.ddl_atomicity,
            self.supports_offset,
            self.supports_explain,
            self.commands
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>()
                .join(","),
            self.original,
            self.second,
            self.multi_database,
            self.schema_level,
        )
    }
}

/// A capability the instance declares missing must produce an **explicit
/// refusal** — never a silent success, never a skip, never a `Verify` — and the
/// refusal must leave the session's original target exactly where it was.
///
/// The name states the limit on purpose. The wrapper's whole *serverless* trait
/// surface has to read as the wrapped driver's; the methods that need a live
/// connection are not in this comparison and cannot be, since this tier runs
/// with no server. [`SERVER_REQUIRED_TRAIT_METHODS`] names them, and
/// `the_wrapper_trait_surface_is_fully_classified` keeps that list equal to the
/// wrapper's actual impl.
///
/// The order matters: both the wrapper and the driver it wraps are read into a
/// [`RefusalSnapshot`] *before* the refusals and again *after* them. Comparing a
/// value with itself would prove nothing, and a capability that vanished without
/// a word is exactly the failure this dimension exists to catch — so the
/// comparison is against the **wrapped driver**, not against the wrapper again.
#[tokio::test]
async fn a_withheld_capability_is_refused_and_leaves_the_target_and_the_serverless_trait_surface_intact(
) {
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
        "withholding one capability must change only the two views of that capability: the \
         wrapper has to read as the driver it wraps, precise cancel overridden to \
         Unsupported/false, and equal on all {} trait methods that need no server — one \
         field each, listed in the two summaries below. A wrapper that corrupted one of \
         those, a mangled quote_ident or an inverted supports_offset, otherwise reads as \
         'only one capability was withheld' while having broken addressing power the driver \
         never lost. This comparison does NOT reach the {} methods that need a live \
         connection ({}): this tier opens none, so their outputs are compared by nobody \
         here, by construction.\n  wrapper: {after}\n  wrapped: {inner}",
        SERVERLESS_TRAIT_METHODS.len(),
        SERVER_REQUIRED_TRAIT_METHODS.len(),
        SERVER_REQUIRED_TRAIT_METHODS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", "),
        after = after.summary(),
        inner = inner_after.summary(),
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

/// Every trait method the wrapper implements is either **compared** by
/// [`RefusalSnapshot`] or **named as unchecked** with a reason — and the two
/// lists below are read against the wrapper's actual source, so neither can drift
/// away from it.
///
/// Without this, "the snapshot has 6 fields" and "the wrapper implements 25
/// methods" are two true statements that add up to 19 methods nothing observes.
/// That gap is invisible to a reader of the passing test, so it is made a
/// failing one: add a delegation to the wrapper and this test fails until the new
/// method is either observed or disclosed. Removing a method from the observed
/// list fails it too.
///
/// How it reads the source, stated so the guard is not credited with more than it
/// does: it locates the impl block, blanks `"…"` literals, and takes `fn` names
/// declared at brace depth 0. A method therefore cannot be missed (trait impl
/// methods sit at depth 0, and Rust forbids `pub` or a body-less declaration
/// there); a `fn` written in a comment can be counted spuriously, which fails the
/// run rather than passing it.
#[test]
fn the_wrapper_trait_surface_is_fully_classified_as_checked_or_named_as_unchecked() {
    let plumbing = template_sources()
        .into_iter()
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name == "real_driver_contract_plumbing.rs")
        })
        .unwrap_or_else(|| {
            panic!(
                "the template no longer contains real_driver_contract_plumbing.rs, so the \
                 wrapper's trait surface cannot be classified and the comparison in {} is \
                 unchecked",
                file!()
            )
        });
    let text = fs::read_to_string(&plumbing).unwrap_or_else(|e| {
        panic!(
            "{} could not be read: {e} — an unreadable file is never reported as a clean one",
            plumbing.display()
        )
    });

    let body = trait_impl_body(&text, WRAPPER_IMPL_HEADER);
    let implemented = methods_in(&body);
    assert!(
        !implemented.is_empty(),
        "no trait method was found in {header:?}: the scan is broken, and an empty result \
         would let both lists stay wrong",
        header = WRAPPER_IMPL_HEADER
    );

    for name in &implemented {
        let checked = SERVERLESS_TRAIT_METHODS.contains(&name.as_str());
        let disclosed = SERVER_REQUIRED_TRAIT_METHODS
            .iter()
            .any(|(other, _)| other == name);
        assert!(
            checked || disclosed,
            "the wrapper implements {name:?}, which RefusalSnapshot neither reads nor \
             SERVER_REQUIRED_TRAIT_METHODS discloses. Every delegation needs a verdict: \
             call it in RefusalSnapshot::read, or name it with a reason."
        );
    }
    for name in SERVERLESS_TRAIT_METHODS {
        assert!(
            implemented.contains(&name.to_string()),
            "{name:?} is listed as compared by RefusalSnapshot but the wrapper no longer \
             implements it — the snapshot reads {implemented:?}"
        );
    }
    for (name, reason) in SERVER_REQUIRED_TRAIT_METHODS {
        assert!(
            implemented.contains(&name.to_string()),
            "{name:?} is disclosed as unchecked but the wrapper no longer implements it, so \
             the disclosure is stale"
        );
        assert!(
            !reason.trim().is_empty(),
            "{name:?} is disclosed as unchecked with no reason; a bare list entry reads as \
             an oversight instead of a ceiling"
        );
    }

    // The containment checks above could in principle all hold while a real
    // method slipped past the scanner. The count closes that: the two lists must
    // account for exactly the methods the source declares.
    assert_eq!(
        SERVERLESS_TRAIT_METHODS.len() + SERVER_REQUIRED_TRAIT_METHODS.len(),
        implemented.len(),
        "the two lists claim {} methods but the wrapper implements {} — one side is stale. \
         implemented: {implemented:?}",
        SERVERLESS_TRAIT_METHODS.len() + SERVER_REQUIRED_TRAIT_METHODS.len(),
        implemented.len()
    );
}

/// The body of the trait impl whose header is `header` — everything between its
/// braces, brace-balanced, with string literals blanked.
fn trait_impl_body(text: &str, header: &str) -> String {
    let start = text.find(header).unwrap_or_else(|| {
        panic!(
            "{header:?} is not in the file — the wrapper moved or was renamed, so its trait \
             surface cannot be read"
        )
    });
    let after_header = &text[start..];
    let open = after_header
        .find('{')
        .unwrap_or_else(|| panic!("{header:?} has no opening brace, so its body cannot be read"));
    let body = without_string_literals(&after_header[open + 1..]);
    let mut depth = 1i32;
    for (index, ch) in body.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return body[..index].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("{header:?} has unbalanced braces, so its body cannot be read")
}

/// Method names declared directly in a trait impl body, in source order.
///
/// Depth 0 of the body is the impl's own level, so a `fn` nested inside another
/// function is not counted.
fn methods_in(body: &str) -> Vec<String> {
    let mut depth = 0i32;
    let mut names = Vec::new();
    for line in body.lines() {
        if depth == 0 {
            let trimmed = line.trim_start();
            let declaration = trimmed.strip_prefix("async ").unwrap_or(trimmed);
            if let Some(rest) = declaration.strip_prefix("fn ") {
                if let Some(name) = rest.split(['(', '<', ' ', ':']).next() {
                    names.push(name.to_string());
                }
            }
        }
        depth += line.matches('{').count() as i32;
        depth -= line.matches('}').count() as i32;
        depth = depth.max(0);
    }
    names
}

/// `text` with every `"…"` literal reduced to `""`, so brace counting and `fn`
/// detection never trip over prose inside a format string — a refusal message
/// that contains `{}` must not shift the depth by one.
fn without_string_literals(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    for ch in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
                out.push('"');
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push('"');
            continue;
        }
        out.push(ch);
    }
    out
}
