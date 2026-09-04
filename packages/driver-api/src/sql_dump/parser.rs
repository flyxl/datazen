//! SQL statement parsing helpers for dump/restore.

pub fn created_relation_ident(stmt: &str) -> Option<String> {
    let line = stmt
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("--") && !l.starts_with("/*"))
        .unwrap_or("");
    let rest = strip_kw(line, "CREATE")?;
    let rest = strip_opt_kw(rest, "OR REPLACE");
    let rest = if let Some(after) = strip_kw(rest, "MATERIALIZED") {
        strip_kw(after, "VIEW")?
    } else if let Some(after) = strip_kw(rest, "TABLE") {
        after
    } else {
        strip_kw(rest, "VIEW")?
    };
    let rest = strip_opt_kw(rest, "IF NOT EXISTS");
    take_qualified_sql_ident(rest)
}

pub(crate) fn strip_kw<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let s = s.trim_start();
    if s.len() < kw.len() || !s[..kw.len()].eq_ignore_ascii_case(kw) {
        return None;
    }
    Some(s[kw.len()..].trim_start())
}

pub(crate) fn strip_opt_kw<'a>(s: &'a str, kw: &str) -> &'a str {
    strip_kw(s, kw).unwrap_or(s)
}

pub(crate) fn take_sql_ident(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    let bytes = s.as_bytes();
    if bytes.first().copied() == Some(b'"') || bytes.first().copied() == Some(b'`') {
        let q = bytes[0];
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == q {
                if i + 1 < bytes.len() && bytes[i + 1] == q {
                    i += 2;
                    continue;
                }
                return Some((&s[..=i], &s[i + 1..]));
            }
            i += 1;
        }
        return None;
    }
    let end = s
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((&s[..end], &s[end..]))
}

pub(crate) fn take_qualified_sql_ident(s: &str) -> Option<String> {
    let (first, rest) = take_sql_ident(s)?;
    let rest = rest.trim_start();
    if let Some(after_dot) = rest.strip_prefix('.') {
        let (second, _) = take_sql_ident(after_dot)?;
        Some(format!("{first}.{second}"))
    } else {
        Some(first.to_string())
    }
}

pub(crate) fn relation_already_exists(error: &str) -> bool {
    let msg = error.to_lowercase();
    msg.contains("already exists")
        || msg.contains("duplicate")
        || msg.contains("1050")
        || msg.contains("42p07")
}

pub(crate) fn take_sql_string_literal(s: &str) -> Option<String> {
    let s = s.trim_start();
    if !s.starts_with('\'') {
        return None;
    }
    let bytes = s.as_bytes();
    let mut i = 1;
    let mut out = String::new();
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                out.push('\'');
                i += 2;
                continue;
            }
            return Some(out);
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    None
}
