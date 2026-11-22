use super::StatementResultView;

pub const QUERY_VERBS: [&str; 7] = [
    "SELECT", "SHOW", "EXPLAIN", "DESC", "DESCRIBE", "PRAGMA", "VALUES",
];

pub const MUTATION_VERBS: [&str; 18] = [
    "INSERT", "UPDATE", "DELETE", "CREATE", "ALTER", "DROP", "TRUNCATE", "RENAME", "GRANT",
    "REVOKE", "SET", "BEGIN", "COMMIT", "ROLLBACK", "USE", "MERGE", "UPSERT", "REPLACE",
];

fn is_verb(list: &[&str], value: &str) -> bool {
    list.iter().any(|verb| *verb == value)
}

pub fn strip_leading_sql_comments(sql: &str) -> String {
    let mut text = sql.trim();
    loop {
        if text.starts_with("--") || text.starts_with('#') {
            match text.find('\n') {
                None => return String::new(),
                Some(index) => text = text[index + 1..].trim(),
            }
        } else if text.starts_with("/*") {
            match text.find("*/") {
                None => return String::new(),
                Some(index) => text = text[index + 2..].trim(),
            }
        } else {
            return text.to_owned();
        }
    }
}

fn leading_word(chars: &[char], index: usize) -> Option<(String, usize)> {
    if chars.get(index).is_none_or(|ch| !ch.is_ascii_alphabetic()) {
        return None;
    }
    let mut end = index;
    while end < chars.len() && chars[end].is_ascii_alphabetic() {
        end += 1;
    }
    Some((
        chars[index..end].iter().collect::<String>().to_uppercase(),
        end,
    ))
}

pub fn get_top_level_sql_verb(sql: &str) -> String {
    let clean = strip_leading_sql_comments(sql);
    if clean.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = clean.chars().collect();
    let (first, end) = match leading_word(&chars, 0) {
        Some(value) => value,
        None => return String::new(),
    };
    if first != "WITH" {
        return first;
    }

    let mut index = end;
    let mut depth: i32 = 0;
    let mut in_single = false;
    let mut in_double = false;
    while index < chars.len() {
        let ch = chars[index];
        let next = chars.get(index + 1).copied();

        if in_single {
            if ch == '\'' && next == Some('\'') {
                index += 2;
                continue;
            }
            if ch == '\'' {
                in_single = false;
            }
            index += 1;
            continue;
        }
        if in_double {
            if ch == '"' && next == Some('"') {
                index += 2;
                continue;
            }
            if ch == '"' {
                in_double = false;
            }
            index += 1;
            continue;
        }
        if ch == '\'' {
            in_single = true;
            index += 1;
            continue;
        }
        if ch == '"' {
            in_double = true;
            index += 1;
            continue;
        }
        if ch == '-' && next == Some('-') {
            match chars[index..].iter().position(|value| *value == '\n') {
                None => break,
                Some(offset) => {
                    index += offset + 1;
                    continue;
                }
            }
        }
        if ch == '/' && next == Some('*') {
            match chars[index + 2..]
                .windows(2)
                .position(|pair| pair == ['*', '/'])
            {
                None => break,
                Some(offset) => {
                    index += offset + 4;
                    continue;
                }
            }
        }
        if ch == '(' {
            depth += 1;
            index += 1;
            continue;
        }
        if ch == ')' {
            depth -= 1;
            index += 1;
            continue;
        }
        if depth == 0 && ch.is_ascii_alphabetic() {
            if let Some((word, next_index)) = leading_word(&chars, index) {
                if word == "RECURSIVE" || word == "AS" {
                    index = next_index;
                    continue;
                }
                if is_verb(&QUERY_VERBS, &word) || is_verb(&MUTATION_VERBS, &word) {
                    return word;
                }
                index = next_index;
                continue;
            }
        }
        index += 1;
    }
    "SELECT".to_owned()
}

pub fn is_mutation_execution(result: &StatementResultView) -> bool {
    if !result.columns.is_empty() || !result.rows.is_empty() {
        return false;
    }
    if !result.sql.is_empty() {
        let verb = get_top_level_sql_verb(&result.sql);
        if is_verb(&QUERY_VERBS, &verb) {
            return false;
        }
        if is_verb(&MUTATION_VERBS, &verb) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_comments_are_stripped_in_all_three_forms() {
        assert_eq!(strip_leading_sql_comments("  select 1 "), "select 1");
        assert_eq!(strip_leading_sql_comments("-- c\n select 1"), "select 1");
        assert_eq!(strip_leading_sql_comments("# c\nselect 1"), "select 1");
        assert_eq!(strip_leading_sql_comments("/* c */ select 1"), "select 1");
        assert_eq!(
            strip_leading_sql_comments("/* c */\n-- d\nselect 1"),
            "select 1"
        );
        assert_eq!(strip_leading_sql_comments("-- unterminated"), "");
        assert_eq!(strip_leading_sql_comments("/* unterminated"), "");
    }

    #[test]
    fn top_level_verbs_cover_plain_query_and_mutation_statements() {
        assert_eq!(get_top_level_sql_verb("SELECT 1"), "SELECT");
        assert_eq!(get_top_level_sql_verb("  update t set a=1"), "UPDATE");
        assert_eq!(get_top_level_sql_verb("-- c\nDELETE FROM t"), "DELETE");
        assert_eq!(get_top_level_sql_verb(""), "");
    }

    #[test]
    fn cte_bodies_are_skipped_to_the_main_verb() {
        assert_eq!(
            get_top_level_sql_verb("WITH x AS (SELECT 1) SELECT * FROM x"),
            "SELECT"
        );
        assert_eq!(
            get_top_level_sql_verb("WITH RECURSIVE x AS (SELECT 1) INSERT INTO t SELECT * FROM x"),
            "INSERT"
        );
        assert_eq!(
            get_top_level_sql_verb("WITH x AS (SELECT 'a)b') UPDATE t SET a=1"),
            "UPDATE"
        );
        assert_eq!(get_top_level_sql_verb("WITH x AS (SELECT 1)"), "SELECT");
    }

    #[test]
    fn mutations_require_no_tabular_output_and_a_known_verb() {
        let empty = StatementResultView {
            sql: "DELETE FROM t".to_owned(),
            ..Default::default()
        };
        assert!(is_mutation_execution(&empty));

        let with_rows = StatementResultView {
            rows: vec![vec![Some(super::super::ResultValue::Number(
                "1".to_owned(),
            ))]],
            ..empty.clone()
        };
        assert!(!is_mutation_execution(&with_rows));
        let with_columns = StatementResultView {
            columns: vec![super::super::ResultColumn {
                name: "a".to_owned(),
                data_type: "int".to_owned(),
                nullable: false,
            }],
            ..empty.clone()
        };
        assert!(!is_mutation_execution(&with_columns));

        let select = StatementResultView {
            sql: "SELECT 1".to_owned(),
            ..Default::default()
        };
        assert!(!is_mutation_execution(&select));
        let unknown = StatementResultView {
            sql: "VACUUM".to_owned(),
            ..Default::default()
        };
        assert!(!is_mutation_execution(&unknown));
        let no_sql = StatementResultView::default();
        assert!(!is_mutation_execution(&no_sql));
    }
}
