//! Dialect-neutral schema migration contracts exposed by the driver API.

use crate::schema_objects::ObjectKind;
use crate::schema_scope_mapping::{SchemaObjectScopeDependency, SchemaObjectScopeMapping};
use crate::{CheckConstraint, ColumnSchema, ForeignKeyInfo, IndexInfo, TableOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_auto_increment: bool,
}

/// A view definition captured from a source database.
///
/// `definition` is the query body returned by the driver's object metadata
/// API. It excludes the `CREATE VIEW ... AS` wrapper so the target renderer
/// can quote the target identifier and choose dialect syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationView {
    pub schema: Option<String>,
    pub name: String,
    pub definition: String,
}

/// A routine definition captured from the source database. `definition` is
/// driver-owned DDL (for example `pg_get_functiondef` or SHOW CREATE) and is
/// never synthesized by the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationRoutine {
    pub kind: ObjectKind,
    pub schema: Option<String>,
    pub name: String,
    pub signature: Option<String>,
    pub definition: String,
}

/// A trigger definition plus the relation it is attached to. Trigger names
/// are not globally unique on every supported engine, so the target relation
/// is part of the reviewed identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationTrigger {
    pub schema: Option<String>,
    pub name: String,
    pub target_schema: Option<String>,
    pub target_name: String,
    pub definition: String,
}

/// A PostgreSQL sequence definition captured from the server catalog. The
/// definition contains only the catalog-generated CREATE statement and an
/// optional OWNED BY statement; callers never supply it from the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationSequence {
    pub schema: Option<String>,
    pub name: String,
    pub definition: String,
}

/// A user-defined database type definition captured from the driver catalog.
///
/// The definition is a complete, driver-owned CREATE TYPE/CREATE DOMAIN
/// statement. The host compares identities and delegates rendering; it never
/// synthesizes type clauses or translates definitions between dialects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationType {
    pub schema: Option<String>,
    pub name: String,
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOperation {
    CreateTable {
        table: String,
        columns: Vec<MigrationColumn>,
        primary_keys: Vec<String>,
    },
    /// Remove a table without a cascade clause.
    ///
    /// A table drop is intentionally a first-class reviewed operation. The
    /// host marks it destructive and does not synthesize rollback SQL because
    /// recreating a table cannot restore its rows, indexes, or constraints.
    DropTable {
        table: String,
    },
    AddColumn {
        table: String,
        column: MigrationColumn,
    },
    DropColumn {
        table: String,
        column: MigrationColumn,
    },
    AlterColumnType {
        table: String,
        column: String,
        from: String,
        to: String,
    },
    SetNullable {
        table: String,
        column: String,
        nullable: bool,
    },
    SetDefault {
        table: String,
        column: String,
        from: Option<String>,
        to: Option<String>,
    },
    SetComment {
        table: String,
        column: String,
        from: Option<String>,
        to: Option<String>,
    },
    /// Change table-level metadata that the driver can represent without
    /// inventing a dialect translation. MySQL currently supports the
    /// comment, storage engine, and default character set fields.
    SetTableOptions {
        table: String,
        from: TableOptions,
        to: TableOptions,
    },
    SetAutoIncrement {
        table: String,
        column: String,
        from: bool,
        to: bool,
    },
    AddPrimaryKey {
        table: String,
        columns: Vec<String>,
    },
    DropPrimaryKey {
        table: String,
        columns: Vec<String>,
    },
    CreateIndex {
        table: String,
        index: IndexInfo,
    },
    DropIndex {
        table: String,
        index: IndexInfo,
    },
    AddForeignKey {
        table: String,
        foreign_key: ForeignKeyInfo,
    },
    DropForeignKey {
        table: String,
        foreign_key: ForeignKeyInfo,
    },
    AddCheckConstraint {
        table: String,
        constraint: CheckConstraint,
    },
    DropCheckConstraint {
        table: String,
        constraint: CheckConstraint,
    },
    CreateView {
        view: MigrationView,
    },
    ReplaceView {
        current: MigrationView,
        desired: MigrationView,
    },
    DropView {
        view: MigrationView,
    },
    CreateRoutine {
        routine: MigrationRoutine,
    },
    ReplaceRoutine {
        current: MigrationRoutine,
        desired: MigrationRoutine,
    },
    DropRoutine {
        routine: MigrationRoutine,
    },
    CreateTrigger {
        trigger: MigrationTrigger,
    },
    ReplaceTrigger {
        current: MigrationTrigger,
        desired: MigrationTrigger,
    },
    DropTrigger {
        trigger: MigrationTrigger,
    },
    CreateSequence {
        sequence: MigrationSequence,
    },
    /// Replacing a sequence resets mutable counter state. Renderers must mark
    /// it destructive and must not claim a complete rollback.
    ReplaceSequence {
        current: MigrationSequence,
        desired: MigrationSequence,
    },
    DropSequence {
        sequence: MigrationSequence,
    },
    CreateType {
        type_definition: MigrationType,
    },
    ReplaceType {
        current: MigrationType,
        desired: MigrationType,
    },
    DropType {
        type_definition: MigrationType,
    },
}

/// Validate and trim a relation identifier used by a reviewed migration
/// statement. A qualified relation may contain dots between non-empty
/// segments, but whitespace around a segment or control characters would make
/// the target relation ambiguous and must fail closed.
pub fn validate_migration_identifier(raw: &str) -> Result<&str, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("migration identifier must not be empty".into());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("migration identifier contains control characters".into());
    }
    if trimmed
        .split('.')
        .any(|segment| segment.is_empty() || segment != segment.trim())
    {
        return Err("migration identifier contains an empty or whitespace-padded segment".into());
    }
    Ok(trimmed)
}

/// Validate a query body before it is embedded into one reviewed DDL
/// statement. A single trailing statement terminator is allowed, while an
/// interior terminator is rejected so a source object cannot turn a reviewed
/// operation into an arbitrary script.
pub fn validate_view_definition(definition: &str) -> Result<(), String> {
    let trimmed = definition.trim();
    if trimmed.is_empty() {
        return Err("view definition must not be empty".into());
    }
    if has_non_trailing_statement_terminator(trimmed) {
        return Err("view definition must contain one query".into());
    }
    if trimmed
        .chars()
        .any(|ch| ch == '\0' || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')))
    {
        return Err("view definition contains control characters".into());
    }
    Ok(())
}

/// Validate a complete routine/trigger DDL payload returned by a driver.
/// Routine bodies may legitimately contain semicolons, so this validates only
/// the executable CREATE envelope rather than trying to parse procedural SQL.
pub fn validate_object_definition(
    definition: &str,
    kind: ObjectKind,
    name: &str,
) -> Result<(), String> {
    validate_object_definition_with_identity(definition, kind, name, None)
}

/// Validate an object definition and, when supplied, its routine identity
/// arguments. A routine's name/signature are extracted from the declaration
/// header; body text and comments are never considered identity evidence.
pub fn validate_object_definition_with_identity(
    definition: &str,
    kind: ObjectKind,
    name: &str,
    signature: Option<&str>,
) -> Result<(), String> {
    let trimmed = definition.trim();
    if trimmed.is_empty() {
        return Err("schema object definition must not be empty".into());
    }
    if trimmed
        .chars()
        .any(|ch| ch == '\0' || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')))
    {
        return Err("schema object definition contains control characters".into());
    }
    let kind_token = match kind {
        ObjectKind::Function => "FUNCTION",
        ObjectKind::Procedure => "PROCEDURE",
        ObjectKind::Trigger => "TRIGGER",
        _ => return Err("schema object kind is not a routine or trigger".into()),
    };
    let Some((declared_kind, declared_name, declared_signature)) =
        object_declaration_identity(trimmed)
    else {
        return Err(format!(
            "schema object definition must declare CREATE {kind_token}"
        ));
    };
    if declared_kind != kind {
        return Err(format!(
            "schema object definition must declare CREATE {kind_token}"
        ));
    }
    if name.trim().is_empty() {
        return Err("schema object name must not be empty".into());
    }
    if normalize_identifier(&declared_name) != normalize_identifier(name) {
        return Err(format!(
            "schema object definition declares `{declared_name}` instead of `{name}`"
        ));
    }
    if let Some(signature) = signature {
        if !matches!(kind, ObjectKind::Function | ObjectKind::Procedure) {
            return Err("routine signature supplied for a non-routine object".into());
        }
        let Some(declared_signature) = declared_signature else {
            return Err("routine declaration arguments could not be verified".into());
        };
        if !routine_signatures_match(&declared_signature, signature) {
            return Err(format!(
                "routine declaration signature `{declared_signature}` does not match requested signature `{signature}`"
            ));
        }
    }
    Ok(())
}

/// Validate PostgreSQL sequence DDL returned by the catalog before a renderer
/// can include it in a reviewed migration plan. Sequence DDL has a narrow,
/// known shape: a CREATE SEQUENCE statement with an optional ALTER SEQUENCE
/// ... OWNED BY statement. Rejecting any other script shape keeps a stale or
/// malicious catalog payload from becoming arbitrary SQL.
pub fn validate_sequence_definition_with_identity(
    definition: &str,
    schema: Option<&str>,
    name: &str,
) -> Result<(), String> {
    validate_migration_identifier(name)?;
    if let Some(schema) = schema.filter(|schema| !schema.is_empty()) {
        validate_migration_identifier(schema)?;
    }
    let trimmed = definition.trim();
    if trimmed.is_empty() {
        return Err("sequence definition must not be empty".into());
    }
    if trimmed
        .chars()
        .any(|ch| ch == '\0' || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')))
    {
        return Err("sequence definition contains control characters".into());
    }
    validate_sequence_definition_safety(trimmed)?;
    let statements = split_sequence_statements(trimmed)?;
    if statements.is_empty() || statements.len() > 2 {
        return Err(
            "sequence definition must contain CREATE and at most one OWNED BY statement".into(),
        );
    }
    let create_tokens = executable_sql_tokens(statements[0]);
    let mut index = 0;
    expect_word(&create_tokens, &mut index, "CREATE")?;
    expect_word(&create_tokens, &mut index, "SEQUENCE")?;
    validate_sequence_identity(&create_tokens, &mut index, schema, name)?;
    validate_sequence_options(&create_tokens[index..])?;

    if let Some(owned_by) = statements.get(1) {
        let tokens = executable_sql_tokens(owned_by);
        let mut index = 0;
        expect_word(&tokens, &mut index, "ALTER")?;
        expect_word(&tokens, &mut index, "SEQUENCE")?;
        validate_sequence_identity(&tokens, &mut index, schema, name)?;
        expect_word(&tokens, &mut index, "OWNED")?;
        expect_word(&tokens, &mut index, "BY")?;
        let owner = consume_qualified_identifier(&tokens, &mut index)
            .ok_or("sequence OWNED BY relation is missing")?;
        if owner.len() != 3 || index != tokens.len() {
            return Err(
                "sequence OWNED BY relation must be schema-qualified table column identity".into(),
            );
        }
    }
    Ok(())
}

/// Validate catalog-generated PostgreSQL sequence DDL and return the CREATE
/// statement plus its optional exact `OWNED BY` relation identity. The Host
/// may use the identity to order reviewed operations, but SQL must still be
/// rendered by the database driver.
// 返回类型是刻意设计：这四元组逐项对应「CREATE 语句 / 可选 `OWNED BY` 身份 / 可选三元组身份 / 可选校验错误」，
// 是 schema diff 与 Host 复核流程按位置消费的既有形状。换成命名结构体或 type 别名属于契约变更，
// 会动到 Wave 1/Wave 2 正在依赖的调用点。
#[allow(clippy::type_complexity)]
pub fn split_sequence_definition(
    definition: &str,
    schema: Option<&str>,
    name: &str,
) -> Result<
    (
        String,
        Option<String>,
        Option<(String, String, String)>,
        Option<String>,
    ),
    String,
> {
    validate_sequence_definition_with_identity(definition, schema, name)?;
    let statements = split_sequence_statements(definition.trim())?;
    let create_statement = statements
        .first()
        .ok_or("sequence definition must contain CREATE SEQUENCE")?
        .trim()
        .to_owned();
    let ownership = statements
        .get(1)
        .map(|statement| parse_sequence_ownership(statement, schema, name))
        .transpose()?;
    let ownership_statement = statements
        .get(1)
        .map(|statement| statement.trim().to_owned());
    let ownership_reset_statement = statements
        .get(1)
        .map(|statement| ownership_reset_statement(statement))
        .transpose()?;
    Ok((
        create_statement,
        ownership_statement,
        ownership,
        ownership_reset_statement,
    ))
}

fn ownership_reset_statement(statement: &str) -> Result<String, String> {
    let bytes = statement.as_bytes();
    let mut tokens: Vec<(Option<String>, usize)> = Vec::new();
    let mut index = 0;
    let mut quoted = false;
    while index < bytes.len() {
        if quoted {
            if bytes[index] == b'"' {
                if bytes.get(index + 1) == Some(&b'"') {
                    index += 2;
                    continue;
                }
                quoted = false;
            }
            index += 1;
            continue;
        }
        match bytes[index] {
            b'"' => {
                quoted = true;
                tokens.push((None, index));
                index += 1;
            }
            byte if byte.is_ascii_whitespace() => index += 1,
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'$'))
                {
                    index += 1;
                }
                tokens.push((Some(statement[start..index].to_ascii_uppercase()), index));
            }
            _ => {
                tokens.push((None, index));
                index += 1;
            }
        }
    }
    let by_end = tokens
        .windows(2)
        .find_map(|pair| match (&pair[0].0, &pair[1].0) {
            (Some(owned), Some(by)) if owned == "OWNED" && by == "BY" => Some(pair[1].1),
            _ => None,
        })
        .ok_or("validated sequence ownership clause is missing OWNED BY")?;
    Ok(format!("{} NONE", statement[..by_end].trim_end()))
}

fn parse_sequence_ownership(
    statement: &str,
    schema: Option<&str>,
    name: &str,
) -> Result<(String, String, String), String> {
    let tokens = executable_sql_tokens(statement);
    let mut index = 0;
    expect_word(&tokens, &mut index, "ALTER")?;
    expect_word(&tokens, &mut index, "SEQUENCE")?;
    validate_sequence_identity(&tokens, &mut index, schema, name)?;
    expect_word(&tokens, &mut index, "OWNED")?;
    expect_word(&tokens, &mut index, "BY")?;
    let owner = consume_qualified_identifier(&tokens, &mut index)
        .ok_or("sequence OWNED BY relation is missing")?;
    if owner.len() != 3 || index != tokens.len() {
        return Err(
            "sequence OWNED BY relation must be schema-qualified table column identity".into(),
        );
    }
    let mut owner = owner
        .into_iter()
        .map(sequence_identifier_value)
        .collect::<Option<Vec<_>>>()
        .ok_or("sequence OWNED BY relation contains an invalid identifier")?;
    Ok((owner.remove(0), owner.remove(0), owner.remove(0)))
}

fn sequence_identifier_value(token: ExecutableSqlToken) -> Option<String> {
    match token {
        ExecutableSqlToken::Word(value) => Some(value.to_ascii_lowercase()),
        ExecutableSqlToken::Identifier(value) => Some(value),
        ExecutableSqlToken::Symbol(_) => None,
    }
}

/// Validate a driver-owned user-defined type definition before it becomes a
/// reviewed migration statement. Only one CREATE TYPE/CREATE DOMAIN statement
/// is accepted, with an optional trailing semicolon. The declaration identity
/// must match the catalog identity so a stale or unrelated DDL payload cannot
/// be applied to another type.
pub fn validate_type_definition_with_identity(
    definition: &str,
    schema: Option<&str>,
    name: &str,
) -> Result<(), String> {
    validate_migration_identifier(name)?;
    if let Some(schema) = schema.filter(|schema| !schema.is_empty()) {
        validate_migration_identifier(schema)?;
    }
    let trimmed = definition.trim();
    if trimmed.is_empty() {
        return Err("type definition must not be empty".into());
    }
    if trimmed
        .chars()
        .any(|ch| ch == '\0' || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')))
    {
        return Err("type definition contains control characters".into());
    }
    if has_non_trailing_statement_terminator(trimmed) {
        return Err("type definition must contain one CREATE TYPE statement".into());
    }

    let tokens = executable_sql_tokens(trimmed.trim_end_matches(';').trim());
    if tokens.first().and_then(ExecutableSqlToken::as_word) != Some("CREATE") {
        return Err("type definition must declare CREATE TYPE or CREATE DOMAIN".into());
    }
    let declaration = tokens.get(1).and_then(ExecutableSqlToken::as_word);
    if !matches!(declaration, Some("TYPE") | Some("DOMAIN")) {
        return Err("type definition must declare CREATE TYPE or CREATE DOMAIN".into());
    }
    let mut index = 2;
    let parts = consume_qualified_identifier(&tokens, &mut index)
        .ok_or("type definition must declare a type identity")?;
    let expected_schema = schema.filter(|schema| !schema.is_empty());
    let identity_matches = match expected_schema {
        Some(schema) => {
            parts.len() == 2
                && type_identifier_matches(&parts[0], schema)
                && type_identifier_matches(&parts[1], name)
        }
        None => parts.len() == 1 && type_identifier_matches(&parts[0], name),
    };
    if !identity_matches {
        return Err("type definition identity does not match the catalog object".into());
    }
    if index >= tokens.len() {
        return Err("type definition is missing its type body".into());
    }
    Ok(())
}

fn has_non_trailing_statement_terminator(definition: &str) -> bool {
    let chars = definition.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_bracket_quote = false;
    while index < chars.len() {
        let ch = chars[index];
        if in_single_quote {
            if ch == '\'' {
                if chars.get(index + 1) == Some(&'\'') {
                    index += 2;
                    continue;
                }
                in_single_quote = false;
            }
        } else if in_double_quote {
            if ch == '"' {
                if chars.get(index + 1) == Some(&'"') {
                    index += 2;
                    continue;
                }
                in_double_quote = false;
            }
        } else if in_bracket_quote {
            if ch == ']' {
                if chars.get(index + 1) == Some(&']') {
                    index += 2;
                    continue;
                }
                in_bracket_quote = false;
            }
        } else {
            match ch {
                '\'' => in_single_quote = true,
                '"' => in_double_quote = true,
                '[' => in_bracket_quote = true,
                ';' if chars[index + 1..]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .is_empty() =>
                {
                    return false;
                }
                ';' => return true,
                _ => {}
            }
        }
        index += 1;
    }
    in_single_quote || in_double_quote || in_bracket_quote
}

fn type_identifier_matches(token: &ExecutableSqlToken, expected: &str) -> bool {
    match token {
        ExecutableSqlToken::Word(value) => value.eq_ignore_ascii_case(expected),
        ExecutableSqlToken::Identifier(value) => value == expected,
        ExecutableSqlToken::Symbol(_) => false,
    }
}

fn split_sequence_statements(definition: &str) -> Result<Vec<&str>, String> {
    let mut statements = Vec::new();
    let mut start = 0;
    let chars = definition.char_indices().collect::<Vec<_>>();
    let mut index = 0;
    let mut quoted_identifier = false;
    while index < chars.len() {
        let (offset, ch) = chars[index];
        if ch == '"' {
            if quoted_identifier && chars.get(index + 1).is_some_and(|(_, next)| *next == '"') {
                index += 2;
                continue;
            }
            quoted_identifier = !quoted_identifier;
        } else if ch == ';' && !quoted_identifier {
            let statement = definition[start..offset].trim();
            if statement.is_empty() {
                return Err("sequence definition contains an empty statement".into());
            }
            statements.push(statement);
            start = offset + ch.len_utf8();
        }
        index += 1;
    }
    if quoted_identifier {
        return Err("sequence definition contains an unclosed quoted identifier".into());
    }
    let tail = definition[start..].trim();
    if !tail.is_empty() {
        statements.push(tail);
    }
    Ok(statements)
}

fn validate_sequence_definition_safety(definition: &str) -> Result<(), String> {
    let chars = definition.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut quoted_identifier = false;
    while index < chars.len() {
        let ch = chars[index];
        if quoted_identifier {
            if ch == '"' {
                if chars.get(index + 1) == Some(&'"') {
                    index += 2;
                    continue;
                }
                quoted_identifier = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' {
            quoted_identifier = true;
            index += 1;
            continue;
        }
        if ch == '\''
            || ch == '#'
            || (ch == '-' && chars.get(index + 1) == Some(&'-'))
            || (ch == '/' && chars.get(index + 1) == Some(&'*'))
            || (ch == '*' && chars.get(index + 1) == Some(&'/'))
        {
            return Err(
                "sequence definition contains unsupported literal or comment syntax".into(),
            );
        }
        index += 1;
    }
    if quoted_identifier {
        return Err("sequence definition contains an unclosed quoted identifier".into());
    }
    Ok(())
}

fn expect_word(
    tokens: &[ExecutableSqlToken],
    index: &mut usize,
    expected: &str,
) -> Result<(), String> {
    if tokens.get(*index).and_then(ExecutableSqlToken::as_word) != Some(expected) {
        return Err(format!("sequence definition must contain {expected}"));
    }
    *index += 1;
    Ok(())
}

fn consume_qualified_identifier(
    tokens: &[ExecutableSqlToken],
    index: &mut usize,
) -> Option<Vec<ExecutableSqlToken>> {
    let mut parts = Vec::new();
    loop {
        let part = match tokens.get(*index) {
            Some(ExecutableSqlToken::Word(_)) | Some(ExecutableSqlToken::Identifier(_)) => {
                tokens[*index].clone()
            }
            _ => return None,
        };
        parts.push(part);
        *index += 1;
        if tokens.get(*index) != Some(&ExecutableSqlToken::Symbol('.')) {
            break;
        }
        *index += 1;
    }
    Some(parts)
}

fn validate_sequence_identity(
    tokens: &[ExecutableSqlToken],
    index: &mut usize,
    schema: Option<&str>,
    name: &str,
) -> Result<(), String> {
    let parts = consume_qualified_identifier(tokens, index)
        .ok_or("sequence definition must declare a sequence identity")?;
    let expected_schema = schema.filter(|schema| !schema.is_empty());
    let identity_matches = match expected_schema {
        Some(schema) => {
            parts.len() == 2
                && sequence_identifier_matches(&parts[0], schema)
                && sequence_identifier_matches(&parts[1], name)
        }
        None => parts.len() == 1 && sequence_identifier_matches(&parts[0], name),
    };
    if identity_matches {
        Ok(())
    } else {
        Err("sequence definition identity does not match the catalog object".into())
    }
}

fn sequence_identifier_matches(token: &ExecutableSqlToken, expected: &str) -> bool {
    match token {
        // PostgreSQL folds unquoted identifiers to lower case. A catalog
        // identity containing upper-case characters therefore requires a
        // quoted token and must not match an unquoted spelling.
        ExecutableSqlToken::Word(value) => value.to_ascii_lowercase() == expected,
        // Quoted identifiers are exact and the lexer has already restored
        // doubled double-quotes.
        ExecutableSqlToken::Identifier(value) => value == expected,
        ExecutableSqlToken::Symbol(_) => false,
    }
}

fn validate_sequence_options(tokens: &[ExecutableSqlToken]) -> Result<(), String> {
    const KEYWORDS: &[&str] = &[
        "AS",
        "SMALLINT",
        "INTEGER",
        "BIGINT",
        "INCREMENT",
        "BY",
        "MINVALUE",
        "MAXVALUE",
        "START",
        "WITH",
        "CACHE",
        "CYCLE",
        "NO",
    ];
    if tokens.is_empty() {
        return Err("sequence definition is missing sequence attributes".into());
    }
    for token in tokens {
        match token {
            ExecutableSqlToken::Word(value)
                if KEYWORDS.contains(&value.as_str())
                    || value.chars().all(|ch| ch.is_ascii_digit()) => {}
            // Negative values are tokenized as their digits after the minus
            // symbol, which is valid PostgreSQL sequence syntax.
            ExecutableSqlToken::Symbol('-') => {}
            _ => return Err("sequence definition contains unsupported attribute syntax".into()),
        }
    }
    Ok(())
}

fn normalize_identifier(value: &str) -> String {
    value
        .trim()
        .trim_matches(['`', '"', '[', ']'])
        .to_ascii_uppercase()
}

/// Return the object kind, terminal identifier, and optional routine
/// declaration arguments from the executable CREATE declaration header.
///
/// The catalog DDL for MySQL may contain a `DEFINER=...` clause between
/// `CREATE` and the object kind, while PostgreSQL commonly uses
/// `CREATE OR REPLACE`. Tokens inside quoted strings and SQL comments are
/// discarded before inspecting the header, so a body or comment cannot make a
/// wrong object type appear valid.
fn object_declaration_identity(definition: &str) -> Option<(ObjectKind, String, Option<String>)> {
    let tokens = executable_sql_tokens(definition);
    if tokens.first().and_then(ExecutableSqlToken::as_word) != Some("CREATE") {
        return None;
    }
    let mut index = 1;
    if tokens.get(index).and_then(ExecutableSqlToken::as_word) == Some("OR")
        && tokens.get(index + 1).and_then(ExecutableSqlToken::as_word) == Some("REPLACE")
    {
        index += 2;
    }
    if tokens.get(index).and_then(ExecutableSqlToken::as_word) == Some("DEFINER") {
        index += 1;
        if tokens.get(index) != Some(&ExecutableSqlToken::Symbol('=')) {
            return None;
        }
        index += 1;
        let mut has_definer_value = false;
        while let Some(token) = tokens.get(index) {
            match token {
                ExecutableSqlToken::Word(value)
                    if matches!(value.as_str(), "FUNCTION" | "PROCEDURE" | "TRIGGER") =>
                {
                    if !has_definer_value {
                        return None;
                    }
                    break;
                }
                ExecutableSqlToken::Word(value)
                    if matches!(
                        value.as_str(),
                        "AS" | "BEGIN" | "VIEW" | "TABLE" | "SCHEMA" | "EVENT"
                    ) =>
                {
                    return None;
                }
                _ => {
                    has_definer_value = true;
                    index += 1;
                }
            }
        }
    }
    let kind = loop {
        match tokens.get(index) {
            Some(ExecutableSqlToken::Word(token)) => match token.as_str() {
                "FUNCTION" => break ObjectKind::Function,
                "PROCEDURE" => break ObjectKind::Procedure,
                "TRIGGER" => break ObjectKind::Trigger,
                // Once the declaration has reached a relation/view body, a later
                // routine word belongs to SQL text rather than the object header.
                "AS" | "BEGIN" | "VIEW" | "TABLE" | "SCHEMA" | "EVENT" => return None,
                _ => index += 1,
            },
            Some(_) => index += 1,
            None => return None,
        }
    };
    index += 1;
    let mut declared_name = match tokens.get(index) {
        Some(ExecutableSqlToken::Word(name)) | Some(ExecutableSqlToken::Identifier(name)) => {
            name.clone()
        }
        _ => return None,
    };
    index += 1;
    while tokens.get(index) == Some(&ExecutableSqlToken::Symbol('.')) {
        index += 1;
        match tokens.get(index) {
            Some(ExecutableSqlToken::Word(name)) | Some(ExecutableSqlToken::Identifier(name)) => {
                declared_name = name.clone();
                index += 1;
            }
            _ => return None,
        }
    }
    let declared_signature = if matches!(kind, ObjectKind::Function | ObjectKind::Procedure) {
        if tokens.get(index) != Some(&ExecutableSqlToken::Symbol('(')) {
            return None;
        }
        let mut depth = 0usize;
        let mut parameters = Vec::new();
        let mut current = Vec::new();
        let mut closed = false;
        for token in tokens.iter().skip(index + 1) {
            match token {
                ExecutableSqlToken::Symbol('(') => {
                    depth += 1;
                    current.push(token.clone());
                }
                ExecutableSqlToken::Symbol(')') if depth > 0 => {
                    depth -= 1;
                    current.push(token.clone());
                }
                ExecutableSqlToken::Symbol(')') => {
                    if !current.is_empty() {
                        parameters.push(normalize_sql_tokens(&current));
                    }
                    closed = true;
                    break;
                }
                ExecutableSqlToken::Symbol(',') if depth == 0 => {
                    parameters.push(normalize_sql_tokens(&current));
                    current.clear();
                }
                _ => current.push(token.clone()),
            }
        }
        if !closed {
            return None;
        }
        Some(parameters.join(", "))
    } else {
        None
    };
    Some((kind, declared_name, declared_signature))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExecutableSqlToken {
    Word(String),
    Identifier(String),
    Symbol(char),
}

impl ExecutableSqlToken {
    fn as_word(&self) -> Option<&str> {
        match self {
            Self::Word(value) => Some(value.as_str()),
            _ => None,
        }
    }
}

fn normalize_sql_tokens(tokens: &[ExecutableSqlToken]) -> String {
    tokens
        .iter()
        .map(|token| match token {
            ExecutableSqlToken::Word(value) | ExecutableSqlToken::Identifier(value) => {
                value.to_ascii_uppercase()
            }
            ExecutableSqlToken::Symbol(value) => value.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" ( ", "(")
        .replace(" )", ")")
        .replace(" ,", ",")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_signature_part(value: &str) -> String {
    let upper = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase();
    let upper = upper
        .split_once(" DEFAULT ")
        .map(|(value, _)| value)
        .or_else(|| upper.split_once("=").map(|(value, _)| value))
        .unwrap_or(&upper)
        .trim();
    let mut words = upper.split_whitespace().collect::<Vec<_>>();
    if matches!(
        words.first().copied(),
        Some("IN" | "OUT" | "INOUT" | "VARIADIC")
    ) {
        words.remove(0);
    }
    words.join(" ")
}

fn signature_parts(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(normalize_signature_part(&value[start..index]));
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = normalize_signature_part(&value[start..]);
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

fn routine_signatures_match(declared: &str, requested: &str) -> bool {
    let declared_parts = signature_parts(declared);
    let requested_parts = signature_parts(requested);
    if declared_parts.len() != requested_parts.len() {
        return false;
    }
    declared_parts
        .iter()
        .zip(requested_parts.iter())
        .all(|(declared, requested)| {
            declared == requested
                || declared
                    .split_whitespace()
                    .enumerate()
                    .skip(1)
                    .any(|(index, _)| {
                        declared
                            .split_whitespace()
                            .skip(index)
                            .collect::<Vec<_>>()
                            .join(" ")
                            == *requested
                    })
        })
}

fn executable_sql_tokens(sql: &str) -> Vec<ExecutableSqlToken> {
    let chars = sql.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '-' && chars.get(index + 1) == Some(&'-') {
            index += 2;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if ch == '#' {
            index += 1;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if ch == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index = (index + 2).min(chars.len());
            continue;
        }
        if ch == '\'' {
            let quote = ch;
            index += 1;
            while index < chars.len() {
                if chars[index] == '\\' {
                    index = (index + 2).min(chars.len());
                    continue;
                }
                if chars[index] == quote {
                    if chars.get(index + 1) == Some(&quote) {
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                index += 1;
            }
            continue;
        }
        if matches!(ch, '"' | '`' | '[') {
            let closing = if ch == '[' { ']' } else { ch };
            index += 1;
            let mut value = String::new();
            let mut closed = false;
            while index < chars.len() {
                if chars[index] == closing {
                    if chars.get(index + 1) == Some(&closing) {
                        value.push(closing);
                        index += 2;
                        continue;
                    }
                    index += 1;
                    closed = true;
                    break;
                }
                value.push(chars[index]);
                index += 1;
            }
            if closed {
                tokens.push(ExecutableSqlToken::Identifier(value));
            }
            continue;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$') {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || matches!(chars[index], '_' | '$'))
            {
                index += 1;
            }
            tokens.push(ExecutableSqlToken::Word(
                chars[start..index]
                    .iter()
                    .collect::<String>()
                    .to_ascii_uppercase(),
            ));
            continue;
        }
        if matches!(ch, '.' | '(' | ')' | ',' | '=' | '@') {
            tokens.push(ExecutableSqlToken::Symbol(ch));
        }
        index += 1;
    }
    tokens
}

/// Validate a CHECK predicate before it is embedded into a reviewed DDL
/// statement.  The predicate is intentionally kept as SQL because only the
/// target driver can render its dialect, but it must remain one expression
/// and cannot terminate the reviewed statement.
pub fn validate_check_expression(expression: &str) -> Result<(), String> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return Err("check constraint expression must not be empty".into());
    }
    if trimmed.contains(';') {
        return Err("check constraint expression must not contain semicolons".into());
    }
    if trimmed
        .chars()
        .any(|ch| ch == '\0' || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')))
    {
        return Err("check constraint expression contains control characters".into());
    }
    Ok(())
}

pub fn migration_object_kind() -> ObjectKind {
    ObjectKind::View
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationRisk {
    Additive,
    Rewrite,
    Destructive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationRequirement {
    Backfill {
        table: String,
        column: String,
        reason: String,
    },
    Unsupported {
        operation: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStatement {
    pub sql: String,
    pub risk: MigrationRisk,
    pub rollback_sql: Option<String>,
    pub summary: String,
}

pub trait MigrationRenderer: Send + Sync {
    fn render(&self, operation: &MigrationOperation) -> Result<MigrationStatement, String>;

    /// Render a new table with source table-level options when the target
    /// dialect can preserve them. The default renderer ignores those options;
    /// drivers with portable table options can override this method.
    fn render_create_table_with_options(
        &self,
        operation: &MigrationOperation,
        _table_options: &TableOptions,
    ) -> Result<MigrationStatement, String> {
        self.render(operation)
    }

    /// Render a reviewed, driver-native rebuild of one existing table.
    /// Drivers should return a multi-statement sequence only when the host
    /// can execute it inside one transaction. Catalog semantics that are not
    /// represented by `TableSchema` must be reported in the snapshot's
    /// `table_options.migration_blockers` and rejected here.
    fn render_table_rebuild(
        &self,
        _table: &str,
        _desired: &crate::TableSchema,
        _current: &crate::TableSchema,
    ) -> Result<Vec<MigrationStatement>, String> {
        Err("This driver does not support reviewed table rebuilds".into())
    }

    /// Map a schema object's definition and exact dependencies from one scope
    /// into another when the driver can prove the dialect-specific rewrite.
    /// Returning `None` means this renderer does not support scope mapping.
    /// The host validates the returned identities against the supplied pairs.
    fn map_schema_object_scope(
        &self,
        _kind: ObjectKind,
        _source_scope: &str,
        _target_scope: &str,
        _definition: &str,
        _dependencies: &[SchemaObjectScopeDependency],
    ) -> Result<Option<SchemaObjectScopeMapping>, String> {
        Ok(None)
    }
}

pub trait MigrationCapabilities: Send + Sync {
    fn supports(&self, operation: &MigrationOperation) -> bool;
    fn requires_table_rebuild(&self, _operation: &MigrationOperation) -> bool {
        false
    }
    fn transactional_ddl(&self) -> bool {
        true
    }
}

pub fn migration_column(column: &ColumnSchema) -> MigrationColumn {
    MigrationColumn {
        name: column.name.clone(),
        data_type: column.data_type.clone(),
        nullable: column.nullable,
        default_value: column.default_value.clone(),
        comment: column.comment.clone(),
        is_auto_increment: column.is_auto_increment,
    }
}

/// Normalize a column type string for comparison purposes.
pub trait TypeNormalizer: Send + Sync {
    fn normalize_type(&self, data_type: &str) -> String;
}

/// Parse a type string into (base, args, suffix) components.
/// Example: `"VARCHAR(255) UNSIGNED"` → `("VARCHAR", Some("255"), "UNSIGNED")`
pub fn parse_type_parts(raw: &str) -> (String, Option<String>, String) {
    let trimmed = collapse_ws(raw);
    let (core, suffix) = peel_suffixes(&trimmed);
    match (core.find('('), core.rfind(')')) {
        (Some(open), Some(close)) if close > open => {
            let base = core[..open].trim().to_string();
            let args = Some(core[open + 1..close].trim().to_string());
            let remainder = core[close + 1..].trim();
            let combined_suffix = match (remainder.is_empty(), suffix.is_empty()) {
                (true, true) => String::new(),
                (false, true) => remainder.to_string(),
                (true, false) => suffix,
                (false, false) => format!("{remainder} {suffix}"),
            };
            (base, args, combined_suffix)
        }
        _ => (core, None, suffix),
    }
}

pub fn format_type(base: &str, args: Option<&str>, suffix: &str) -> String {
    let mut out = base.to_string();
    if let Some(a) = args {
        if !a.is_empty() {
            out.push('(');
            out.push_str(a);
            out.push(')');
        }
    }
    if !suffix.is_empty() {
        out.push(' ');
        out.push_str(suffix);
    }
    out
}

fn peel_suffixes(raw: &str) -> (String, String) {
    let mut parts: Vec<&str> = raw.split_whitespace().collect();
    let mut suffix = Vec::new();
    while let Some(last) = parts.last().copied() {
        if matches!(last, "UNSIGNED" | "ZEROFILL" | "BINARY") {
            suffix.push(parts.pop().expect("last token"));
        } else {
            break;
        }
    }
    suffix.reverse();
    (parts.join(" "), suffix.join(" "))
}

fn collapse_ws(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

#[cfg(test)]
mod type_parts_tests {
    use super::*;

    #[test]
    fn parse_varchar_with_unsigned_suffix() {
        let (base, args, suffix) = parse_type_parts("VARCHAR(255) UNSIGNED");
        assert_eq!(base, "VARCHAR");
        assert_eq!(args.as_deref(), Some("255"));
        assert_eq!(suffix, "UNSIGNED");
        assert_eq!(
            format_type(&base, args.as_deref(), &suffix),
            "VARCHAR(255) UNSIGNED"
        );
    }

    #[test]
    fn parse_multi_word_base() {
        let (base, args, suffix) = parse_type_parts("double precision");
        assert_eq!(base, "DOUBLE PRECISION");
        assert!(args.is_none());
        assert!(suffix.is_empty());
    }

    #[test]
    fn empty_input_yields_empty_base() {
        let (base, args, suffix) = parse_type_parts("  ");
        assert!(base.is_empty());
        assert!(args.is_none());
        assert!(suffix.is_empty());
    }

    #[test]
    fn parse_array_types_preserves_brackets() {
        let (base, args, suffix) = parse_type_parts("VARCHAR(255)[]");
        assert_eq!(base, "VARCHAR");
        assert_eq!(args.as_deref(), Some("255"));
        assert_eq!(suffix, "[]");
        assert_eq!(
            format_type(&base, args.as_deref(), &suffix),
            "VARCHAR(255) []"
        );
    }

    #[test]
    fn parse_timestamp_with_time_zone_preserves_suffix() {
        let (base, args, suffix) = parse_type_parts("TIMESTAMP(6) WITH TIME ZONE");
        assert_eq!(base, "TIMESTAMP");
        assert_eq!(args.as_deref(), Some("6"));
        assert_eq!(suffix, "WITH TIME ZONE");
        assert_eq!(
            format_type(&base, args.as_deref(), &suffix),
            "TIMESTAMP(6) WITH TIME ZONE"
        );
    }

    #[test]
    fn view_definition_validation_rejects_empty_scripts_and_controls() {
        assert!(validate_view_definition("SELECT 1").is_ok());
        assert!(validate_view_definition("SELECT 1;").is_ok());
        assert!(validate_view_definition("SELECT ';' AS marker;").is_ok());
        assert!(validate_view_definition("  ").is_err());
        assert!(validate_view_definition("SELECT 1; DROP TABLE users").is_err());
        assert!(validate_view_definition("SELECT 1; SELECT 2;").is_err());
        assert!(validate_view_definition("SELECT '\0'").is_err());
    }

    #[test]
    fn migration_identifier_validation_rejects_blank_controls_and_bad_segments() {
        assert_eq!(
            validate_migration_identifier("  audit.events  ").unwrap(),
            "audit.events"
        );
        for value in [
            "",
            " \t ",
            "audit\nevents",
            "audit..events",
            "audit. events",
        ] {
            assert!(validate_migration_identifier(value).is_err(), "{value:?}");
        }
    }

    #[test]
    fn object_definition_validation_allows_routine_bodies_but_rejects_missing_or_wrong_ddl() {
        assert!(validate_object_definition(
            "CREATE FUNCTION calculate_total(integer) RETURNS integer AS $$ BEGIN SELECT 1; END $$",
            ObjectKind::Function,
            "calculate_total"
        )
        .is_ok());
        assert!(validate_object_definition(
            "CREATE TRIGGER audit_insert AFTER INSERT ON orders BEGIN SELECT 1; END",
            ObjectKind::Trigger,
            "audit_insert"
        )
        .is_ok());
        for definition in [
            "",
            "SELECT 1",
            "CREATE VIEW v AS SELECT 1",
            "CREATE FUNCTION x()\0",
        ] {
            assert!(validate_object_definition(definition, ObjectKind::Function, "x").is_err());
        }
    }

    #[test]
    fn test_tester_object_definition_requires_kind_in_create_header() {
        // A routine/trigger token in a literal must not make a CREATE VIEW
        // eligible for routine or trigger migration.
        assert!(validate_object_definition(
            "CREATE VIEW calculate_total AS SELECT 'FUNCTION' AS marker",
            ObjectKind::Function,
            "calculate_total",
        )
        .is_err());
        assert!(validate_object_definition(
            "CREATE VIEW audit_insert AS SELECT 'TRIGGER' AS marker",
            ObjectKind::Trigger,
            "audit_insert",
        )
        .is_err());
        assert!(validate_object_definition(
            "/* FUNCTION */ CREATE VIEW calculate_total AS SELECT 1",
            ObjectKind::Function,
            "calculate_total",
        )
        .is_err());
        assert!(validate_object_definition(
            "CREATE VIEW audit_insert AS SELECT 1 -- TRIGGER",
            ObjectKind::Trigger,
            "audit_insert",
        )
        .is_err());
    }

    #[test]
    fn test_tester_object_definition_ignores_quoted_kind_tokens_outside_header() {
        // Quoted identifiers remain identifiers, rather than declaration-kind
        // tokens. A view body/header containing one must never be accepted as
        // a routine or trigger definition.
        for (definition, kind, name) in [
            (
                "CREATE VIEW report AS SELECT `FUNCTION` FROM metadata",
                ObjectKind::Function,
                "report",
            ),
            (
                "CREATE VIEW `FUNCTION` AS SELECT 1",
                ObjectKind::Function,
                "FUNCTION",
            ),
            (
                "CREATE VIEW report AS SELECT \"TRIGGER\" FROM metadata",
                ObjectKind::Trigger,
                "report",
            ),
            (
                "CREATE VIEW [TRIGGER] AS SELECT 1",
                ObjectKind::Trigger,
                "TRIGGER",
            ),
        ] {
            assert!(
                validate_object_definition(definition, kind, name).is_err(),
                "{definition}"
            );
        }
    }

    #[test]
    fn test_tester_object_definition_requires_requested_name_in_declaration() {
        // The driver contract must not accept an unrelated routine merely
        // because the requested identity occurs in a body literal or comment.
        // Conversely, supported PostgreSQL and MySQL declaration envelopes
        // remain valid when their declared name is the requested identity.
        for definition in [
            "CREATE OR REPLACE FUNCTION other_name() RETURNS integer AS $$ SELECT 'wanted_name' $$ LANGUAGE sql",
            "CREATE FUNCTION other_name() RETURNS integer AS $$ SELECT 1 $$ LANGUAGE sql -- wanted_name",
        ] {
            assert!(
                validate_object_definition(definition, ObjectKind::Function, "wanted_name").is_err(),
                "{definition}"
            );
        }

        assert!(validate_object_definition(
            "CREATE OR REPLACE FUNCTION wanted_name() RETURNS integer AS $$ SELECT 1 $$ LANGUAGE sql",
            ObjectKind::Function,
            "wanted_name",
        )
        .is_ok());
        assert!(validate_object_definition(
            "CREATE DEFINER=`root`@`%` PROCEDURE wanted_name() SELECT 1",
            ObjectKind::Procedure,
            "wanted_name",
        )
        .is_ok());
        assert!(validate_object_definition(
            "CREATE DEFINER=`root`@`%` TRIGGER wanted_name BEFORE INSERT ON orders FOR EACH ROW SET NEW.id = NEW.id",
            ObjectKind::Trigger,
            "wanted_name",
        )
        .is_ok());
    }

    #[test]
    fn sequence_definition_validation_requires_exact_qualified_identity_and_attributes() {
        let ddl = "CREATE SEQUENCE \"public\".\"orders_id_seq\" AS bigint INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1 NO CYCLE; ALTER SEQUENCE \"public\".\"orders_id_seq\" OWNED BY \"public\".\"orders\".\"id\";";
        assert!(
            validate_sequence_definition_with_identity(ddl, Some("public"), "orders_id_seq")
                .is_ok()
        );
        assert!(
            validate_sequence_definition_with_identity(ddl, Some("other"), "orders_id_seq")
                .is_err()
        );
        assert!(validate_sequence_definition_with_identity(ddl, Some("public"), "other").is_err());
        assert!(validate_sequence_definition_with_identity(
            "CREATE SEQUENCE \"public\".\"orders_id_seq\"",
            Some("public"),
            "orders_id_seq"
        )
        .is_err());
    }

    #[test]
    fn sequence_definition_split_returns_exact_validated_owner_identity() {
        let ddl = "CREATE SEQUENCE public.orders_id_seq AS bigint; ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders.id;";
        let (create, ownership_statement, ownership, ownership_reset_statement) =
            split_sequence_definition(ddl, Some("public"), "orders_id_seq").unwrap();
        assert_eq!(create, "CREATE SEQUENCE public.orders_id_seq AS bigint");
        assert_eq!(
            ownership_statement.as_deref(),
            Some("ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders.id")
        );
        assert_eq!(
            ownership,
            Some(("public".into(), "orders".into(), "id".into()))
        );
        assert_eq!(
            ownership_reset_statement.as_deref(),
            Some("ALTER SEQUENCE public.orders_id_seq OWNED BY NONE")
        );

        let (quoted_create, quoted_owner, quoted_identity, quoted_reset) =
            split_sequence_definition(
                "CREATE SEQUENCE \"public\".\"OWNED BY\" AS bigint; ALTER SEQUENCE \"public\".\"OWNED BY\" OWNED BY \"public\".\"CaseTable\".\"Id\";",
                Some("public"),
                "OWNED BY",
            )
            .unwrap();
        assert_eq!(
            quoted_create,
            "CREATE SEQUENCE \"public\".\"OWNED BY\" AS bigint"
        );
        assert_eq!(
            quoted_owner.as_deref(),
            Some("ALTER SEQUENCE \"public\".\"OWNED BY\" OWNED BY \"public\".\"CaseTable\".\"Id\"")
        );
        assert_eq!(
            quoted_identity,
            Some(("public".into(), "CaseTable".into(), "Id".into()))
        );
        assert_eq!(
            quoted_reset.as_deref(),
            Some("ALTER SEQUENCE \"public\".\"OWNED BY\" OWNED BY NONE")
        );

        let (plain_create, plain_ownership_statement, plain_ownership, plain_reset) =
            split_sequence_definition(
                "CREATE SEQUENCE \"public\".\"plain_seq\" AS integer;",
                Some("public"),
                "plain_seq",
            )
            .unwrap();
        assert_eq!(
            plain_create,
            "CREATE SEQUENCE \"public\".\"plain_seq\" AS integer"
        );
        assert_eq!(plain_ownership_statement, None);
        assert_eq!(plain_ownership, None);
        assert_eq!(plain_reset, None);
    }

    #[test]
    fn sequence_definition_validation_rejects_scripts_literals_comments_and_ambiguous_owner() {
        let valid = "CREATE SEQUENCE \"public\".\"s\" AS integer INCREMENT BY -1 MINVALUE -2147483648 MAXVALUE 2147483647 START WITH 1 CACHE 2 CYCLE;";
        for unsafe_ddl in [
            "SELECT 1",
            "CREATE SEQUENCE \"public\".\"s\" AS integer; DROP TABLE users",
            "CREATE SEQUENCE \"public\".\"s\" AS integer -- comment",
            "CREATE SEQUENCE \"public\".\"s\" AS integer START WITH '1'",
            "CREATE SEQUENCE \"public\".\"s\" AS integer; ALTER SEQUENCE \"public\".\"other\" OWNED BY \"public\".\"t\".\"id\"",
            "CREATE SEQUENCE \"public\".\"s\" AS integer; ALTER SEQUENCE \"public\".\"s\" OWNED BY \"public\".\"t\"",
            "CREATE SEQUENCE \"public\".\"s\" AS integer; ALTER SEQUENCE \"public\".\"s\" OWNED BY \"public\".\"t\".\"id\" -- OWNED BY \"public\".\"evil\".\"column\"",
        ] {
            assert!(validate_sequence_definition_with_identity(unsafe_ddl, Some("public"), "s").is_err(), "{unsafe_ddl}");
        }
        assert!(validate_sequence_definition_with_identity(valid, Some("public"), "s").is_ok());
    }

    #[test]
    fn type_definition_validation_requires_exact_identity_and_one_statement() {
        assert!(validate_type_definition_with_identity(
            "CREATE TYPE \"public\".\"mood\" AS ENUM ('sad', 'ok', 'happy');",
            Some("public"),
            "mood"
        )
        .is_ok());
        assert!(validate_type_definition_with_identity(
            "CREATE DOMAIN public.email AS text CHECK (POSITION('@' IN VALUE) > 1)",
            Some("public"),
            "email"
        )
        .is_ok());
        for (definition, schema, name) in [
            ("SELECT 1", Some("public"), "mood"),
            (
                "CREATE TYPE public.other AS ENUM ('ok')",
                Some("public"),
                "mood",
            ),
            (
                "CREATE TYPE public.mood AS ENUM ('ok'); DROP TABLE users",
                Some("public"),
                "mood",
            ),
            ("CREATE TYPE public.mood", Some("public"), "mood"),
        ] {
            assert!(
                validate_type_definition_with_identity(definition, schema, name).is_err(),
                "{definition}"
            );
        }
    }

    #[test]
    fn type_definition_validation_allows_semicolons_inside_enum_literals() {
        assert!(validate_type_definition_with_identity(
            "CREATE TYPE \"public\".\"punctuation\" AS ENUM ('a;b', 'c');",
            Some("public"),
            "punctuation"
        )
        .is_ok());
    }

    #[test]
    fn test_tester_mysql_definer_nested_arguments_and_postgres_identity_validation() {
        let mysql_definition = "CREATE DEFINER=`migrator`@`%` FUNCTION `app`.`sum_amount`(IN p_amount DECIMAL(10,2), OUT p_label VARCHAR(20)) RETURNS DECIMAL(10,2) RETURN p_amount";
        assert!(validate_object_definition_with_identity(
            mysql_definition,
            ObjectKind::Function,
            "sum_amount",
            None,
        )
        .is_ok());
        assert_eq!(
            object_declaration_identity(mysql_definition),
            Some((
                ObjectKind::Function,
                "sum_amount".into(),
                Some("IN P_AMOUNT DECIMAL(10, 2), OUT P_LABEL VARCHAR(20)".into()),
            ))
        );
        let postgres_definition = "CREATE FUNCTION app.sum_amount(p_amount numeric, p_label character varying) RETURNS numeric LANGUAGE SQL AS $$ SELECT p_amount $$";
        assert!(validate_object_definition_with_identity(
            postgres_definition,
            ObjectKind::Function,
            "sum_amount",
            Some("numeric, character varying"),
        )
        .is_ok());
        assert!(validate_object_definition_with_identity(
            postgres_definition,
            ObjectKind::Function,
            "sum_amount",
            Some("integer, character varying"),
        )
        .is_err());
        assert!(object_declaration_identity("CREATE DEFINER FUNCTION f() RETURNS int").is_none());
        assert!(object_declaration_identity("CREATE DEFINER = FUNCTION f() RETURNS int").is_none());
    }

    #[test]
    fn test_tester_sequence_split_fails_closed_on_ambiguous_statement_framing() {
        for invalid in [
            "CREATE SEQUENCE public.orders_id_seq AS bigint;;",
            "CREATE SEQUENCE \"public.orders_id_seq AS bigint",
            "CREATE SEQUENCE public.orders_id_seq AS bigint; ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders",
            "CREATE SEQUENCE public.orders_id_seq AS bigint; ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders.id.extra",
        ] {
            assert!(
                split_sequence_definition(invalid, Some("public"), "orders_id_seq").is_err(),
                "{invalid}"
            );
        }

        assert!(
            ownership_reset_statement("ALTER SEQUENCE public.orders_id_seq RESTART WITH 1")
                .unwrap_err()
                .contains("missing OWNED BY")
        );
        assert!(parse_sequence_ownership(
            "ALTER TABLE public.orders OWNED BY public.orders.id",
            Some("public"),
            "orders_id_seq",
        )
        .is_err());
    }
}
