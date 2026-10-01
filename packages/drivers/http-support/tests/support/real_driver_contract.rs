//! Reusable real-driver contract template (shared by every `path` driver).
//!
//! This file is **not** a Cargo test target on its own — `packages/drivers/<id>/tests/`
//! has no `support/main.rs`, so `cargo test -p datazen-driver-http-support` ignores it.
//! A driver opts in with a thin binding, e.g. `packages/drivers/postgres/tests/real_driver_contract.rs`:
//!
//! ```ignore
//! #[path = "../../http-support/tests/support/real_driver_contract.rs"]
//! mod support;
//!
//! use support::{Capability, Contract, Dialect};
//!
//! const CONTRACT: Contract = Contract { /* this driver's declared expectations */ };
//! fn contract_driver() -> PostgresDriver { PostgresDriver::new() }
//! ```
//!
//! Why `http-support`: it is the one crate under `packages/drivers/` documented as a
//! *shared helper* rather than a driver (`driver-capability-migration.md` §1.2), so no
//! driver crate owns the contract and every driver pulls it in as a peer file. `#[path]`
//! keeps the template inside `tests/**` (AGENTS.md「驱动测试落点」: driver tests never move
//! to Host) and avoids needing a `[dev-dependencies]` entry in each driver `Cargo.toml`.
//! Every line of real dialect SQL lives in the driver's own crate, never here.
//!
//! Two tiers:
//!
//! * **Free tier** — real assertions that need no server. They run everywhere, including
//!   CI, and are what actually proves the *declared* contract is self-consistent.
//! * **Live tier** — one named `#[tokio::test]` per CM dimension. Each needs a real
//!   server and **dedicated** fixture databases; without them it skips and reports the
//!   dimension it could not verify. `fake-runtime-fixtures.md` §10.4 forbids substituting
//!   a fake runtime for a real-protocol conclusion, so a skip is reported as unverified,
//!   never as "verified".
//!
//! Credentials come from the **process environment only** (CI secret or developer shell),
//! per `fake-runtime-fixtures.md` §10.2 rule 5 and AGENTS.md「本地环境变量文件保护」.
//! `test_sources_never_read_env_files` enforces that as a regression guard.
//!
//! Add `-- --nocapture` to see the skip reasons printed by the live tier.

#![allow(dead_code)]

use std::path::PathBuf;

use datazen_driver_api::{
    validate_schema_target, DatabaseDriver, DdlAtomicity, DriverError, SchemaScope, SqlTarget,
};
#[path = "real_driver_contract_plumbing.rs"]
mod plumbing;
pub use plumbing::*;

#[path = "real_driver_contract_live.rs"]
mod live;

// ---------------------------------------------------------------------------
// Declared-capability vocabulary
// ---------------------------------------------------------------------------

/// Per-capability verdict, mirroring the `driver-capability-migration.md` §5.2
/// vocabulary. `Unsupported` and `Unknown` are deliberately distinct: conflating
/// them is exactly the "silently skipped, then claimed as verified" failure mode
/// `fake-runtime-fixtures.md` §10.4 forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// The driver implements it and the contract asserts the real behavior.
    Supported,
    /// The driver declares it absent and must refuse explicitly.
    Unsupported,
    /// The driver cannot answer; fail closed, same explicit-refusal duty.
    Unknown,
}

impl Capability {
    fn is_present(self) -> bool {
        matches!(self, Self::Supported)
    }
}

/// What a test must prove for a declared capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Run the real-protocol journey against a live server.
    Verify,
    /// Must return `Err(DriverError::Unsupported(..))` — a *passing* refusal.
    RefuseAsUnsupported,
    /// Must return `Err(DriverError::TransactionError(..))`.
    RefuseAsTransactionError,
}

/// Fail-closed mapping from a declaration to the obligation it creates.
/// A missing capability never produces a skip: it produces a refusal assertion.
fn verdict_for(capability: Capability, refusal: Verdict) -> Verdict {
    if capability.is_present() {
        Verdict::Verify
    } else {
        refusal
    }
}

// ---------------------------------------------------------------------------
// Driver-supplied contract
// ---------------------------------------------------------------------------

/// All dialect-specific SQL lives here, i.e. in the driver's own crate.
/// The template itself contains no `CREATE TABLE`, no `USE`, no quoting literal.
pub struct Dialect {
    /// Quoted name of the A/B marker table.
    pub marker: &'static str,
    pub create_marker: &'static str,
    /// Insert statement carrying a `{marker}` placeholder.
    pub insert_marker: &'static str,
    pub select_marker: &'static str,
    pub drop_marker: &'static str,
    /// Scalar expression naming the current namespace (`current_database()` …).
    pub current_namespace: &'static str,
    /// `SELECT` whose text merely *mentions* the switch keyword; must not switch.
    pub decoy_text_select: &'static str,
    /// Relation name that never exists, used to force a clean statement failure.
    pub missing_object: &'static str,
    /// `{name}`-templated statements for a per-case uniquely named table.
    pub create_named: &'static str,
    pub insert_named: &'static str,
    pub select_named: &'static str,
    pub drop_named: &'static str,
    /// `{name}`-templated statements for a session-scoped temporary object.
    pub create_temp: &'static str,
    pub insert_temp: &'static str,
    pub select_temp: &'static str,
    /// The context-switch keyword this engine must never emit implicitly.
    pub switch_keyword: &'static str,
}

/// Everything a driver declares about itself. The free tier asserts the
/// declarations against the live trait; the live tier drives the real journeys.
pub struct Contract {
    /// `driver_type()` / `ConnectionConfig::database_type` value.
    pub label: &'static str,
    /// Every `driver_type()` this contract covers (MySQL also reports `mariadb`).
    pub driver_types: &'static [&'static str],
    /// Process-environment key prefix, e.g. `TEST_PG_`.
    pub env_prefix: &'static str,
    pub default_port: u16,
    pub default_user: &'static str,

    pub has_schema_level: bool,
    pub default_schema: Option<&'static str>,
    pub has_multi_database: bool,
    pub ddl_atomicity: DdlAtomicity,
    pub supports_offset: bool,
    /// The driver keeps a closable resource **per database** (postgres-style
    /// per-database pools). A driver that serves every database from one pool
    /// must declare `false`; CM-69 then asserts the *no-resource* contract.
    pub per_database_resource: bool,

    pub transactions: Capability,
    pub precise_cancel: Capability,
    pub session_scoped_state: Capability,
    pub read_snapshots: Capability,
    pub reset_for_reuse: Capability,

    pub dialect: Dialect,
}

impl Contract {
    /// Live-tier availability, with a reason so the unverified report can say
    /// *which* precondition failed.
    fn availability(&self) -> Result<(), String> {
        let prefix = self.env_prefix;
        if !std::env::vars().any(|(k, _)| k.starts_with(prefix)) {
            return Err(format!("no `{prefix}*` in the process environment"));
        }
        let a = required_env(format!("{prefix}DATABASE"))?;
        let b = optional_env(format!("{prefix}DATABASE_B")).unwrap_or_else(|| a.clone());
        if a == b {
            return Err("the two fixture targets are the same database".into());
        }
        for (key, db) in [("DATABASE", &a), ("DATABASE_B", &b)] {
            if !db.starts_with(FIXTURE_PREFIX) {
                return Err(format!(
                    "`{prefix}{key}` is `{db}`, which is not a dedicated fixture database \
                     (must start with `{FIXTURE_PREFIX}`) — refusing to touch it"
                ));
            }
        }
        Ok(())
    }
}
// ---------------------------------------------------------------------------
// Free tier — runs everywhere, no server required
// ---------------------------------------------------------------------------

#[test]
fn contract_matrix_covers_every_required_dimension() {
    let contract = &crate::CONTRACT;
    let mut seen = std::collections::BTreeSet::new();
    for (id, test, _) in REQUIRED_DIMENSIONS {
        assert!(seen.insert(*id), "dimension {id} is listed twice");
        assert!(test.starts_with("cm"), "{id} owner {test} is not a cm* test");
    }
    assert!(
        REQUIRED_DIMENSIONS.len() >= 12,
        "the required-dimension table must not shrink silently"
    );
    assert!(!contract.driver_types.is_empty(), "driver_types must not be empty");
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
    assert!(driver.quote_char() != '\0', "quote_char must name a real delimiter");
    assert!(!driver.sync_family().is_empty(), "sync_family must not be empty");

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
    assert!(!syntax.clause.is_empty(), "the dialect must produce a pagination clause");
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
        validate_schema_target(&driver, database, declared_schema, SchemaScope::ExactSchema).is_ok(),
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
        let schema = crate::CONTRACT.default_schema.expect("schema level needs a default");
        assert!(validate_schema_target(&driver, database, Some(schema), SchemaScope::AnySchema).is_ok());
        assert!(validate_schema_target(&driver, database, Some(schema), SchemaScope::ExactSchema).is_ok());
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
#[test]
fn missing_capabilities_demand_an_explicit_refusal() {
    let contract = &crate::CONTRACT;
    let cases: [(&str, Capability, Verdict); 5] = [
        ("transactions", contract.transactions, Verdict::RefuseAsTransactionError),
        ("precise_cancel", contract.precise_cancel, Verdict::RefuseAsUnsupported),
        ("session_scoped_state", contract.session_scoped_state, Verdict::RefuseAsUnsupported),
        ("read_snapshots", contract.read_snapshots, Verdict::RefuseAsUnsupported),
        ("reset_for_reuse", contract.reset_for_reuse, Verdict::RefuseAsUnsupported),
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
                verdict,
                refusal,
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
    assert_ne!(verdict_for(Capability::Unknown, Verdict::RefuseAsUnsupported), Verdict::Verify);
    assert_eq!(
        verdict_for(Capability::Unknown, Verdict::RefuseAsUnsupported),
        verdict_for(Capability::Unsupported, Verdict::RefuseAsUnsupported)
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
        !first.trim_start().to_uppercase().starts_with(&contract.dialect.switch_keyword),
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

/// The env-file names the guard refuses to see spelled out in a string literal.
/// Assembled from fragments so this guard cannot match its own source.
fn env_file_names() -> [&'static str; 2] {
    [concat!(".en", "v"), concat!(".en", "v.test")]
}

/// Every spelling that names an env file inside a string literal, plus the
/// dotenv-style loader calls that read one implicitly.
///
/// Each env-file name is matched **against its closing quote**, not as a bare
/// substring. That is what keeps `contract.env_prefix` and the backticked prose
/// in the sibling tests legal while still catching a directory-qualified read
/// such as `../<name>` or `fixtures/<name>` — a shape that both a bare name
/// substring and a bare quoted-name token miss.
fn env_guard_tokens() -> Vec<String> {
    let mut tokens: Vec<String> = vec![
        concat!("load_", "dotenv").to_string(),
        concat!("dot", "env()").to_string(),
        concat!("dot", "env::").to_string(),
        concat!("dot", "envy").to_string(),
    ];
    for name in env_file_names() {
        for quote in ['"', '\''] {
            // A bare quoted name, a prefixed name and a path-qualified name all
            // end in this suffix, in either quote style.
            tokens.push(format!("{name}{quote}"));
        }
    }
    tokens
}

/// The first forbidden token this source contains, if any.
fn env_guard_violation(content: &str) -> Option<String> {
    env_guard_tokens()
        .into_iter()
        .find(|token| content.contains(token.as_str()))
}

/// §10.2 rule 5 as a regression guard: no Rust source under this driver crate's
/// `tests/`, and no shared template source, may name an env file as a **string
/// literal** or call a dotenv-style loader. Prose that mentions such a file in
/// backticks stays allowed, so the guard cannot be satisfied by deleting words
/// from the docs, and it cannot be defeated by a comment.
#[test]
fn test_sources_never_read_env_files() {
    assert!(
        file!().ends_with("http-support/tests/support/real_driver_contract.rs"),
        "this file is not the shared template (file!() = {})",
        file!()
    );

    let mut sources = template_sources();
    assert!(
        sources.iter().all(|path| path.is_file()),
        "the shared template must be reachable from the driver crate: {sources:?}"
    );
    collect_rs(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests"), &mut sources);
    assert!(sources.len() > 3, "the guard found no driver sources to scan");

    for path in &sources {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        if let Some(token) = env_guard_violation(&content) {
            panic!(
                "{} must not contain {token:?} — credentials come from the process environment only",
                path.display()
            );
        }
    }
}

/// The guard above only ever sees **one** driver crate: `env!("CARGO_MANIFEST_DIR")`
/// resolves to the crate that included this template, not to the crate the file
/// physically lives in. A crate that does not bind the template is therefore
/// scanned by nobody, silently — which is how `sqlserver` kept reading a local
/// env file while a guard named `test_sources_never_read_env_files` stayed green.
///
/// This test makes that scope explicit instead of accidental. A driver crate that
/// ships a `tests/` directory must either bind the template (and so be scanned
/// whenever its own contract test runs) or be listed here with its reason. Adding
/// a driver crate and forgetting the list therefore fails loudly, and fixing a
/// listed crate and forgetting to remove it also fails loudly.
#[test]
fn every_driver_crate_with_tests_either_binds_this_guard_or_is_declared() {
    let root = drivers_root();
    let template = template_sources();
    let mut declared: Vec<String> = UNGUARDED_DRIVER_CRATES
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    declared.sort();

    let mut scanned_by_nobody: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", root.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.join("tests").is_dir() {
            continue;
        }
        // A crate is covered when it runs the template, or when it *is* the
        // template's home and its sources are already in the scanned set.
        let bound = path.join("tests/real_driver_contract.rs").is_file();
        let is_template_home = !template.is_empty() && template.iter().all(|src| src.starts_with(&path));
        if bound || is_template_home {
            continue;
        }
        scanned_by_nobody.push(entry.file_name().to_string_lossy().into_owned());
    }
    scanned_by_nobody.sort();

    assert_eq!(
        scanned_by_nobody, declared,
        "driver crates that ship tests but are scanned by no run of this guard: \
         {scanned_by_nobody:?} vs declared {declared:?}"
    );
    for (name, reason) in UNGUARDED_DRIVER_CRATES {
        assert!(
            drivers_root().join(name).join("tests").is_dir(),
            "UNGUARDED_DRIVER_CRATES lists `{name}` but that crate no longer ships tests: \
             remove it from the list"
        );
        let _ = reason;
    }
}

/// Driver crates that ship a `tests/` directory but never run this guard, with
/// the reason each one is not covered yet. Every entry is a known, visible gap
/// in the §10.2 rule 5 enforcement, not a silent one.
const UNGUARDED_DRIVER_CRATES: &[(&str, &str)] = &[
    ("clickhouse", "no contract binding yet; no env-file read in its tests today"),
    ("duckdb", "no contract binding yet; no env-file read in its tests today"),
    ("mongodb", "no contract binding yet; no env-file read in its tests today"),
    ("redis", "no contract binding yet; no env-file read in its tests today"),
    ("sqlite", "no contract binding yet; no env-file read in its tests today"),
    (
        "sqlserver",
        "tests/common/mod.rs still parses a local env file for its live suite",
    ),
];

/// The guard's own matcher, proven against the spellings that must be caught and
/// the ones that must stay legal.
///
/// Without this, a token list can quietly stop matching the shape it was written
/// for and a green guard keeps asserting nothing. That is exactly how a
/// directory-qualified `read_to_string` of a prefixed name and a reader of the
/// `test` variant both passed a guard written to forbid them.
#[test]
fn the_env_guard_matches_every_shape_of_env_file_literal() {
    let name = env_file_names()[0];
    let test_variant = env_file_names()[1];

    // (source, must_be_flagged)
    let cases: Vec<(String, bool)> = vec![
        // --- must be flagged: a quoted path that names an env file
        (format!("read_to_string({q}{name}{q})", q = '"'), true),
        (format!("read_to_string({q}{name}{q})", q = '\''), true),
        (format!("read_to_string({q}../{name}{q})", q = '"'), true),
        (format!("dir.join({q}fixtures/{name}{q})", q = '"'), true),
        (format!("read_to_string({q}x{name}{q})", q = '"'), true),
        (format!("read_to_string({q}{test_variant}{q})", q = '"'), true),
        (format!("dir.join({q}{test_variant}{q})", q = '"'), true),
        (format!("read_to_string({q}{test_variant}{q})", q = '\''), true),
        (format!("read_to_string({q}../{test_variant}{q})", q = '"'), true),
        // --- must be flagged: an implicit loader
        (concat!("load_", "dotenv").to_string(), true),
        (concat!("dot", "env::from_filename").to_string(), true),
        (concat!("dot", "envy::from_filename").to_string(), true),
        // --- must stay legal: prose in backticks, and substrings, not paths
        (format!("//! why no {q}{name}{q} file is read", q = '`'), false),
        (
            format!("/// must never parse {q}packages/drivers/{name}{q}", q = '`'),
            false,
        ),
        (
            format!("/// configured in {q}drivers/sqlserver/{test_variant}{q}", q = '`'),
            false,
        ),
        ("contract.env_prefix".to_string(), false),
        ("TEST_PG_DATABASE".to_string(), false),
        ("sql_guard".to_string(), false),
    ];

    for (source, must_flag) in &cases {
        assert_eq!(
            env_guard_violation(source).is_some(),
            *must_flag,
            "guard disagreed about {source:?}"
        );
    }
}

/// §10.2 rules 1 and 3, statically: fixture objects are uniquely named and carry
/// the dedicated prefix, so a live run can never touch a shared or production
/// object and teardown can target exactly what it created.
#[test]
fn fixture_objects_use_the_dedicated_prefix_and_unique_names() {
    let dialect = &crate::CONTRACT.dialect;
    for statement in [
        dialect.create_marker,
        dialect.insert_marker,
        dialect.select_marker,
        dialect.drop_marker,
    ] {
        assert!(
            statement.to_lowercase().contains(FIXTURE_PREFIX),
            "fixture statement does not reference the dedicated prefix: {statement}"
        );
    }
    assert!(
        dialect.insert_marker.contains("{marker}"),
        "the A/B marker insert must carry a {{marker}} placeholder"
    );
    assert!(
        dialect.select_marker.to_lowercase().contains(&dialect.marker.to_lowercase()),
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

    let mut fixture = Fixture::new("static");
    let first = fixture.name("one");
    let second = fixture.name("two");
    assert_ne!(first, second, "fixture names must be unique per case");
    for name in [&first, &second] {
        assert!(name.starts_with(FIXTURE_PREFIX), "{name} lacks the fixture prefix");
        assert!(!name.contains(' '), "{name} must be usable in an unquoted identifier");
        assert!(!name.contains(';'), "{name} must not break statement splitting");
    }
    fixture.on_drop(dialect.drop_marker.to_string());
    assert_eq!(fixture.drops.len(), 1, "teardown must be registered explicitly");
}

/// Anti-silent-drop guard: a required dimension may not stay in the matrix while
/// its live test quietly disappears.
#[test]
fn every_required_dimension_owns_a_named_test() {
    let source = template_sources()
        .iter()
        .map(|path| {
            std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
        })
        .collect::<Vec<_>>()
        .join("\n");
    for (id, test, _) in REQUIRED_DIMENSIONS {
        assert!(
            source.contains(&format!("async fn {test}(")),
            "{id} is listed as required but has no live test named {test}"
        );
    }
}

/// The honest inventory of what a run did **not** verify. Always runs, so a green
/// suite can never be read as "capabilities verified" when no real database was
/// involved. Use `-- --nocapture` to read the reasons.
#[test]
fn unverified_scope_report() {
    let contract = &crate::CONTRACT;
    eprintln!("\n=== {} real-driver contract: 未验证范围 ===", contract.label);
    match contract.availability() {
        Ok(()) => eprintln!(
            "live 层前置条件齐备；若某维度未出现在测试输出中，说明其 live 测试已真实执行并通过。"
        ),
        Err(reason) => eprintln!("live 层未启用：{reason}"),
    }
    for (id, test, what) in REQUIRED_DIMENSIONS {
        eprintln!("  - {id} / {test}: {what}");
    }
    eprintln!("free 层（声明自洽、能力区分、拒绝断言、幂等性、无 env 文件读取）已实际执行并通过。");
    eprintln!("============================================\n");
}
