use std::collections::BTreeSet;

use crate::query_context_path::types::QueryContextResolveOpts;
use crate::sql_path_prefix::extract_qualified_token;

const RELATION_KEYWORDS: [&str; 4] = ["from", "join", "update", "into"];

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub fn unquote_ident(segment: &str) -> String {
    let trimmed = segment.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'`' && bytes[bytes.len() - 1] == b'`'))
    {
        let inner = &trimmed[1..trimmed.len() - 1];
        return inner.replace("\"\"", "\"").replace("``", "`");
    }
    trimmed.to_owned()
}

fn read_ident(input: &str, start: usize) -> Option<(String, usize)> {
    let bytes = input.as_bytes();
    match bytes.get(start).copied()? {
        b'"' | b'`' => {
            let quote = bytes[start];
            let mut index = start + 1;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            if index >= bytes.len() {
                return None;
            }
            Some((input[start + 1..index].to_owned(), index + 1))
        }
        byte if byte.is_ascii_alphabetic() || byte == b'_' => {
            let mut index = start + 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_' || bytes[index] == b'$')
            {
                index += 1;
            }
            Some((input[start..index].to_owned(), index))
        }
        _ => None,
    }
}

pub fn infer_sql_relation_path(sql: &str) -> Vec<String> {
    let bytes = sql.as_bytes();
    let mut last: Vec<String> = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if index > 0 && is_word(bytes[index - 1]) {
            index += 1;
            continue;
        }
        let Some(keyword) = RELATION_KEYWORDS
            .iter()
            .find(|candidate| {
                index + candidate.len() <= bytes.len()
                    && sql[index..index + candidate.len()].eq_ignore_ascii_case(candidate)
            })
        else {
            index += 1;
            continue;
        };
        let mut cursor = index + keyword.len();
        if cursor >= bytes.len() || !bytes[cursor].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor + 4 <= bytes.len()
            && sql[cursor..cursor + 4].eq_ignore_ascii_case("only")
            && bytes.get(cursor + 4).is_some_and(u8::is_ascii_whitespace)
        {
            cursor += 4;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
        }
        let Some((first, mut end)) = read_ident(sql, cursor) else {
            index += 1;
            continue;
        };
        let mut segments = vec![unquote_ident(&first)];
        loop {
            let mut dot = end;
            while dot < bytes.len() && bytes[dot].is_ascii_whitespace() {
                dot += 1;
            }
            if bytes.get(dot) != Some(&b'.') {
                break;
            }
            dot += 1;
            while dot < bytes.len() && bytes[dot].is_ascii_whitespace() {
                dot += 1;
            }
            let Some((segment, next)) = read_ident(sql, dot) else {
                break;
            };
            segments.push(unquote_ident(&segment));
            end = next;
        }
        last = segments.into_iter().filter(|segment| !segment.is_empty()).collect();
        index = end;
    }
    last
}

fn roots_with<'a>(opts: &'a QueryContextResolveOpts<'a>) -> BTreeSet<&'a str> {
    opts.databases
        .iter()
        .chain(opts.namespace_roots.iter())
        .map(String::as_str)
        .collect()
}

fn from_known_root(segments: Vec<String>, roots: &BTreeSet<&str>) -> Option<Vec<String>> {
    let first = segments.first()?;
    if roots.contains(first.as_str()) {
        Some(segments)
    } else {
        None
    }
}

pub fn resolve_query_context_path(
    sql: &str,
    opts: &QueryContextResolveOpts<'_>,
) -> Option<Vec<String>> {
    let roots = roots_with(opts);
    if let Some(token) = extract_qualified_token(sql, sql.len()) {
        let segments: Vec<String> = token
            .value
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(str::to_owned)
            .collect();
        let parents = if token.ends_with_dot {
            segments
        } else {
            let end = segments.len().saturating_sub(1);
            segments[..end].to_vec()
        };
        if let Some(path) = from_known_root(parents, &roots) {
            return Some(path);
        }
    }
    let path = infer_sql_relation_path(sql);
    if path.len() < 2 {
        return None;
    }
    let end = path.len() - 1;
    from_known_root(path[..end].to_vec(), &roots)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts<'a>(databases: &'a [String], roots: &'a [String]) -> QueryContextResolveOpts<'a> {
        QueryContextResolveOpts { databases, namespace_roots: roots }
    }

    #[test]
    fn unquoting_handles_double_and_backtick_escapes() {
        assert_eq!(unquote_ident("  \"a\"\"b\"  "), "a\"b");
        assert_eq!(unquote_ident("`a``b`"), "a`b");
        assert_eq!(unquote_ident("plain"), "plain");
        assert_eq!(unquote_ident("  spaced  "), "spaced");
    }

    #[test]
    fn the_last_relation_keyword_wins() {
        assert_eq!(
            infer_sql_relation_path("SELECT * FROM first JOIN second"),
            vec!["second"]
        );
        assert_eq!(
            infer_sql_relation_path("UPDATE t SET x = 1"),
            vec!["t"]
        );
        assert_eq!(
            infer_sql_relation_path("INSERT INTO target VALUES (1)"),
            vec!["target"]
        );
        assert!(infer_sql_relation_path("SELECT 1").is_empty());
    }

    #[test]
    fn qualified_relations_split_and_unquote_every_segment() {
        assert_eq!(
            infer_sql_relation_path("FROM trading_dev.t_order"),
            vec!["trading_dev", "t_order"]
        );
        assert_eq!(
            infer_sql_relation_path("from \"My DB\".`my table`"),
            vec!["My DB", "my table"]
        );
        assert_eq!(
            infer_sql_relation_path("FROM ONLY cat.sch.tbl"),
            vec!["cat", "sch", "tbl"]
        );
        assert_eq!(
            infer_sql_relation_path("from a . b . c"),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn a_keyword_inside_an_identifier_is_not_a_relation() {
        assert!(infer_sql_relation_path("SELECT fromage FROM t").len() == 1);
        assert_eq!(infer_sql_relation_path("SELECT x FROM t"), vec!["t"]);
    }

    #[test]
    fn trailing_dot_token_resolves_immediately() {
        let databases = vec!["hive".to_owned()];
        let roots = Vec::new();
        assert_eq!(
            resolve_query_context_path("SELECT * FROM hive.snap.", &opts(&databases, &roots)),
            Some(vec!["hive".to_owned(), "snap".to_owned()])
        );
    }

    #[test]
    fn complete_relation_resolves_when_the_first_segment_is_a_root() {
        let databases = vec!["hive".to_owned()];
        let roots = Vec::new();
        assert_eq!(
            resolve_query_context_path("SELECT * FROM hive.snap.orders", &opts(&databases, &roots)),
            Some(vec!["hive".to_owned(), "snap".to_owned()])
        );
    }

    #[test]
    fn unqualified_and_unknown_root_sql_yield_no_context() {
        let databases = vec!["hive".to_owned()];
        let roots = Vec::new();
        assert_eq!(resolve_query_context_path("SELECT * FROM orders", &opts(&databases, &roots)), None);
        assert_eq!(resolve_query_context_path("SELECT * FROM public.users", &opts(&databases, &roots)), None);
        assert_eq!(resolve_query_context_path("", &opts(&databases, &roots)), None);
    }

    #[test]
    fn namespace_roots_join_the_databases_as_valid_roots() {
        let databases = Vec::new();
        let roots = vec!["catalog".to_owned()];
        assert_eq!(
            resolve_query_context_path("SELECT * FROM catalog.sch.t", &opts(&databases, &roots)),
            Some(vec!["catalog".to_owned(), "sch".to_owned()])
        );
    }
}
