//! The vocabulary every other tier of the template speaks: what a driver
//! *declares* about a capability, and the verdict that declaration implies.
//!
//! Split out from the template root because a driver binding only ever needs
//! these three names (`use support::{Capability, Contract, Dialect}`), and while
//! they lived beside the tests they were impossible to find without reading all
//! of it. Nothing here runs a test: it is the declared contract, plus the
//! fail-closed mapping from a declaration to a verdict.
//!
//! `Contract::availability` is the strict gate the live tier checks before it
//! touches a server: both target names must be dedicated fixture databases and
//! must differ, so a developer with nothing installed gets a skip with a reason
//! instead of a run against whatever happens to be there.

#![allow(dead_code)]

use datazen_driver_api::DdlAtomicity;

use super::{optional_env, required_env, FIXTURE_PREFIX};

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
    pub fn is_present(self) -> bool {
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
pub fn verdict_for(capability: Capability, refusal: Verdict) -> Verdict {
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
    /// The A/B marker table name used by the **free tier**, which never touches a
    /// server. Live cases must not use it: every live case in a crate runs
    /// concurrently against the same two fixture targets, so a table name fixed in
    /// the dialect would make them create, insert into and drop *one shared*
    /// relation (two backends racing on the same `CREATE TABLE IF NOT EXISTS` is
    /// exactly how a live run fails for a reason that has nothing to do with the
    /// dimension under test).
    pub marker: &'static str,
    /// `{table}`-templated statements for the A/B marker table. The **same** table
    /// name is created in both targets with a **different** row value
    /// (`fake-runtime-fixtures.md` §10.2 rule 2), so the two markers stay
    /// indistinguishable by name alone.
    pub create_marker: &'static str,
    /// Carries both placeholders: `{table}` for the relation, `{marker}` for the
    /// row value.
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
    pub fn availability(&self) -> Result<(), String> {
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
