#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanState {
    Normal,
    Single,
    Double,
    Backtick,
    Bracket,
    LineComment,
    BlockComment,
}

pub fn mask_semicolons_in_literals(sql: &str) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut state = ScanState::Normal;
    let mut index = 0usize;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        let masked = if current == ';' && state != ScanState::Normal { ' ' } else { current };
        match state {
            ScanState::Normal => {
                if current == '\'' {
                    state = ScanState::Single;
                } else if current == '"' {
                    state = ScanState::Double;
                } else if current == '`' {
                    state = ScanState::Backtick;
                } else if current == '[' {
                    state = ScanState::Bracket;
                } else if current == '-' && next == Some('-') {
                    state = ScanState::LineComment;
                } else if current == '/' && next == Some('*') {
                    state = ScanState::BlockComment;
                }
                out.push(masked);
            }
            ScanState::Single => {
                if current == '\\' && next.is_some() {
                    out.push(current);
                    index += 1;
                    out.push(chars[index]);
                } else if current == '\'' && next == Some('\'') {
                    out.push(current);
                    index += 1;
                    out.push(chars[index]);
                } else {
                    out.push(masked);
                    if current == '\'' {
                        state = ScanState::Normal;
                    }
                }
            }
            ScanState::Double | ScanState::Backtick => {
                let quote = if state == ScanState::Double { '"' } else { '`' };
                if current == quote && next == Some(quote) {
                    out.push(current);
                    index += 1;
                    out.push(chars[index]);
                } else {
                    out.push(masked);
                    if current == quote {
                        state = ScanState::Normal;
                    }
                }
            }
            ScanState::Bracket => {
                out.push(masked);
                if current == ']' {
                    state = ScanState::Normal;
                }
            }
            ScanState::LineComment => {
                out.push(masked);
                if current == '\n' || current == '\r' {
                    state = ScanState::Normal;
                }
            }
            ScanState::BlockComment => {
                if current == '*' && next == Some('/') {
                    out.push(current);
                    index += 1;
                    out.push(chars[index]);
                    state = ScanState::Normal;
                } else {
                    out.push(masked);
                }
            }
        }
        index += 1;
    }
    out
}

pub fn split_sql_statements(sql: &str) -> Vec<String> {
    mask_semicolons_in_literals(sql)
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
        .map(str::to_owned)
        .collect()
}

fn normalized_head(statement: &str) -> String {
    statement.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase()
}

fn head_words(head: &str) -> Vec<&str> {
    head.split(' ').filter(|word| !word.is_empty()).collect()
}

pub fn is_schema_changing_statement(statement: &str) -> bool {
    let head = normalized_head(statement);
    let words = head_words(&head);
    let mut position = 0usize;
    match words.get(position) {
        Some(&"CREATE") | Some(&"DROP") => position += 1,
        _ => return false,
    }
    if words.get(position) == Some(&"OR") {
        if words.get(position + 1) != Some(&"REPLACE") {
            return false;
        }
        position += 2;
    }
    if words.get(position) == Some(&"TEMPORARY") {
        position += 1;
    }
    matches!(words.get(position), Some(&"DATABASE") | Some(&"SCHEMA") | Some(&"TABLE"))
}

pub fn sql_contains_schema_changing_ddl(sql: &str) -> bool {
    split_sql_statements(sql).iter().any(|statement| is_schema_changing_statement(statement))
}

pub fn is_schema_mutating_statement(statement: &str) -> bool {
    let head = normalized_head(statement);
    let words = head_words(&head);
    match words.first().copied() {
        Some("CREATE") | Some("ALTER") | Some("DROP") | Some("TRUNCATE") | Some("RENAME") => true,
        Some("COMMENT") => words.get(1) == Some(&"ON"),
        _ => false,
    }
}

pub fn sql_may_mutate_schema(sql: &str) -> bool {
    split_sql_statements(sql).iter().any(|statement| is_schema_mutating_statement(statement))
}

pub fn is_data_modifying_statement(statement: &str) -> bool {
    let head = normalized_head(statement);
    let words = head_words(&head);
    matches!(
        words.first().copied(),
        Some("INSERT") | Some("UPDATE") | Some("DELETE") | Some("REPLACE") | Some("MERGE") | Some("CALL") | Some("TRUNCATE")
    )
}

pub fn sql_contains_data_modifying(sql: &str) -> bool {
    split_sql_statements(sql).iter().any(|statement| is_data_modifying_statement(statement))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semicolons_in_literals_and_comments_are_masked() {
        assert_eq!(split_sql_statements("SELECT 1; SELECT 2"), vec!["SELECT 1", "SELECT 2"]);
        assert_eq!(split_sql_statements("SELECT ';'; SELECT 2;"), vec!["SELECT ' '", "SELECT 2"]);
        assert_eq!(split_sql_statements("SELECT \"a;b\"; SELECT 2"), vec!["SELECT \"a b\"", "SELECT 2"]);
        assert_eq!(split_sql_statements("SELECT `a;b`; SELECT 2"), vec!["SELECT `a b`", "SELECT 2"]);
        assert_eq!(split_sql_statements("SELECT [a;b]; SELECT 2"), vec!["SELECT [a b]", "SELECT 2"]);
        assert_eq!(split_sql_statements("-- a;b\nSELECT 1;"), vec!["-- a b\nSELECT 1"]);
        assert_eq!(split_sql_statements("/* a;b */ SELECT 1;"), vec!["/* a b */ SELECT 1"]);
        assert!(split_sql_statements("  ;  ").is_empty());
    }

    #[test]
    fn escaped_and_doubled_quotes_keep_their_semicolons_masked() {
        let masked = mask_semicolons_in_literals("SELECT 'it''s;ok', 'a\\';b'");
        assert!(!masked.contains("';"), "semicolons inside strings are masked");
        assert_eq!(split_sql_statements("SELECT 'it''s;ok'; SELECT 2"), vec!["SELECT 'it''s ok'", "SELECT 2"]);
    }

    #[test]
    fn schema_changing_ddl_requires_create_or_drop_of_database_schema_table() {
        for sql in [
            "CREATE TABLE t (id int)",
            "create or replace temporary schema s",
            "DROP DATABASE d",
            "  CREATE   TABLE   t  ",
        ] {
            assert!(is_schema_changing_statement(sql), "{sql}");
        }
        for sql in [
            "",
            "ALTER TABLE t ADD c int",
            "CREATE VIEW v AS SELECT 1",
            "CREATE TABLESPACE ts",
            "SELECT * FROM t",
        ] {
            assert!(!is_schema_changing_statement(sql), "{sql}");
        }
    }

    #[test]
    fn schema_mutating_ddl_covers_the_verb_family() {
        for sql in [
            "CREATE TABLE t (id int)",
            "ALTER TABLE t ADD c int",
            "DROP VIEW v",
            "TRUNCATE TABLE t",
            "RENAME TABLE a TO b",
            "COMMENT ON TABLE t IS 'x'",
        ] {
            assert!(is_schema_mutating_statement(sql), "{sql}");
        }
        assert!(!is_schema_mutating_statement("COMMENTARY x"));
        assert!(!is_schema_mutating_statement("INSERT INTO t VALUES (1)"));
    }

    #[test]
    fn data_modifying_covers_dml_and_truncate() {
        for sql in [
            "INSERT INTO t VALUES (1)",
            "UPDATE t SET a=1",
            "DELETE FROM t",
            "REPLACE INTO t VALUES (1)",
            "MERGE INTO t USING s ON 1=1",
            "CALL proc()",
            "TRUNCATE t",
        ] {
            assert!(is_data_modifying_statement(sql), "{sql}");
        }
        assert!(!is_data_modifying_statement("SELECT 1"));
        assert!(!is_data_modifying_statement("CALLING x"));
    }

    #[test]
    fn script_level_checks_split_statements() {
        let script = "SELECT 1; CREATE TABLE t (id int); DROP VIEW v; INSERT INTO t VALUES (1);";
        assert!(sql_contains_schema_changing_ddl(script));
        assert!(sql_may_mutate_schema(script));
        assert!(sql_contains_data_modifying(script));
        assert!(!sql_contains_schema_changing_ddl("SELECT 'CREATE TABLE x'; SELECT 2"));
        assert!(!sql_contains_data_modifying("SELECT 'INSERT INTO x'; SELECT 2"));
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::mask_semicolons_in_literals;
        let _ = crate::split_sql_statements;
        let _ = crate::is_schema_changing_statement;
        let _ = crate::sql_contains_schema_changing_ddl;
        let _ = crate::is_schema_mutating_statement;
        let _ = crate::sql_may_mutate_schema;
        let _ = crate::is_data_modifying_statement;
        let _ = crate::sql_contains_data_modifying;
    }
}
