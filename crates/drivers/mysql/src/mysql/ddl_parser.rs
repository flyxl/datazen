//! SHOW CREATE TABLE scanners for foreign keys and CHECK constraints.

use super::*;

impl MysqlDriver {
    /// Parse CONSTRAINT ... FOREIGN KEY lines from SHOW CREATE TABLE output.
    pub(crate) fn parse_fk_from_create_table(create_sql: &str) -> Vec<ForeignKeyInfo> {
        let mut fks = Vec::new();
        for line in create_sql.lines() {
            let trimmed = line.trim();
            if !trimmed.contains("FOREIGN KEY") {
                continue;
            }
            // Pattern: CONSTRAINT `name` FOREIGN KEY (`cols`) REFERENCES `table` (`cols`) ...
            let fk_name = Self::extract_backtick_after(trimmed, "CONSTRAINT");
            let fk_cols = Self::extract_backtick_list_after(trimmed, "FOREIGN KEY");
            let ref_table = Self::extract_qualified_table_after(trimmed, "REFERENCES");
            let ref_cols = Self::extract_backtick_list_after(trimmed, "REFERENCES");

            let on_delete = Self::extract_rule(trimmed, "ON DELETE");
            let on_update = Self::extract_rule(trimmed, "ON UPDATE");

            if !fk_name.is_empty() && !fk_cols.is_empty() {
                fks.push(ForeignKeyInfo {
                    name: fk_name,
                    columns: fk_cols,
                    referenced_table: ref_table,
                    referenced_columns: ref_cols,
                    on_delete,
                    on_update,
                    deferrability: ForeignKeyDeferrability::NotDeferrable,
                });
            }
        }
        fks.sort_by(|a, b| a.name.cmp(&b.name));
        fks
    }

    /// Parse CHECK clauses from SHOW CREATE TABLE. The scanner skips quoted
    /// SQL and comments before looking for a real CHECK keyword, then balances
    /// the predicate parentheses so literals containing CHECK cannot become
    /// false constraints or truncate an expression.
    pub(crate) fn parse_check_from_create_table(create_sql: &str) -> Vec<CheckConstraint> {
        let mut checks = Vec::new();
        let bytes = create_sql.as_bytes();
        let mut index = 0usize;
        while index + 5 <= bytes.len() {
            let ch = bytes[index] as char;
            if matches!(ch, '\'' | '"' | '`') {
                index = Self::skip_quoted_sql(bytes, index, ch);
                continue;
            }
            if index + 1 < bytes.len() && bytes[index] == b'/' && bytes[index + 1] == b'*' {
                index = Self::skip_block_comment(bytes, index);
                continue;
            }
            if bytes[index..].starts_with(b"--") || bytes[index] == b'#' {
                index = Self::skip_line_comment(bytes, index);
                continue;
            }
            let is_check = bytes[index..index + 5].eq_ignore_ascii_case(b"CHECK")
                && (index == 0 || !Self::is_sql_identifier_byte(bytes[index - 1]))
                && (index + 5 == bytes.len() || !Self::is_sql_identifier_byte(bytes[index + 5]));
            if !is_check {
                index += 1;
                continue;
            }
            let mut open = index + 5;
            while open < bytes.len() && bytes[open].is_ascii_whitespace() {
                open += 1;
            }
            if open >= bytes.len() || bytes[open] != b'(' {
                index += 5;
                continue;
            }
            let Some(close) = Self::scan_sql_parentheses(bytes, open) else {
                break;
            };
            let expression = create_sql[open + 1..close].trim();
            if !expression.is_empty() {
                let line_start = create_sql[..index].rfind('\n').map_or(0, |pos| pos + 1);
                let name =
                    Self::extract_backtick_after(&create_sql[line_start..index], "CONSTRAINT");
                checks.push(CheckConstraint {
                    name: if name.is_empty() {
                        format!("check_{}", checks.len())
                    } else {
                        name
                    },
                    expression: expression.to_string(),
                });
            }
            index = close + 1;
        }
        checks.sort_by(|a, b| a.name.cmp(&b.name));
        checks
    }

    fn skip_quoted_sql(bytes: &[u8], start: usize, quote: char) -> usize {
        let mut index = start + 1;
        while index < bytes.len() {
            if bytes[index] == b'\\' && index + 1 < bytes.len() {
                index += 2;
            } else if bytes[index] as char == quote {
                if index + 1 < bytes.len() && bytes[index + 1] as char == quote {
                    index += 2;
                } else {
                    return index + 1;
                }
            } else {
                index += 1;
            }
        }
        bytes.len()
    }

    fn skip_block_comment(bytes: &[u8], start: usize) -> usize {
        bytes[start + 2..]
            .windows(2)
            .position(|pair| pair == b"*/")
            .map_or(bytes.len(), |offset| start + 2 + offset + 2)
    }

    fn skip_line_comment(bytes: &[u8], start: usize) -> usize {
        bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| start + offset + 1)
    }

    fn is_sql_identifier_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }

    fn scan_sql_parentheses(bytes: &[u8], open: usize) -> Option<usize> {
        let mut depth = 0usize;
        let mut index = open;
        while index < bytes.len() {
            let ch = bytes[index] as char;
            if matches!(ch, '\'' | '"' | '`') {
                index = Self::skip_quoted_sql(bytes, index, ch);
                continue;
            }
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                _ => {}
            }
            index += 1;
        }
        None
    }

    /// Extract the first backtick-quoted identifier after a keyword.
    fn extract_backtick_after(s: &str, keyword: &str) -> String {
        if let Some(pos) = s.find(keyword) {
            let after = &s[pos + keyword.len()..];
            if let Some(start) = after.find('`') {
                let inner = &after[start + 1..];
                if let Some(end) = inner.find('`') {
                    return inner[..end].to_string();
                }
            }
        }
        String::new()
    }

    pub(crate) fn extract_qualified_table_after(s: &str, keyword: &str) -> String {
        let Some(pos) = s.find(keyword) else {
            return String::new();
        };
        let after = s[pos + keyword.len()..].trim_start();
        let bytes = after.as_bytes();
        let mut index = 0;
        let mut parts = Vec::new();
        loop {
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index >= bytes.len() || bytes[index] == b'(' {
                return String::new();
            }
            let part = if bytes[index] == b'`' {
                index += 1;
                let content_start = index;
                let mut value = String::new();
                let mut closed = false;
                while index < bytes.len() {
                    if bytes[index] == b'`' {
                        if index + 1 < bytes.len() && bytes[index + 1] == b'`' {
                            index += 2;
                        } else {
                            value = after[content_start..index].replace("``", "`");
                            index += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        index += 1;
                    }
                }
                if !closed {
                    return String::new();
                }
                value
            } else {
                let start = index;
                while index < bytes.len()
                    && bytes[index] != b'.'
                    && bytes[index] != b'('
                    && !bytes[index].is_ascii_whitespace()
                {
                    index += 1;
                }
                after[start..index].to_string()
            };
            if part.is_empty() || part.contains('.') {
                return String::new();
            }
            parts.push(part);
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'.' {
                index += 1;
                continue;
            }
            if index == bytes.len() || bytes[index] == b'(' || bytes[index].is_ascii_whitespace() {
                return parts.join(".");
            }
            return String::new();
        }
    }
}
