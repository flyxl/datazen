//! Port of `packages/frontend/src/lib/sqlDialects/dialectProfile.ts`.
//!
//! Dialect profile resolution: how identifiers fold and quote, which
//! placeholder syntax is accepted, how visible projection aliases are.
//! Dialect metadata, not editor state — the SQL editor and the Visual
//! Query Builder both resolve relation keys through it.
//!
//! ## What maps where
//!
//! - The resolution order is React's verbatim: the driver's own
//!   `sqlDialectProfile` declaration wins, then the dialect family's
//!   built-in default, then the standard profile; a driver declaring only
//!   some fields still replaces its family default wholesale (only the
//!   five placeholder flags default, to "not accepted").
//! - React's `DB_REGISTRY` lookup (by dialect id, then by `sqlDialect`
//!   family match) is the HOST's concern — the registry is generated from
//!   the driver manifests (`lib/databaseTypes.ts`, segment 2's T450) and
//!   does not exist in this crate. The host resolves the declared profile
//!   and passes it as [`resolve_dialect_profile`]'s `declared` argument;
//!   `None` is exactly "the registry has no declaration for this id"
//!   (which is what every built-in-only dialect resolves through).
//! - React's `profileCache` (a `Map` memo) is elided: the built-ins are
//!   consts and a declared profile is constructed once per call — the
//!   cache is memoization, not semantics.
//! - The minimal fold/quote copy `relation_metadata/identity.rs` carried
//!   since T032 is this module's first consumer seam: its private
//!   `resolve_dialect_profile` shrinks to a call here once its callers
//!   pass declarations through (the fold behaviour it pinned — the
//!   builtin table's fold-case per key — is byte-identical below).
//!
//! Known deltas: none behavioural — every profile row, key and default is
//! the React table verbatim.

/// React `quoteStyle`: 'none' | 'double' | 'backtick' | 'bracket'. No
/// built-in profile uses 'none' (a driver may declare it); identifier
/// quoting is the passthrough then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteStyle {
    None,
    Double,
    Backtick,
    Bracket,
}

/// React `foldCase`: 'lower' | 'upper' | 'preserve'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldCase {
    Lower,
    Upper,
    Preserve,
}

/// React `projectionAliasVisibility`: 'select-only' | 'order-group' | 'broad'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionAliasVisibility {
    SelectOnly,
    OrderGroup,
    Broad,
}

/// React `parameterPolicy` (all five flags; a driver that omits one means
/// "not accepted").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterPolicy {
    pub at_named: bool,
    pub question: bool,
    pub dollar_positional: bool,
    pub dollar_named: bool,
    pub template: bool,
}

/// React `ResolvedSqlDialectProfile`.
#[derive(Clone, Debug, PartialEq)]
pub struct SqlDialectProfile {
    pub quote_style: QuoteStyle,
    pub fold_case: FoldCase,
    pub projection_alias_visibility: ProjectionAliasVisibility,
    pub parameter_policy: ParameterPolicy,
    /// `reservedKeywords?: string[]` — carried, not interpreted here.
    pub reserved_keywords: Option<Vec<String>>,
}

/// React `SqlDialectProfile['parameterPolicy']` (the declared, partially
/// optional shape).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeclaredParameterPolicy {
    pub at_named: Option<bool>,
    pub question: Option<bool>,
    pub dollar_positional: Option<bool>,
    pub dollar_named: Option<bool>,
    pub template: Option<bool>,
}

/// React `SqlDialectProfile` (a driver's declaration).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeclaredSqlDialectProfile {
    pub quote_style: Option<QuoteStyle>,
    pub fold_case: Option<FoldCase>,
    pub projection_alias_visibility: Option<ProjectionAliasVisibility>,
    pub parameter_policy: DeclaredParameterPolicy,
    pub reserved_keywords: Option<Vec<String>>,
}

/// React `STANDARD_DIALECT_PROFILE` (dollar-positional only).
pub const STANDARD_DIALECT_PROFILE: SqlDialectProfile = SqlDialectProfile {
    quote_style: QuoteStyle::Double,
    fold_case: FoldCase::Lower,
    projection_alias_visibility: ProjectionAliasVisibility::SelectOnly,
    parameter_policy: ParameterPolicy {
        at_named: false,
        question: false,
        dollar_positional: true,
        dollar_named: false,
        template: false,
    },
    reserved_keywords: None,
};

const SQLITE_PROFILE: SqlDialectProfile = SqlDialectProfile {
    quote_style: QuoteStyle::Double,
    fold_case: FoldCase::Lower,
    projection_alias_visibility: ProjectionAliasVisibility::SelectOnly,
    parameter_policy: ParameterPolicy {
        at_named: false,
        question: true,
        dollar_positional: false,
        dollar_named: false,
        template: false,
    },
    reserved_keywords: None,
};

const MYSQL_PROFILE: SqlDialectProfile = SqlDialectProfile {
    quote_style: QuoteStyle::Backtick,
    fold_case: FoldCase::Lower,
    projection_alias_visibility: ProjectionAliasVisibility::OrderGroup,
    parameter_policy: ParameterPolicy {
        at_named: false,
        question: true,
        dollar_positional: false,
        dollar_named: false,
        template: false,
    },
    reserved_keywords: None,
};

const SQLSERVER_PROFILE: SqlDialectProfile = SqlDialectProfile {
    quote_style: QuoteStyle::Bracket,
    fold_case: FoldCase::Preserve,
    projection_alias_visibility: ProjectionAliasVisibility::Broad,
    parameter_policy: ParameterPolicy {
        at_named: true,
        question: false,
        dollar_positional: false,
        dollar_named: false,
        template: false,
    },
    reserved_keywords: None,
};

const DUCKDB_PROFILE: SqlDialectProfile = SqlDialectProfile {
    quote_style: QuoteStyle::Double,
    fold_case: FoldCase::Lower,
    projection_alias_visibility: ProjectionAliasVisibility::SelectOnly,
    parameter_policy: ParameterPolicy {
        at_named: false,
        question: true,
        dollar_positional: true,
        dollar_named: false,
        template: false,
    },
    reserved_keywords: None,
};

const CLICKHOUSE_PROFILE: SqlDialectProfile = SqlDialectProfile {
    quote_style: QuoteStyle::Backtick,
    fold_case: FoldCase::Lower,
    projection_alias_visibility: ProjectionAliasVisibility::Broad,
    parameter_policy: ParameterPolicy {
        at_named: false,
        question: false,
        dollar_positional: false,
        dollar_named: false,
        template: false,
    },
    reserved_keywords: None,
};

const ORACLE_PROFILE: SqlDialectProfile = SqlDialectProfile {
    quote_style: QuoteStyle::Double,
    fold_case: FoldCase::Upper,
    projection_alias_visibility: ProjectionAliasVisibility::Broad,
    parameter_policy: ParameterPolicy {
        at_named: false,
        question: false,
        dollar_positional: false,
        dollar_named: false,
        template: false,
    },
    reserved_keywords: None,
};

/// The built-in default for a dialect id *or* family alias (the exact keys
/// of React's `DIALECT_PROFILES` table).
pub fn builtin_dialect_profile(key: &str) -> Option<&'static SqlDialectProfile> {
    match key {
        "standard" | "generic" | "postgresql" | "postgres" | "pg" => Some(&STANDARD_DIALECT_PROFILE),
        "sqlite" => Some(&SQLITE_PROFILE),
        "mysql" | "mariadb" => Some(&MYSQL_PROFILE),
        "sqlserver" | "mssql" => Some(&SQLSERVER_PROFILE),
        "duckdb" => Some(&DUCKDB_PROFILE),
        "clickhouse" => Some(&CLICKHOUSE_PROFILE),
        "oracle" => Some(&ORACLE_PROFILE),
        _ => None,
    }
}

/// React `normalizeDeclaredProfile`: the declaration replaces the family
/// default wholesale; only the five placeholder flags default (to false —
/// a driver that omits one means "not accepted").
pub fn normalize_declared_profile(declared: &DeclaredSqlDialectProfile) -> SqlDialectProfile {
    SqlDialectProfile {
        // The React type makes the non-policy fields required; the Option
        // here is the Rust projection seam — a missing field resolves the
        // way the host's registry validation would (it never sees one).
        quote_style: declared.quote_style.unwrap_or(QuoteStyle::Double),
        fold_case: declared.fold_case.unwrap_or(FoldCase::Lower),
        projection_alias_visibility: declared
            .projection_alias_visibility
            .unwrap_or(ProjectionAliasVisibility::SelectOnly),
        parameter_policy: ParameterPolicy {
            at_named: declared.parameter_policy.at_named.unwrap_or(false),
            question: declared.parameter_policy.question.unwrap_or(false),
            dollar_positional: declared.parameter_policy.dollar_positional.unwrap_or(false),
            dollar_named: declared.parameter_policy.dollar_named.unwrap_or(false),
            template: declared.parameter_policy.template.unwrap_or(false),
        },
        reserved_keywords: declared.reserved_keywords.clone(),
    }
}

/// React `resolveSqlDialectProfile`, with the registry lookup lifted to the
/// caller: `declared` is the host's resolved `sqlDialectProfile`
/// declaration for the dialect (by id, or by family match); `None` means
/// the registry has none. Trim/lowercase the id, empty = `standard`,
/// built-in table, standard fallback.
pub fn resolve_dialect_profile(
    dialect_id: &str,
    declared: Option<&DeclaredSqlDialectProfile>,
) -> SqlDialectProfile {
    let trimmed = dialect_id.trim().to_lowercase();
    let key = if trimmed.is_empty() { "standard" } else { trimmed.as_str() };
    match declared {
        Some(declared) => normalize_declared_profile(declared),
        None => builtin_dialect_profile(key)
            .cloned()
            .unwrap_or(STANDARD_DIALECT_PROFILE),
    }
}

/// React `listBuiltinDialectProfileIds`, in the table's declaration order.
pub fn list_builtin_dialect_profile_ids() -> &'static [&'static str] {
    &[
        "standard",
        "generic",
        "postgresql",
        "postgres",
        "pg",
        "sqlite",
        "mysql",
        "mariadb",
        "sqlserver",
        "mssql",
        "duckdb",
        "clickhouse",
        "oracle",
    ]
}

/// React `foldIdentifierForDialect` (the host passes the declaration it
/// resolved for the dialect, or `None` for the built-in path).
pub fn fold_identifier_for_dialect(
    value: &str,
    dialect_id: &str,
    declared: Option<&DeclaredSqlDialectProfile>,
) -> String {
    let profile = resolve_dialect_profile(dialect_id, declared);
    fold_identifier(value, &profile)
}

/// Fold an unquoted identifier the way the dialect does (the primitive
/// lives in [`super::identifier_quoting`]; this is the profile-resolved
/// spelling React's callers reach through).
pub fn fold_identifier(value: &str, profile: &SqlDialectProfile) -> String {
    super::identifier_quoting::fold_identifier(value, profile.fold_case)
}
