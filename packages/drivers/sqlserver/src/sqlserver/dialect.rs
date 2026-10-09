//! SQL Server *text* rules: everything that shapes or reads T-SQL text with no
//! live session in sight — identifier quoting, catalog SQL templates, statement
//! splitting, batch routing and TOP/OFFSET rewriting.
//!
//! Split out from `super` because none of it touches a connection: every item
//! here is a pure function of T-SQL grammar, which is why the dialect can be
//! unit-tested without a server and why a dialect fix lands in exactly one file.

use super::*;

impl SqlServerDriver {
    /// `[database].` prefix for a catalog view, or an empty string when the
    /// caller targets the connection's current database. SQL Server accepts
    /// three-part names, so a metadata read of another database never needs a
    /// session-level `USE`.
    pub(crate) fn catalog_prefix(database: &str) -> String {
        let trimmed = database.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            // Bracket quoting; escape `]` by doubling.
            format!("[{}].", trimmed.replace(']', "]]"))
        }
    }

    pub(crate) fn quote_identifier(identifier: &str) -> Result<String, DriverError> {
        if identifier.is_empty() || identifier.contains('\0') {
            return Err(DriverError::InvalidConfig(
                "SQL Server relation identifiers must be non-empty and contain no NUL".into(),
            ));
        }
        Ok(format!("[{}]", identifier.replace(']', "]]")))
    }

    pub(crate) fn identity_insert_relation(
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<String, DriverError> {
        let mut parts = Vec::with_capacity(3);
        if !database.trim().is_empty() {
            parts.push(Self::quote_identifier(database)?);
        }
        let schema = schema
            .map(str::trim)
            .filter(|schema| !schema.is_empty())
            .unwrap_or("dbo");
        parts.push(Self::quote_identifier(schema)?);
        parts.push(Self::quote_identifier(table)?);
        Ok(parts.join("."))
    }

    pub(crate) fn identity_insert_statement(
        database: &str,
        schema: Option<&str>,
        table: &str,
        enabled: bool,
    ) -> Result<String, DriverError> {
        let relation = Self::identity_insert_relation(database, schema, table)?;
        let mode = if enabled { "ON" } else { "OFF" };
        Ok(format!("SET IDENTITY_INSERT {relation} {mode}"))
    }

    /// Accept only a relation rendered by Data Transfer's SQL Server
    /// identifier quoter. This prevents a caller from smuggling SQL into the
    /// session toggle or the OBJECT_ID condition.
    pub(crate) fn quoted_transfer_relation_parts(relation: &str) -> Option<Vec<&str>> {
        let bytes = relation.as_bytes();
        let mut cursor = 0;
        let mut parts = Vec::new();
        while cursor < bytes.len() {
            if bytes[cursor] != b'[' {
                return None;
            }
            let part_start = cursor;
            cursor += 1;
            let mut has_identifier_content = false;
            let mut closed = false;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b']' if bytes.get(cursor + 1) == Some(&b']') => {
                        has_identifier_content = true;
                        cursor += 2;
                    }
                    b']' => {
                        cursor += 1;
                        closed = true;
                        break;
                    }
                    _ => {
                        has_identifier_content = true;
                        cursor += 1;
                    }
                }
            }
            if !closed || !has_identifier_content {
                return None;
            }
            parts.push(&relation[part_start..cursor]);
            if cursor == bytes.len() {
                break;
            }
            if bytes[cursor] != b'.' {
                return None;
            }
            cursor += 1;
            if cursor == bytes.len() {
                return None;
            }
        }
        (1..=3).contains(&parts.len()).then_some(parts)
    }

    pub(crate) fn render_sql_file_identity_insert(
        insert_sql: &str,
        target_relation: &str,
        mapped_target_columns: &[String],
    ) -> Result<String, DriverError> {
        if mapped_target_columns.is_empty() {
            return Ok(insert_sql.to_string());
        }
        if mapped_target_columns
            .iter()
            .any(|column| column.is_empty() || column.contains('\0'))
        {
            return Err(DriverError::InvalidConfig(
                "SQL Server SQL-file identity wrapper requires valid mapped target column names"
                    .into(),
            ));
        }
        let relation_parts =
            Self::quoted_transfer_relation_parts(target_relation).ok_or_else(|| {
                DriverError::InvalidConfig(
                    "SQL Server SQL-file identity wrapper requires a safely quoted target relation"
                        .into(),
                )
            })?;
        let object_name = target_relation.replace('\'', "''");
        let identity_catalog = relation_parts
            .get(2)
            .map(|_| format!("{}.sys.identity_columns", relation_parts[0]))
            .unwrap_or_else(|| "sys.identity_columns".into());
        let mapped_columns = mapped_target_columns
            .iter()
            .map(|column| format!("N'{}'", column.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!(
            "IF EXISTS (SELECT 1 FROM {identity_catalog} WHERE object_id = OBJECT_ID(N'{object_name}', N'U') AND name IN ({mapped_columns}))\nBEGIN\n    BEGIN TRY\n        SET IDENTITY_INSERT {target_relation} ON;\n        {insert_sql};\n        SET IDENTITY_INSERT {target_relation} OFF;\n    END TRY\n    BEGIN CATCH\n        BEGIN TRY\n            SET IDENTITY_INSERT {target_relation} OFF;\n        END TRY\n        BEGIN CATCH\n            THROW;\n        END CATCH\n        THROW;\n    END CATCH\nEND\nELSE\nBEGIN\n    {insert_sql};\nEND"
        ))
    }

    /// List tables and views together with the schema that owns them.
    ///
    /// The schema column is mandatory: SQL Server has a real schema level, and
    /// the backup/dump path feeds `TableInfo::schema` straight back into
    /// `get_table_schema`, which the contract validator rejects when it is
    /// missing. A schema filter is applied only when the caller pinned one;
    /// `None` lists every schema in the database, which is the set the
    /// connection tree groups by.
    pub(crate) fn build_tables_sql(database: &str, schema: Option<&str>) -> String {
        let catalog = Self::catalog_prefix(database);
        let filter = match schema.map(str::trim).filter(|s| !s.is_empty()) {
            Some(schema) => format!(" WHERE s.name = '{}'", schema.replace('\'', "''")),
            None => String::new(),
        };
        format!(
            "SELECT s.name AS schema_name, t.name AS table_name, 'TABLE' AS kind \
             FROM {catalog}sys.tables t JOIN {catalog}sys.schemas s ON t.schema_id = s.schema_id{filter} \
             UNION ALL \
             SELECT s.name, v.name, 'VIEW' \
             FROM {catalog}sys.views v JOIN {catalog}sys.schemas s ON v.schema_id = s.schema_id{filter} \
             ORDER BY schema_name, table_name"
        )
    }

    /// Template used by catalog-builder unit tests; object names are bound by
    /// the caller rather than interpolated into this SQL text.
    #[cfg(test)]
    pub(crate) fn build_table_schema_sql(database: &str, schema: &str, table: &str) -> String {
        let _ = (schema, table);
        crate::metadata::columns_sql(database)
    }

    /// Batch columns for every table/view in `database` (optionally narrowed to
    /// one `schema`). `database` is inlined as a catalog prefix, so a batch read
    /// of another database needs no session `USE`.
    pub(crate) fn build_all_columns_sql(database: &str, schema: Option<&str>) -> String {
        let catalog = Self::catalog_prefix(database);
        let filter = match schema.map(str::trim).filter(|s| !s.is_empty()) {
            Some(schema) => format!(" AND s.name = '{}'", schema.replace('\'', "''")),
            None => String::new(),
        };
        format!(
            "SELECT s.name AS schema_name, o.name AS table_name, c.name AS column_name, \
             CASE \
               WHEN tp.is_user_defined = 1 THEN QUOTENAME(tp_schema.name) + N'.' + QUOTENAME(tp.name) \
               WHEN tp.name IN ('nvarchar', 'nchar') THEN CONCAT(tp.name, '(', CASE WHEN c.max_length = -1 THEN 'max' ELSE CONVERT(varchar(10), c.max_length / 2) END, ')') \
               WHEN tp.name IN ('varchar', 'char', 'varbinary', 'binary') THEN CONCAT(tp.name, '(', CASE WHEN c.max_length = -1 THEN 'max' ELSE CONVERT(varchar(10), c.max_length) END, ')') \
               WHEN tp.name IN ('decimal', 'numeric') THEN CONCAT(tp.name, '(', c.precision, ',', c.scale, ')') \
               WHEN tp.name IN ('time', 'datetime2', 'datetimeoffset') THEN CONCAT(tp.name, '(', c.scale, ')') \
               WHEN tp.name = 'float' THEN CONCAT(tp.name, '(', c.precision, ')') \
               ELSE tp.name \
             END AS data_type, c.is_nullable, c.is_identity, dc.definition AS default_value, \
             CAST(ep.value AS nvarchar(max)) AS comment, \
             CAST(CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS bit) AS is_pk, \
             ISNULL(pk.key_ordinal, 0) AS pk_ordinal \
             FROM {catalog}sys.columns c \
             JOIN {catalog}sys.objects o ON c.object_id = o.object_id \
             JOIN {catalog}sys.schemas s ON o.schema_id = s.schema_id \
             JOIN {catalog}sys.types tp ON c.user_type_id = tp.user_type_id \
             LEFT JOIN {catalog}sys.schemas tp_schema ON tp_schema.schema_id = tp.schema_id \
             LEFT JOIN {catalog}sys.default_constraints dc ON c.default_object_id = dc.object_id \
             LEFT JOIN {catalog}sys.extended_properties ep ON ep.major_id = c.object_id AND ep.minor_id = c.column_id AND ep.name = 'MS_Description' \
             LEFT JOIN ( \
               SELECT ic.object_id, ic.column_id, ic.key_ordinal \
               FROM {catalog}sys.index_columns ic \
               INNER JOIN {catalog}sys.indexes i ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
               WHERE i.is_primary_key = 1 \
             ) pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id \
             WHERE o.type IN ('U', 'V'){filter} \
             ORDER BY s.name, o.name, c.column_id"
        )
    }

    /// Effective schema for a single-table read: the explicit argument wins,
    /// otherwise the driver's conventional default (`dbo`). Never a hardcoded
    /// literal at the call site, so the convention stays owned by the driver.
    pub(crate) fn effective_schema<'a>(&self, schema: Option<&'a str>) -> Option<&'a str> {
        schema
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or(self.default_schema())
    }

    pub(crate) fn bit_true(v: &Option<Value>) -> bool {
        matches!(v, Some(Value::Bool(true)) | Some(Value::Integer(1)))
    }
}

/// Split a multi-statement script into individual statements.
///
/// A plain `split(';')` breaks on any semicolon inside a string literal, a
/// bracketed identifier or a comment (`SELECT ';' AS [a]` failed with error 105
/// "Unclosed quotation mark"), so this delegates to the shared, quote/comment
/// aware scanner in `driver-api`.
pub(crate) fn split_statements(sql: &str) -> Vec<String> {
    use datazen_driver_api::sql_split::{is_comment_only_or_empty, split_sql_statements};

    // A routine definition is one SQL Server batch. The shared splitter is
    // dialect-neutral and treats the semicolon after a statement in a
    // BEGIN/END body as a top-level separator, which sends an incomplete
    // CREATE FUNCTION/PROCEDURE/TRIGGER to the server. Keep the module body
    // intact; `needs_own_batch` routes it through `simple_query` below.
    if is_routine_definition(sql) {
        let statement = sql.trim();
        return if statement.is_empty() {
            Vec::new()
        } else {
            vec![statement.to_string()]
        };
    }

    split_sql_statements(sql)
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !is_comment_only_or_empty(s))
        .collect()
}

fn is_routine_definition(sql: &str) -> bool {
    let mut words = leading_keywords(sql).into_iter();
    let Some(first) = words.next() else {
        return false;
    };
    if first != "CREATE" && first != "ALTER" {
        return false;
    }

    let mut kind = words.next();
    if kind.as_deref() == Some("OR") {
        let _ = words.next();
        kind = words.next();
    }
    matches!(
        kind.as_deref(),
        Some("FUNCTION" | "PROCEDURE" | "PROC" | "TRIGGER")
    )
}

/// T-SQL accepts a few statements **only as the first statement of a batch**:
/// the programmable-object definitions (`CREATE`/`ALTER` `SCHEMA`, `VIEW`,
/// `PROCEDURE`/`PROC`, `FUNCTION`, `TRIGGER`, `RULE`, `DEFAULT`). tiberius sends
/// `Client::query`/`Client::execute` through `sp_executesql`, which rejects them
/// with error 156 (`Incorrect syntax near the keyword 'SCHEMA'`) — verified
/// live against Azure SQL Database. Such statements must go out as a real batch
/// via `Client::simple_query`.
///
/// Session-scoped statements (`SET`, `USE`, transaction control) are routed the
/// same way for the same underlying reason: `sp_executesql` runs them in a
/// module whose scope ends with the call, so the setting or transaction would be
/// discarded (and transaction control fails with error 266).
pub(crate) fn needs_own_batch(sql: &str) -> bool {
    let mut words = leading_keywords(sql).into_iter();
    let Some(first) = words.next() else {
        return false;
    };
    match first.as_str() {
        // Session-scoped statements do not survive the module boundary that
        // `sp_executesql` (tiberius `Client::query`) creates: the setting, the
        // `USE` context or the open transaction is rolled back when the module
        // exits. Transaction control additionally fails outright with error 266
        // ("Transaction count after EXECUTE indicates a mismatching number of
        // BEGIN and COMMIT statements"). Send these as a real batch.
        //
        // `SET SHOWPLAN_TEXT ON` is the sharpest case: if it does not stick,
        // `explain()` executes the statement it was asked to plan.
        "BEGIN" | "COMMIT" | "ROLLBACK" | "SAVE" | "SET" => return true,
        "CREATE" | "ALTER" => {}
        _ => return false,
    }
    let mut kind = words.next();
    if kind.as_deref() == Some("OR") {
        // `CREATE OR ALTER <kind>`, the SQL Server 2016 SP1+ form.
        let _ = words.next();
        kind = words.next();
    }
    matches!(
        kind.as_deref(),
        Some(
            "SCHEMA" | "VIEW" | "PROCEDURE" | "PROC" | "FUNCTION" | "TRIGGER" | "RULE" | "DEFAULT"
        )
    )
}

/// The first few keywords of `sql`, uppercased, with leading line and block
/// comments skipped.
fn leading_keywords(sql: &str) -> Vec<String> {
    let mut rest = sql;
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("--") {
            rest = after.split_once('\n').map_or("", |(_, tail)| tail);
            continue;
        }
        if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, tail)| tail);
            continue;
        }
        break;
    }
    rest.split_whitespace()
        .take(4)
        .map(|word| {
            word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .to_ascii_uppercase()
        })
        .collect()
}

/// True when the statement paginates itself with a **top-level** `OFFSET`
/// clause.
///
/// T-SQL rejects `TOP` in the same query as `OFFSET … FETCH` (error 10741:
/// "A TOP can not be used in the same query or sub-query as a OFFSET"), so the
/// editor row cap must not be injected into a statement that already pages:
/// `SELECT … ORDER BY (SELECT NULL) OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY` —
/// exactly what the Visual Query Builder emits for SQL Server — failed on
/// execute until this check existed.
///
/// Only depth-0 occurrences count: `TOP` in an outer query next to an `OFFSET`
/// inside a sub-query is legal, so a nested one must not disable the cap.
/// String literals, quoted/bracketed identifiers and comments are skipped, so
/// `SELECT 'OFFSET 5 ROWS' AS [offset] FROM t` still gets its cap.
pub(crate) fn has_top_level_offset(sql: &str) -> bool {
    let chars: Vec<char> = sql.chars().collect();
    let mut depth = 0i32;
    let mut words: Vec<(i32, String)> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' | '"' => {
                let quote = c;
                i += 1;
                while i < chars.len() {
                    if chars[i] == quote {
                        // A doubled quote is an escaped quote, not the end.
                        if chars.get(i + 1) == Some(&quote) {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '[' => {
                i += 1;
                while i < chars.len() {
                    if chars[i] == ']' {
                        if chars.get(i + 1) == Some(&']') {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '-' if chars.get(i + 1) == Some(&'-') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
            }
            '(' => {
                depth += 1;
                i += 1;
            }
            ')' => {
                depth -= 1;
                i += 1;
            }
            c if c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '#' | '$') => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric()
                        || matches!(chars[i], '_' | '@' | '#' | '$'))
                {
                    i += 1;
                }
                words.push((
                    depth,
                    chars[start..i]
                        .iter()
                        .collect::<String>()
                        .to_ascii_uppercase(),
                ));
            }
            _ => i += 1,
        }
    }

    // `OFFSET <count> [ROW|ROWS]`: the count is a literal or a variable, never a
    // bare identifier — that is what keeps a column named `offset` from
    // disabling the cap.
    words.windows(2).any(|pair| {
        pair[0].0 == 0
            && pair[0].1 == "OFFSET"
            && (pair[1].1.starts_with('@')
                || pair[1].1.chars().next().is_some_and(|c| c.is_ascii_digit()))
    })
}

pub(crate) fn apply_sqlserver_top(stmt: &str, limit: Option<u32>) -> (String, Option<u32>) {
    let Some(lim) = limit else {
        return (stmt.to_string(), None);
    };
    let trimmed = stmt.trim();
    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("SELECT") {
        return (stmt.to_string(), None);
    }
    // A statement that pages itself is left alone: `TOP` cannot join it (10741)
    // and its own `FETCH NEXT` already bounds the result.
    if has_top_level_offset(trimmed) {
        return (stmt.to_string(), None);
    }
    let after_select = trimmed["SELECT".len()..].trim_start();
    let after_upper = after_select.to_ascii_uppercase();
    let (prefix, body) = if after_upper.starts_with("DISTINCT") {
        (
            "SELECT DISTINCT",
            after_select["DISTINCT".len()..].trim_start(),
        )
    } else {
        ("SELECT", after_select)
    };
    // `TOP` is this dialect's own row limit; the switch only caps SELECTs
    // *without* one, so a hand-written `TOP` is respected verbatim.
    if body.to_ascii_uppercase().starts_with("TOP") {
        return (stmt.to_string(), None);
    }
    (format!("{prefix} TOP {} {body}", lim + 1), Some(lim))
}

pub(crate) fn schema_migration_blockers_for_indexes(indexes: &[IndexInfo]) -> Vec<String> {
    let primary_indexes = indexes
        .iter()
        .filter(|index| index.is_primary)
        .collect::<Vec<_>>();
    let mut blockers = Vec::new();
    if primary_indexes.len() > 1 {
        blockers.push(
            "SQL Server returned multiple primary-key indexes; the shared migration IR cannot identify one safely".into(),
        );
    } else if let Some(primary) = primary_indexes.first() {
        let kind = primary
            .index_type
            .strip_prefix("UNIQUE_CONSTRAINT:")
            .unwrap_or(&primary.index_type);
        if !kind.eq_ignore_ascii_case("CLUSTERED") {
            blockers.push(
                "SQL Server primary-key clustering is nonclustered; the shared migration IR cannot preserve this property during table creation".into(),
            );
        }
    }
    if indexes.iter().any(|index| {
        !index.is_primary
            && index
                .index_type
                .strip_prefix("UNIQUE_CONSTRAINT:")
                .unwrap_or(&index.index_type)
                .eq_ignore_ascii_case("CLUSTERED")
    }) {
        blockers.push(
            "SQL Server table has a clustered secondary index; the shared migration IR cannot prove this index layout is compatible with every planned primary-key change".into(),
        );
    }
    blockers
}

pub(crate) const PHYSICAL_DATABASE_IDENTITY_SQL: &str =
    "SELECT CONVERT(nvarchar(256), SERVERPROPERTY('ServerName')) AS server_name, d.database_id AS database_id FROM sys.databases AS d WHERE d.name = DB_NAME() AND NULLIF(CONVERT(nvarchar(256), SERVERPROPERTY('ServerName')), N'') IS NOT NULL AND DB_ID() IS NOT NULL AND DB_ID(NULLIF(@P1, N'')) = DB_ID()";

pub(crate) fn parse_physical_database_identity(result: &QueryResult) -> Option<String> {
    if result.rows.len() != 1 || result.rows[0].len() != 2 {
        return None;
    }
    let row = result.rows.first()?;
    let server_name = match row.first()?.as_ref()? {
        Value::String(value) if !value.trim().is_empty() => value.trim(),
        _ => return None,
    };
    let database_id = match row.get(1)?.as_ref()? {
        Value::Integer(value) if *value > 0 => value,
        _ => return None,
    };
    Some(format!(
        "sqlserver:server:{}:{}:database:{}",
        server_name.len(),
        server_name,
        database_id
    ))
}

pub(crate) fn parse_schema_scope_identity(result: &QueryResult) -> Option<String> {
    if result.rows.len() != 1 || result.rows[0].len() != 1 {
        return None;
    }
    let schema_id = match result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(Option::as_ref)
    {
        Some(Value::Integer(value)) if *value > 0 => *value,
        Some(Value::String(value)) => match value.trim().parse::<i64>() {
            Ok(value) if value > 0 => value,
            _ => return None,
        },
        _ => return None,
    };
    Some(format!("sqlserver:schema-id:{schema_id}"))
}
