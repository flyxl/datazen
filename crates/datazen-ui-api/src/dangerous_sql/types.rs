#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlTokenKind {
    Semicolon,
    LineComment,
    BlockComment,
    SingleQuoted,
    DoubleQuoted,
    BacktickQuoted,
    BracketQuoted,
    DollarQuoted,
    OpenParen,
    CloseParen,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlToken {
    pub kind: SqlTokenKind,
    pub from: usize,
    pub to: usize,
    pub text: String,
    pub paren_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlRiskClassification {
    Read,
    Mutation,
    Unknown,
}

impl SqlRiskClassification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Mutation => "mutation",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlRiskFindingType {
    Drop,
    Truncate,
    NoWhere,
}

impl SqlRiskFindingType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Drop => "drop",
            Self::Truncate => "truncate",
            Self::NoWhere => "no-where",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlRiskFinding {
    pub kind: SqlRiskFindingType,
    pub from: usize,
    pub to: usize,
    pub statement_index: usize,
    pub verb: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SqlStatementRange {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlStatementRisk {
    pub index: usize,
    pub verb: Option<String>,
    pub classification: SqlRiskClassification,
    pub findings: Vec<SqlRiskFinding>,
    pub comment_hides_write_verb: bool,
    pub has_top_level_where: bool,
    pub strict_write: bool,
    pub range: SqlStatementRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlRiskAssessment {
    pub statements: Vec<SqlStatementRisk>,
    pub classification: SqlRiskClassification,
    pub findings: Vec<SqlRiskFinding>,
    pub has_high_risk: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigKind {
    Open,
    Close,
    Word,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigTok {
    pub kind: SigKind,
    pub word: Option<String>,
    pub from: usize,
    pub to: usize,
    pub depth: usize,
}

pub const MAIN_VERBS: &[&str] = &[
    "SELECT",
    "INSERT",
    "UPDATE",
    "DELETE",
    "MERGE",
    "UPSERT",
    "REPLACE",
    "CALL",
    "EXEC",
    "EXECUTE",
    "DO",
    "SHOW",
    "DESCRIBE",
    "DESC",
    "EXPLAIN",
    "SET",
    "USE",
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT",
    "RELEASE",
    "PREPARE",
    "DEALLOCATE",
    "ANALYZE",
    "VACUUM",
    "COPY",
    "LOAD",
    "UNLOAD",
    "HANDLER",
    "OPTIMIZE",
    "REPAIR",
    "CHECKSUM",
    "CHECK",
    "FLUSH",
    "RESET",
    "KILL",
    "SHUTDOWN",
    "CREATE",
    "ALTER",
    "DROP",
    "TRUNCATE",
    "GRANT",
    "REVOKE",
    "RENAME",
    "COMMENT",
    "LOCK",
    "UNLOCK",
    "PURGE",
];

pub const WRITE_VERBS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "MERGE", "UPSERT", "REPLACE", "CREATE", "ALTER", "DROP",
    "TRUNCATE", "GRANT", "REVOKE", "RENAME", "COPY", "LOAD", "UNLOAD", "CALL", "EXEC", "EXECUTE",
    "DO", "HANDLER", "OPTIMIZE", "REPAIR", "FLUSH", "RESET", "KILL", "SHUTDOWN", "PURGE", "VACUUM",
    "LOCK", "UNLOCK", "COMMENT",
];

pub const READ_VERBS: &[&str] = &[
    "SELECT", "SHOW", "DESCRIBE", "DESC", "EXPLAIN", "PRAGMA", "USE", "WITH", "VALUES",
];

pub const DESTRUCTIVE_VERBS: &[&str] = &["DROP", "TRUNCATE"];
pub const WHERE_NEEDS_VERBS: &[&str] = &["UPDATE", "DELETE"];
pub const FALSE_SQL_VALUES: &[&str] = &["FALSE", "OFF", "NO", "0"];
pub const BOOLEAN_SQL_VALUES: &[&str] = &["TRUE", "FALSE", "OFF", "NO", "0"];
