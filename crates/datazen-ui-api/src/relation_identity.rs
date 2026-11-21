pub type EditorRelationKey = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlIdentifierSegment {
    pub name: String,
    pub quoted: bool,
}

impl SqlIdentifierSegment {
    pub fn new(name: impl Into<String>, quoted: bool) -> Self {
        Self {
            name: name.into(),
            quoted,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualifiedRelationId {
    pub namespace_path: Vec<SqlIdentifierSegment>,
    pub name: SqlIdentifierSegment,
}

impl QualifiedRelationId {
    pub fn bare(name: impl Into<String>) -> Self {
        Self {
            namespace_path: Vec::new(),
            name: SqlIdentifierSegment::new(name, false),
        }
    }

    pub fn qualified(namespace: &[&str], name: impl Into<String>) -> Self {
        Self {
            namespace_path: namespace
                .iter()
                .map(|segment| SqlIdentifierSegment::new(*segment, false))
                .collect(),
            name: SqlIdentifierSegment::new(name, false),
        }
    }

    fn all_segments(&self) -> impl Iterator<Item = &SqlIdentifierSegment> {
        self.namespace_path
            .iter()
            .chain(std::iter::once(&self.name))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlRelationSourceKind {
    Table,
    View,
    Cte,
    Subquery,
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

pub fn fold_segment(segment: &SqlIdentifierSegment, dialect_id: &str) -> String {
    if segment.quoted {
        segment.name.clone()
    } else {
        fold_identifier(&segment.name, dialect_id)
    }
}

pub fn relation_identity_key(identity: &QualifiedRelationId, dialect_id: &str) -> String {
    identity
        .all_segments()
        .map(|segment| fold_segment(segment, dialect_id))
        .collect::<Vec<_>>()
        .join(".")
}

pub fn build_editor_relation_key(
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

pub fn qualified_name_text(identity: &QualifiedRelationId, dialect_id: &str) -> String {
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

pub fn relation_base_name(identity: &QualifiedRelationId, dialect_id: &str) -> String {
    fold_segment(&identity.name, dialect_id)
}

pub fn has_namespace(identity: &QualifiedRelationId) -> bool {
    !identity.namespace_path.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_helpers_build_bare_and_qualified_ids() {
        let bare = QualifiedRelationId::bare("Users");
        assert!(!has_namespace(&bare));
        assert!(bare.namespace_path.is_empty());

        let qualified = QualifiedRelationId::qualified(&["db", "schema"], "users");
        assert!(has_namespace(&qualified));
        assert_eq!(qualified.all_segments().count(), 3);
        assert_eq!(qualified.name, SqlIdentifierSegment::new("users", false));
    }

    #[test]
    fn unquoted_segments_fold_by_dialect_quoted_keep_case() {
        let segment = SqlIdentifierSegment::new("Users", false);
        assert_eq!(fold_segment(&segment, "postgresql"), "users");
        assert_eq!(fold_segment(&segment, "mysql"), "users");
        assert_eq!(fold_segment(&segment, "oracle"), "USERS");
        assert_eq!(fold_segment(&segment, "sqlserver"), "Users");
        let quoted = SqlIdentifierSegment::new("Users", true);
        assert_eq!(fold_segment(&quoted, "postgresql"), "Users");
        assert_eq!(fold_segment(&quoted, "oracle"), "Users");
        assert_eq!(fold_segment(&quoted, "sqlserver"), "Users");
    }

    #[test]
    fn identity_key_folds_every_segment_and_joins_with_dots() {
        let identity = QualifiedRelationId::qualified(&["PUBLIC"], "Users");
        assert_eq!(
            relation_identity_key(&identity, "postgresql"),
            "public.users"
        );
        assert_eq!(relation_identity_key(&identity, "oracle"), "PUBLIC.USERS");
    }

    #[test]
    fn editor_key_prefixes_the_session_id() {
        let identity = QualifiedRelationId::qualified(&["PUBLIC"], "Users");
        assert_eq!(
            build_editor_relation_key("s1", &identity, "postgresql"),
            "s1::public.users"
        );
        assert_eq!(
            build_editor_relation_key("s1", &identity, "oracle"),
            "s1::PUBLIC.USERS"
        );
    }

    #[test]
    fn qualified_name_text_preserves_each_segments_quote_style() {
        let identity = QualifiedRelationId {
            namespace_path: vec![
                SqlIdentifierSegment::new("db", false),
                SqlIdentifierSegment::new("MySchema", true),
            ],
            name: SqlIdentifierSegment::new("Users", true),
        };
        assert_eq!(
            qualified_name_text(&identity, "postgresql"),
            "db.\"MySchema\".\"Users\""
        );
        assert_eq!(
            qualified_name_text(&identity, "mysql"),
            "db.`MySchema`.`Users`"
        );
        assert_eq!(
            qualified_name_text(&identity, "sqlserver"),
            "db.[MySchema].[Users]"
        );
    }

    #[test]
    fn qualified_name_text_escapes_embedded_quote_characters() {
        let identity = QualifiedRelationId {
            namespace_path: Vec::new(),
            name: SqlIdentifierSegment::new("a\"b", true),
        };
        assert_eq!(qualified_name_text(&identity, "postgresql"), "\"a\"\"b\"");
        let bracket = QualifiedRelationId {
            namespace_path: Vec::new(),
            name: SqlIdentifierSegment::new("a]b", true),
        };
        assert_eq!(qualified_name_text(&bracket, "sqlserver"), "[a]]b]");
    }

    #[test]
    fn bare_name_yields_a_single_unqualified_identifier() {
        let bare = QualifiedRelationId::qualified(&[], "Users");
        assert_eq!(qualified_name_text(&bare, "postgresql"), "Users");
        assert_eq!(relation_base_name(&bare, "postgresql"), "users");
        assert_eq!(relation_base_name(&bare, "oracle"), "USERS");
        assert_eq!(relation_base_name(&bare, "sqlserver"), "Users");
    }

    #[test]
    fn unknown_dialect_falls_back_to_standard_and_trims_the_id() {
        let segment = SqlIdentifierSegment::new("Users", false);
        assert_eq!(fold_segment(&segment, "  POSTGRES  "), "users");
        assert_eq!(fold_segment(&segment, ""), "users");
        assert_eq!(fold_segment(&segment, "mystery-db"), "users");
    }
}
