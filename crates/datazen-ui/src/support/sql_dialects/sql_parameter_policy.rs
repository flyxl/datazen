//! Port of `packages/frontend/src/lib/sqlDialects/sqlParameterPolicy.ts`.
//!
//! Bind-parameter policy resolution: which placeholder syntaxes the bind
//! scanner accepts for the selected driver. The decision table is the
//! driver's own registry metadata (`sqlParameterPolicy` / the profile's
//! `parameterPolicy`), not the dialect family defaults — a driver that
//! declares nothing native still gets the one shared form (template
//! placeholders), because `${name}` interpolation is a client-side
//! feature no driver can veto.
//!
//! ## What maps where
//!
//! - React's `DB_REGISTRY` default argument is a host seam (generated from
//!   driver manifests, segment 2's T450 — the same ruling
//!   `dialect_profile.rs` records). The caller passes the registry the
//!   host resolved; `resolve_sql_parameter_policy(None, &empty)` is
//!   exactly "no driver selected".
//! - `SqlParamDialectPolicy` — the resolved, every-flag-decided form —
//!   belongs to `lib/sqlBindParams.ts` (segment 2, T605). It is defined
//!   here because this file produces it; T605 absorbs it when the
//!   scanner lands, keeping the name and fields.
//! - `SqlParameterOccurrence` / `SqlParameterStrategy` are
//!   extension-points' candidate-token contract (`sqlDialectTypes.ts`).
//!   The strategy closure is carried opaque (the scanner never runs
//!   here); `Rc::ptr_eq` is the React `toEqual` identity the tests pin.
//! - The two scanner-dependent React tests (MySQL `@session_var`, the
//!   PostgreSQL JSON `?` operator) resolve their policies here and assert
//!   the flags; the scan itself is T605's, as is `parseSqlParams`.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use super::dialect_profile::DeclaredParameterPolicy;

/// extension-points `SqlParameterOccurrence['syntax']` — the six placeholder
/// forms the shared scanner recognizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlParamSyntax {
    Colon,
    At,
    DollarPositional,
    DollarNamed,
    Question,
    Template,
}

/// extension-points `SqlParameterOccurrence` — a candidate bind token with
/// its span. Carried so [`SqlParameterStrategy`] is nameable; T605's
/// scanner emits it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlParameterOccurrence {
    pub from: usize,
    pub to: usize,
    pub id: String,
    pub token: String,
    pub syntax: SqlParamSyntax,
    pub name: String,
    /// 1-based ordinal for `?` placeholders.
    pub ordinal: Option<u32>,
}

/// extension-points `SqlParameterStrategy` — optional driver-owned
/// filtering for syntax that is ambiguous in its dialect.
#[derive(Clone)]
pub struct SqlParameterStrategy {
    pub filter_occurrences:
        Rc<dyn Fn(&str, &[SqlParameterOccurrence]) -> Vec<SqlParameterOccurrence>>,
}

impl SqlParameterStrategy {
    /// The identity filter React's test uses as "a strategy the driver
    /// bound" — and the shape of a driver with no disambiguation to do.
    pub fn passthrough() -> Self {
        Self { filter_occurrences: Rc::new(|_, occurrences| occurrences.to_vec()) }
    }

    /// Reference identity — what React's `toEqual` checks for function
    /// values: the same binding passes, a look-alike closure fails.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.filter_occurrences, &other.filter_occurrences)
    }
}

impl fmt::Debug for SqlParameterStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SqlParameterStrategy(filter_occurrences)")
    }
}

/// sqlBindParams `SqlParamDialectPolicy` — the resolved scanner policy
/// (T605 owns that file and will absorb this type verbatim).
#[derive(Clone, Default)]
pub struct SqlParamDialectPolicy {
    /// Recognize `@name`; driver metadata decides whether this syntax is native.
    pub enable_at: bool,
    /// Recognize `?`; driver metadata decides whether this syntax is native.
    pub enable_question: bool,
    /// Recognize `$1` placeholders.
    pub enable_dollar_positional: bool,
    /// Recognize `$name` placeholders.
    pub enable_dollar_named: bool,
    /// Recognize `${name}` template placeholders.
    pub enable_template: bool,
    /// Driver-owned syntax disambiguation, carried through untouched.
    pub strategy: Option<SqlParameterStrategy>,
}

impl PartialEq for SqlParamDialectPolicy {
    fn eq(&self, other: &Self) -> bool {
        // React `toEqual`: flags by value, the strategy by identity.
        self.enable_at == other.enable_at
            && self.enable_question == other.enable_question
            && self.enable_dollar_positional == other.enable_dollar_positional
            && self.enable_dollar_named == other.enable_dollar_named
            && self.enable_template == other.enable_template
            && match (&self.strategy, &other.strategy) {
                (None, None) => true,
                (Some(a), Some(b)) => a.ptr_eq(b),
                _ => false,
            }
    }
}

impl fmt::Debug for SqlParamDialectPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqlParamDialectPolicy")
            .field("enable_at", &self.enable_at)
            .field("enable_question", &self.enable_question)
            .field("enable_dollar_positional", &self.enable_dollar_positional)
            .field("enable_dollar_named", &self.enable_dollar_named)
            .field("enable_template", &self.enable_template)
            .field("strategy", &self.strategy)
            .finish()
    }
}

/// React `GENERIC_SQL_PARAMETER_POLICY` — what a driver with no native
/// declaration resolves to: only the shared template form.
pub const GENERIC_SQL_PARAMETER_POLICY: SqlParamDialectPolicy = SqlParamDialectPolicy {
    enable_at: false,
    enable_question: false,
    enable_dollar_positional: false,
    enable_dollar_named: false,
    enable_template: true,
    strategy: None,
};

/// React `SqlParameterMeta` — the three optional declarations a driver's
/// registry entry carries. `sqlParameterPolicy` is the focused form;
/// `sqlDialectProfile.parameterPolicy` is the same flags reached through
/// the profile declaration (React's `Pick<SqlDialectProfile,
/// 'parameterPolicy'>`).
pub struct SqlParameterMeta {
    pub sql_parameter_policy: Option<DeclaredParameterPolicy>,
    pub sql_dialect_profile_parameter_policy: Option<DeclaredParameterPolicy>,
    pub sql_parameter_strategy: Option<SqlParameterStrategy>,
}

impl Default for SqlParameterMeta {
    fn default() -> Self {
        Self {
            sql_parameter_policy: None,
            sql_dialect_profile_parameter_policy: None,
            sql_parameter_strategy: None,
        }
    }
}

impl SqlParameterMeta {
    /// `meta?.sqlParameterPolicy ?? meta?.sqlDialectProfile?.parameterPolicy`
    /// — the focused policy wins, the profile's is the fallback.
    fn native_policy(&self) -> Option<DeclaredParameterPolicy> {
        self.sql_parameter_policy.or(self.sql_dialect_profile_parameter_policy)
    }
}

/// The registry the host resolved (`DB_REGISTRY`'s `Record<string,
/// SqlParameterMeta>`; a `BTreeMap` so test expectations are
/// order-independent).
pub type SqlParameterRegistry = BTreeMap<String, SqlParameterMeta>;

/// React `resolveSqlParameterPolicy`.
///
/// The key is the trimmed, lowercased `databaseType` (empty or absent =
/// no driver selected); a driver that declares no native policy resolves
/// to the generic form with its strategy still carried, and a driver that
/// does declare one maps each flag through `?? false` (an omitted flag
/// means "not accepted").
pub fn resolve_sql_parameter_policy(
    database_type: Option<&str>,
    registry: &SqlParameterRegistry,
) -> SqlParamDialectPolicy {
    // `const key = databaseType?.trim().toLowerCase();`
    let key = database_type
        .map(str::trim)
        .map(str::to_lowercase)
        .filter(|k| !k.is_empty());
    // `const meta = key ? registry[key] : undefined;`
    let meta = key.as_deref().and_then(|k| registry.get(k));
    // `const nativePolicy = meta?.sqlParameterPolicy
    //     ?? meta?.sqlDialectProfile?.parameterPolicy;`
    let native_policy = meta.and_then(SqlParameterMeta::native_policy);

    match native_policy {
        None => SqlParamDialectPolicy {
            // `{ ...GENERIC_SQL_PARAMETER_POLICY, strategy: ... }`
            enable_at: GENERIC_SQL_PARAMETER_POLICY.enable_at,
            enable_question: GENERIC_SQL_PARAMETER_POLICY.enable_question,
            enable_dollar_positional: GENERIC_SQL_PARAMETER_POLICY.enable_dollar_positional,
            enable_dollar_named: GENERIC_SQL_PARAMETER_POLICY.enable_dollar_named,
            enable_template: GENERIC_SQL_PARAMETER_POLICY.enable_template,
            strategy: meta.and_then(|m| m.sql_parameter_strategy.clone()),
        },
        Some(policy) => SqlParamDialectPolicy {
            enable_at: policy.at_named.unwrap_or(false),
            enable_question: policy.question.unwrap_or(false),
            enable_dollar_positional: policy.dollar_positional.unwrap_or(false),
            enable_dollar_named: policy.dollar_named.unwrap_or(false),
            enable_template: policy.template.unwrap_or(false),
            strategy: meta.and_then(|m| m.sql_parameter_strategy.clone()),
        },
    }
}
