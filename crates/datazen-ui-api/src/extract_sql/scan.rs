pub const SQL_KEYWORDS: [&str; 21] = [
    "SELECT", "INSERT", "UPDATE", "DELETE", "WITH", "CREATE", "ALTER", "DROP", "EXPLAIN", "SHOW",
    "DESCRIBE", "PRAGMA", "TRUNCATE", "GRANT", "REVOKE", "BEGIN", "COMMIT", "ROLLBACK", "SET",
    "USE", "VACUUM",
];

pub const FENCE_TAGS: [&str; 5] = ["sql", "mysql", "postgresql", "postgres", "sqlite"];

pub const CONTINUATION_KEYWORDS: [&str; 49] = [
    "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "CROSS", "ON", "AND", "OR", "NOT",
    "IN", "EXISTS", "BETWEEN", "LIKE", "ILIKE", "ORDER", "GROUP", "HAVING", "LIMIT", "OFFSET",
    "UNION", "INTERSECT", "EXCEPT", "AS", "CASE", "WHEN", "THEN", "ELSE", "END", "INTO", "VALUES",
    "SET", "RETURNING", "FETCH", "FOR", "WINDOW", "PARTITION", "OVER", "ROWS", "RANGE", "LATERAL",
    "NATURAL", "USING", "FILTER", "WITHIN", "RECURSIVE", "MATERIALIZED",
];

pub fn starts_sql_statement(line: &str) -> bool {
    let upper = line.trim().to_ascii_uppercase();
    SQL_KEYWORDS.iter().any(|keyword| match upper.strip_prefix(keyword) {
        Some(rest) => match rest.as_bytes().first() {
            None => true,
            Some(next) => !(next.is_ascii_alphanumeric() || *next == b'_'),
        },
        None => false,
    })
}

pub fn looks_like_sql(text: &str) -> bool {
    match text.split('\n').next() {
        Some(first) => starts_sql_statement(first),
        None => false,
    }
}

fn starts_with_ascii_ci(text: &str, at: usize, tag: &str) -> bool {
    let bytes = text.as_bytes();
    if at + tag.len() > bytes.len() {
        return false;
    }
    bytes[at..at + tag.len()].eq_ignore_ascii_case(tag.as_bytes())
}

fn after_fence_markers(text: &str, at: usize, sql_tag: bool) -> Vec<usize> {
    let base = at + 3;
    if sql_tag {
        FENCE_TAGS
            .iter()
            .filter(|tag| starts_with_ascii_ci(text, base, tag))
            .map(|tag| base + tag.len())
            .collect()
    } else {
        vec![base]
    }
}

fn content_start(text: &str, from: usize) -> Option<usize> {
    let mut result = None;
    for (offset, character) in text[from..].char_indices() {
        if character == '\n' {
            result = Some(from + offset + 1);
        }
        if !character.is_whitespace() {
            break;
        }
    }
    result
}

pub fn find_fence(text: &str, sql_tag: bool, closed: bool) -> Option<String> {
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find("```") {
        let marker = cursor + relative;
        for after_marker in after_fence_markers(text, marker, sql_tag) {
            let Some(start) = content_start(text, after_marker) else {
                continue;
            };
            if closed {
                let Some(relative_end) = text[start..].find("```") else {
                    continue;
                };
                return Some(text[start..start + relative_end].to_owned());
            }
            return Some(text[start..].to_owned());
        }
        cursor = marker + 3;
    }
    None
}

fn is_keyword_continuation(upper: &str) -> bool {
    CONTINUATION_KEYWORDS.iter().any(|keyword| {
        upper == *keyword
            || upper.starts_with(&format!("{keyword} "))
            || upper.starts_with(&format!("{keyword}\t"))
    })
}

pub fn is_sql_continuation(line: &str) -> bool {
    let upper = line.to_uppercase();
    let upper = upper.trim_start();
    if is_keyword_continuation(upper) {
        return true;
    }
    let trimmed = line.trim();
    if trimmed.starts_with(')') || trimmed.starts_with(',') || trimmed.starts_with(';') {
        return true;
    }
    if trimmed.starts_with('(')
        || trimmed.starts_with('-')
        || trimmed.starts_with('+')
        || trimmed.starts_with('*')
        || trimmed.starts_with('/')
        || trimmed.starts_with('%')
        || trimmed.starts_with('|')
        || trimmed.starts_with('&')
    {
        return true;
    }
    let leading = line.chars().take_while(|character| character.is_whitespace()).count();
    let leading_bytes = line
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map(|(index, _)| index)
        .unwrap_or(line.len());
    if leading >= 2 && line[leading_bytes..].chars().next().is_some_and(|character| !character.is_whitespace())
    {
        let label = trimmed
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
            && (trimmed.contains(':') || trimmed.contains('：'));
        if !label {
            return true;
        }
    }
    false
}

pub fn extract_sql_lines(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut in_block = false;
    for line in text.split('\n') {
        let stripped = line.trim();
        if stripped.is_empty() {
            if in_block {
                lines.push(String::new());
            }
            continue;
        }
        if starts_sql_statement(stripped) {
            lines.push(line.to_owned());
            in_block = true;
        } else if in_block && is_sql_continuation(stripped) {
            lines.push(line.to_owned());
        } else {
            in_block = false;
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_keywords_require_a_word_boundary() {
        for keyword in SQL_KEYWORDS {
            assert!(starts_sql_statement(keyword));
            assert!(starts_sql_statement(&format!("  {keyword}\nnext")));
        }
        assert!(starts_sql_statement("SELECT * FROM t"));
        assert!(!starts_sql_statement("SELECTED * FROM t"));
        assert!(!starts_sql_statement("select_1"));
        assert!(!starts_sql_statement("  -- SELECT"));
        assert!(looks_like_sql("explain select 1"));
        assert!(!looks_like_sql(""));
    }

    #[test]
    fn fence_tags_are_case_insensitive_and_require_a_newline() {
        assert!(find_fence("```sql\nSELECT 1\n```", true, true).is_some());
        assert!(find_fence("```SQL\nSELECT 1\n```", true, true).is_some());
        assert!(find_fence("```postgresql\nSELECT 1\n```", true, true).is_some());
        assert!(find_fence("```sqlite\nSELECT 1\n```", true, true).is_some());
        assert!(find_fence("```sql SELECT 1```", true, true).is_none());
        assert!(find_fence("```json\n{}\n```", true, true).is_none());
        assert!(find_fence("```json\n{}\n```", false, true).is_none());
        assert!(find_fence("```\n{}\n```", false, true).is_some());
    }

    #[test]
    fn closed_and_unclosed_fences_are_both_supported() {
        assert_eq!(find_fence("```sql\nSELECT 1\n```", true, true).as_deref(), Some("SELECT 1\n"));
        assert_eq!(find_fence("```sql\nSELECT 1", true, false).as_deref(), Some("SELECT 1"));
        assert_eq!(find_fence("```\nSELECT 2\n```", false, true).as_deref(), Some("SELECT 2\n"));
        assert_eq!(find_fence("```\nSELECT 2", false, false).as_deref(), Some("SELECT 2"));
    }

    #[test]
    fn the_first_satisfying_marker_wins() {
        assert_eq!(
            find_fence("```json\n{}\n```\n```sql\nSELECT 1\n```", true, true).as_deref(),
            Some("SELECT 1\n")
        );
    }

    #[test]
    fn continuation_keywords_need_the_exact_delimiter() {
        assert!(is_sql_continuation("FROM users"));
        assert!(is_sql_continuation("FROM\tusers"));
        assert!(is_sql_continuation("WHERE"));
        assert!(!is_sql_continuation("FROMAGE users"));
        assert!(is_sql_continuation("  nested"));
        assert!(!is_sql_continuation("Note: something"));
        assert!(is_sql_continuation("  Note something"));
        assert!(is_sql_continuation(")"));
        assert!(is_sql_continuation(","));
        assert!(is_sql_continuation(";"));
        assert!(is_sql_continuation("( x = 1"));
        assert!(is_sql_continuation("- x = 1"));
        assert!(is_sql_continuation("+ x = 1"));
        assert!(is_sql_continuation("* x = 1"));
        assert!(is_sql_continuation("/ x = 1"));
        assert!(is_sql_continuation("% x = 1"));
        assert!(is_sql_continuation("| x = 1"));
        assert!(is_sql_continuation("& x = 1"));
        assert!(is_sql_continuation("AND x = 1"));
        assert!(!is_sql_continuation("random prose"));
    }

    #[test]
    fn line_extraction_keeps_original_indentation_and_blank_lines_in_block() {
        let text = "intro\nSELECT a,\n  b\nFROM t;\n\nnope\nSELECT 2";
        assert_eq!(extract_sql_lines(text), "SELECT a,\nSELECT 2");
        assert_eq!(extract_sql_lines("nothing here"), "");
    }
}
