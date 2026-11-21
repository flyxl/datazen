use crate::scan_sql;

const CAPTURE_KEYWORDS: [&str; 5] = ["FROM", "JOIN", "INTO", "UPDATE", "TABLE"];
const TERMINATOR_KEYWORDS: [&str; 12] = [
    "SELECT",
    "WHERE",
    "GROUP",
    "ORDER",
    "HAVING",
    "LIMIT",
    "OFFSET",
    "SET",
    "VALUES",
    "RETURNING",
    "ON",
    "USING",
];

pub fn table_clause_fingerprint(sql: &str) -> String {
    if sql.trim().is_empty() {
        return "empty".to_owned();
    }
    let mut parts: Vec<String> = Vec::new();
    let mut capture = false;
    let mut depth = 0usize;
    for token in scan_sql(sql) {
        let text = token.text;
        if text == "(" {
            depth += 1;
            if capture {
                parts.push("(".to_owned());
            }
            continue;
        }
        if text == ")" {
            depth = depth.saturating_sub(1);
            if capture {
                parts.push(")".to_owned());
            }
            continue;
        }
        if text == ";" {
            parts.push(";".to_owned());
            capture = false;
            continue;
        }
        let upper = text.to_uppercase();
        if CAPTURE_KEYWORDS.contains(&upper.as_str()) {
            capture = true;
            parts.push(upper);
            continue;
        }
        if TERMINATOR_KEYWORDS.contains(&upper.as_str()) {
            capture = false;
            parts.push(upper);
            continue;
        }
        if capture && depth == 0 {
            parts.push(text.to_lowercase());
        }
    }
    format!("{}:{}", parts.len(), parts.join("|"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_sql_has_the_empty_fingerprint() {
        assert_eq!(table_clause_fingerprint(""), "empty");
        assert_eq!(table_clause_fingerprint("   \n "), "empty");
    }

    #[test]
    fn select_list_edits_keep_the_fingerprint_stable() {
        let first = table_clause_fingerprint("SELECT a, b FROM users");
        let second = table_clause_fingerprint("SELECT a FROM users");
        assert_eq!(first, second);
    }

    #[test]
    fn table_clause_edits_change_the_fingerprint() {
        let first = table_clause_fingerprint("SELECT a FROM users");
        let second = table_clause_fingerprint("SELECT a FROM orders");
        assert_ne!(first, second);
        assert!(first.contains("FROM"));
        assert!(first.contains("users"));
    }

    #[test]
    fn capture_keywords_and_terminators_are_case_insensitive() {
        let lower = table_clause_fingerprint("select a from t where b = 1");
        let upper = table_clause_fingerprint("SELECT a FROM t WHERE b = 1");
        assert_eq!(lower, upper);
        assert!(lower.contains("WHERE"));
        assert!(!lower.contains("where"));
    }

    #[test]
    fn nested_parentheses_are_not_captured_and_semicolons_reset() {
        let value = table_clause_fingerprint(
            "SELECT * FROM t WHERE x IN (SELECT y FROM z); SELECT * FROM q",
        );
        assert!(value.contains(";"));
        assert!(value.contains("q"));
    }

    #[test]
    fn keyword_text_inside_quotes_does_not_start_capture() {
        let value = table_clause_fingerprint("SELECT 'FROM' AS x");
        assert_eq!(value, "1:SELECT");
    }
}
