//! T155 audit: the `sqlParameterPolicy` port, verified against the React
//! source re-read line by line (`lib/sqlDialects/sqlParameterPolicy.ts`)
//! and the React test `__tests__/sqlParameterPolicy.test.ts` (the policy
//! halves of its five cases; the two scanner-dependent cases assert the
//! resolved flags here and leave `parseSqlParams` to T605, which owns
//! `lib/sqlBindParams.ts`).
//!
//! Tester-written: the coder's inline suites are treated as a black box;
//! every expectation below is re-derived from the React files. T155's
//! ledger also lists `lib/sqlDialects/sqlite.ts`; that file landed whole
//! with T123 (`SQLITE_DIALECT`, audited in `sql_dialect_strategies.rs`),
//! so nothing new is needed for it here.

use datazen_ui::support::sql_dialects as sd;
use datazen_ui::support::sql_dialects::dialect_profile::DeclaredParameterPolicy;
use datazen_ui::support::sql_dialects::sql_parameter_policy::{
    resolve_sql_parameter_policy, SqlParameterMeta, SqlParameterRegistry, SqlParameterStrategy,
    SqlParamDialectPolicy, GENERIC_SQL_PARAMETER_POLICY,
};

/// `DeclaredParameterPolicy` from five explicit flags (the React literal
/// shape; `None` = the flag the driver omitted).
fn policy(
    at: Option<bool>,
    question: Option<bool>,
    dollar_positional: Option<bool>,
    dollar_named: Option<bool>,
    template: Option<bool>,
) -> DeclaredParameterPolicy {
    DeclaredParameterPolicy {
        at_named: at,
        question,
        dollar_positional,
        dollar_named,
        template,
    }
}

fn flags(p: &SqlParamDialectPolicy) -> [bool; 5] {
    [p.enable_at, p.enable_question, p.enable_dollar_positional, p.enable_dollar_named, p.enable_template]
}

/// React test 1: the sqlserver entry's profile policy and the bound
/// strategy both come through, every omitted flag reading false.
#[test]
fn selected_driver_policy_and_strategy_come_through() {
    let strategy = SqlParameterStrategy::passthrough();
    let mut registry = SqlParameterRegistry::new();
    registry.insert(
        "sqlserver".into(),
        SqlParameterMeta {
            sql_dialect_profile_parameter_policy: Some(policy(
                Some(true),
                Some(false),
                Some(false),
                None,
                Some(false),
            )),
            sql_parameter_strategy: Some(strategy.clone()),
            ..SqlParameterMeta::default()
        },
    );

    let resolved = resolve_sql_parameter_policy(Some("sqlserver"), &registry);
    // toEqual: the five flags by value ...
    assert_eq!(flags(&resolved), [true, false, false, false, false]);
    // ... and `strategy` by identity (React toEqual on a function value).
    let carried = resolved.strategy.as_ref().expect("strategy carried");
    assert!(carried.ptr_eq(&strategy));
}

/// React test 2: a focused `sqlParameterPolicy` is honoured when there is
/// no full dialect profile declaration.
#[test]
fn focused_driver_policy_without_profile() {
    let mut registry = SqlParameterRegistry::new();
    registry.insert(
        "turso".into(),
        SqlParameterMeta {
            sql_parameter_policy: Some(policy(
                Some(true),
                Some(true),
                Some(false),
                Some(true),
                Some(false),
            )),
            ..SqlParameterMeta::default()
        },
    );

    let resolved = resolve_sql_parameter_policy(Some("turso"), &registry);
    assert_eq!(flags(&resolved), [true, true, false, true, false]);
    assert!(resolved.strategy.is_none());
}

/// React test 3: no declaration at all resolves to the generic form —
/// only the template placeholders, which no driver can veto.
#[test]
fn undeclared_driver_gets_only_shared_forms() {
    let resolved = resolve_sql_parameter_policy(Some("generic"), &SqlParameterRegistry::new());
    assert_eq!(flags(&resolved), [false, false, false, false, true]);
    assert!(resolved.strategy.is_none());
    // And the generic constant is that same row.
    assert_eq!(flags(&GENERIC_SQL_PARAMETER_POLICY), [false, false, false, false, true]);
}

/// The `??` chain order: the focused policy wins over the profile's; a
/// driver with no focused policy still reads the profile's.
#[test]
fn focused_policy_wins_over_profile_policy() {
    let mut registry = SqlParameterRegistry::new();
    registry.insert(
        "dual".into(),
        SqlParameterMeta {
            sql_parameter_policy: Some(policy(Some(true), None, None, None, None)),
            sql_dialect_profile_parameter_policy: Some(policy(
                Some(false),
                Some(true),
                None,
                None,
                None,
            )),
            ..SqlParameterMeta::default()
        },
    );
    let resolved = resolve_sql_parameter_policy(Some("dual"), &registry);
    // The focused policy replaced the profile's wholesale (React `??` on
    // the whole object, then per-flag `?? false`).
    assert_eq!(flags(&resolved), [true, false, false, false, false]);
}

/// The generic path still carries a bound strategy —
/// `{ ...GENERIC, strategy: meta?.sqlParameterStrategy }`.
#[test]
fn generic_flags_with_bound_strategy() {
    let strategy = SqlParameterStrategy::passthrough();
    let mut registry = SqlParameterRegistry::new();
    registry.insert(
        "plugin".into(),
        SqlParameterMeta {
            sql_parameter_strategy: Some(strategy.clone()),
            ..SqlParameterMeta::default()
        },
    );
    let resolved = resolve_sql_parameter_policy(Some("plugin"), &registry);
    assert_eq!(flags(&resolved), [false, false, false, false, true]);
    assert!(resolved
        .strategy
        .as_ref()
        .expect("strategy carried on the generic path")
        .ptr_eq(&strategy));
}

/// `const key = databaseType?.trim().toLowerCase()`: whitespace and case
/// are normalized away; no driver selected (`None`) and unknown keys read
/// as "no declaration".
#[test]
fn key_normalization_and_absent_driver() {
    let mut registry = SqlParameterRegistry::new();
    registry.insert(
        "sqlserver".into(),
        SqlParameterMeta {
            sql_parameter_policy: Some(policy(Some(true), None, None, None, None)),
            ..SqlParameterMeta::default()
        },
    );
    // Trim + case folding reach the registry entry.
    assert_eq!(
        flags(&resolve_sql_parameter_policy(Some("  SqlServer  "), &registry)),
        [true, false, false, false, false]
    );
    // A blank string is no driver at all (React's falsy-key branch).
    assert_eq!(
        flags(&resolve_sql_parameter_policy(Some("   "), &registry)),
        [false, false, false, false, true]
    );
    // `None` and unknown keys both resolve through the generic form.
    assert_eq!(
        flags(&resolve_sql_parameter_policy(None, &registry)),
        [false, false, false, false, true]
    );
    assert_eq!(
        flags(&resolve_sql_parameter_policy(Some("nosuch"), &registry)),
        [false, false, false, false, true]
    );
}

/// React test 4's policy half: MySQL declares `?` but not `@name` —
/// session variables are not editor parameters. (`parseSqlParams`
/// asserting `['question:1']` for `'SELECT @session_var, ?'` is T605's,
/// with the scanner.)
#[test]
fn mysql_policy_disables_at_names() {
    let mut registry = SqlParameterRegistry::new();
    registry.insert(
        "mysql".into(),
        SqlParameterMeta {
            sql_dialect_profile_parameter_policy: Some(policy(
                Some(false),
                Some(true),
                Some(false),
                None,
                Some(false),
            )),
            ..SqlParameterMeta::default()
        },
    );
    let resolved = resolve_sql_parameter_policy(Some("mysql"), &registry);
    assert_eq!(flags(&resolved), [false, true, false, false, false]);
}

/// React test 5's policy half: PostgreSQL declares `$1` but not `?` — the
/// JSON question operator is not a bind marker. (The
/// `"SELECT data ? 'key', $1"` → `['dollar:1']` scan is T605's.)
#[test]
fn postgresql_policy_keeps_dollar_positional_only() {
    let mut registry = SqlParameterRegistry::new();
    registry.insert(
        "postgresql".into(),
        SqlParameterMeta {
            sql_dialect_profile_parameter_policy: Some(policy(
                Some(false),
                Some(false),
                Some(true),
                None,
                Some(false),
            )),
            ..SqlParameterMeta::default()
        },
    );
    let resolved = resolve_sql_parameter_policy(Some("postgresql"), &registry);
    assert_eq!(flags(&resolved), [false, false, true, false, false]);
}

/// Mutation guards: a look-alike strategy is NOT equal (React toEqual on
/// function identity), and the policy's own equality follows that rule.
#[test]
fn policy_equality_follows_strategy_identity() {
    let base = || SqlParamDialectPolicy {
        enable_template: true,
        ..SqlParamDialectPolicy::default()
    };
    let a = SqlParamDialectPolicy {
        strategy: Some(SqlParameterStrategy::passthrough()),
        ..base()
    };
    let b = SqlParamDialectPolicy {
        strategy: Some(SqlParameterStrategy::passthrough()),
        ..base()
    };
    // Same flags, different closures: not equal.
    assert_ne!(a, b);
    // Clones share the closure: equal.
    let c = a.clone();
    assert_eq!(a, c);
    // A flag flip is not equal.
    let d = SqlParamDialectPolicy { enable_at: true, ..base() };
    assert_ne!(base(), d);
}
