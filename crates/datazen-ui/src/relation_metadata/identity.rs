//! Relation identity and the canonical cache key, ported from React
//! `packages/frontend/src/lib/relationMetadata/identity.ts` and the dialect
//! folding/quoting rules it depends on (`sqlDialects/{dialectProfile,
//! identifierQuoting}.ts`).
//!
//! Keeping the derivation in one place is what lets the SQL editor and the
//! Visual Query Builder read each other's cache entries: a second implementation
//! would fold identifiers differently and silently split one physical relation
//! into two cells.

/// Canonical relation key: `dbSessionId::<normalized identity>`.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) type EditorRelationKey = String;

/// One SQL identifier segment. Unquoted segments are folded by the dialect;
/// quoted ones keep their spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct SqlIdentifierSegment {
    pub name: String,
    pub quoted: bool,
}

#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
impl SqlIdentifierSegment {
    pub fn new(name: impl Into<String>, quoted: bool) -> Self {
        Self {
            name: name.into(),
            quoted,
        }
    }
}

/// Database / schema / relation path without alias.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct QualifiedRelationId {
    pub namespace_path: Vec<SqlIdentifierSegment>,
    pub name: SqlIdentifierSegment,
}

#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
impl QualifiedRelationId {
    /// A bare, unquoted name (the common case).
    pub fn bare(name: impl Into<String>) -> Self {
        Self {
            namespace_path: Vec::new(),
            name: SqlIdentifierSegment::new(name, false),
        }
    }

    /// A name qualified by unquoted namespace segments, e.g. `public.users`.
    pub fn qualified(namespace: &[&str], name: impl Into<String>) -> Self {
        Self {
            namespace_path: namespace
                .iter()
                .map(|n| SqlIdentifierSegment::new(*n, false))
                .collect(),
            name: SqlIdentifierSegment::new(name, false),
        }
    }

    fn all_segments(&self) -> impl Iterator<Item = &SqlIdentifierSegment> {
        self.namespace_path.iter().chain(std::iter::once(&self.name))
    }
}

/// How the semantic model reported a relation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) enum SqlRelationSourceKind {
    Table,
    View,
    Cte,
    Subquery,
}

/// A loadable relation kind (`cte`/`subquery` are filtered out before loading).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) enum EditorRelationKind {
    Table,
    View,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FoldCase {
    Lower,
    Upper,
    Preserve,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuoteStyle {
    None,
    Double,
    Backtick,
    Bracket,
}

#[derive(Clone, Copy, Debug)]
struct DialectProfile {
    fold_case: FoldCase,
    quote_style: QuoteStyle,
}

const STANDARD: DialectProfile = DialectProfile {
    fold_case: FoldCase::Lower,
    quote_style: QuoteStyle::Double,
};
const SQLITE: DialectProfile = DialectProfile {
    fold_case: FoldCase::Lower,
    quote_style: QuoteStyle::Double,
};
const MYSQL: DialectProfile = DialectProfile {
    fold_case: FoldCase::Lower,
    quote_style: QuoteStyle::Backtick,
};
const SQLSERVER: DialectProfile = DialectProfile {
    fold_case: FoldCase::Preserve,
    quote_style: QuoteStyle::Bracket,
};
const DUCKDB: DialectProfile = DialectProfile {
    fold_case: FoldCase::Lower,
    quote_style: QuoteStyle::Double,
};
const CLICKHOUSE: DialectProfile = DialectProfile {
    fold_case: FoldCase::Lower,
    quote_style: QuoteStyle::Backtick,
};
const ORACLE: DialectProfile = DialectProfile {
    fold_case: FoldCase::Upper,
    quote_style: QuoteStyle::Double,
};

/// Mirrors `resolveSqlDialectProfile`: trim/lowercase the id, map aliases to
/// built-in defaults, and fall back to `standard` for unknown ids.
fn resolve_dialect_profile(dialect_id: &str) -> DialectProfile {
    let trimmed = dialect_id.trim().to_lowercase();
    let key = if trimmed.is_empty() {
        "standard"
    } else {
        trimmed.as_str()
    };
    match key {
        "standard" | "generic" | "postgresql" | "postgres" | "pg" => STANDARD,
        "sqlite" => SQLITE,
        "mysql" | "mariadb" => MYSQL,
        "sqlserver" | "mssql" => SQLSERVER,
        "duckdb" => DUCKDB,
        "clickhouse" => CLICKHOUSE,
        "oracle" => ORACLE,
        _ => STANDARD,
    }
}

fn fold_identifier(value: &str, dialect_id: &str) -> String {
    match resolve_dialect_profile(dialect_id).fold_case {
        FoldCase::Lower => value.to_lowercase(),
        FoldCase::Upper => value.to_uppercase(),
        FoldCase::Preserve => value.to_owned(),
    }
}

fn quote_identifier(value: &str, profile: DialectProfile) -> String {
    match profile.quote_style {
        QuoteStyle::None => value.to_owned(),
        QuoteStyle::Double => format!("\"{}\"", value.replace('"', "\"\"")),
        QuoteStyle::Backtick => format!("`{}`", value),
        QuoteStyle::Bracket => format!("[{}]", value.replace(']', "]]")),
    }
}

fn fold_segment(segment: &SqlIdentifierSegment, dialect_id: &str) -> String {
    if segment.quoted {
        segment.name.clone()
    } else {
        fold_identifier(&segment.name, dialect_id)
    }
}

/// Normalized cross-namespace identity key (no `dbSessionId`).
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn relation_identity_key(identity: &QualifiedRelationId, dialect_id: &str) -> String {
    identity
        .all_segments()
        .map(|segment| fold_segment(segment, dialect_id))
        .collect::<Vec<_>>()
        .join(".")
}

/// `dbSessionId::<normalized identity>` — the single canonical cache key.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn build_editor_relation_key(
    db_session_id: &str,
    identity: &QualifiedRelationId,
    dialect_id: &str,
) -> EditorRelationKey {
    format!(
        "{}::{}",
        db_session_id,
        relation_identity_key(identity, dialect_id)
    )
}

/// Driver-accepted qualified identifier text (preserves quote style).
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn qualified_name_text(identity: &QualifiedRelationId, dialect_id: &str) -> String {
    let profile = resolve_dialect_profile(dialect_id);
    identity
        .all_segments()
        .map(|segment| {
            if segment.quoted {
                quote_identifier(&segment.name, profile)
            } else {
                segment.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// The unqualified, normalized relation name.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn relation_base_name(identity: &QualifiedRelationId, dialect_id: &str) -> String {
    fold_segment(&identity.name, dialect_id)
}

/// Whether the relation carries an explicit namespace.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn has_namespace(identity: &QualifiedRelationId) -> bool {
    !identity.namespace_path.is_empty()
}

/// Utility export mirroring React's `editorRelationKey`.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn editor_relation_key(
    db_session_id: &str,
    identity: &QualifiedRelationId,
    dialect_id: &str,
) -> EditorRelationKey {
    build_editor_relation_key(db_session_id, identity, dialect_id)
}
