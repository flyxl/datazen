#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlCompletionKind {
    Column,
    Table,
    Any,
}

impl SqlCompletionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SqlCompletionKind::Column => "column",
            SqlCompletionKind::Table => "table",
            SqlCompletionKind::Any => "any",
        }
    }
}

const TABLE_KWS: [&str; 7] = ["from", "join", "into", "update", "table", "using", "truncate"];

const COLUMN_KWS: [&str; 21] = [
    "select", "where", "having", "set", "on", "and", "or", "not", "between", "like",
    "ilike", "in", "when", "returning", "group", "order", "by", "distinct", "values",
    "limit", "offset",
];

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

pub fn last_bare_sql_keyword_with_offset(sql: &str) -> Option<(String, usize)> {
    let chars: Vec<char> = sql.chars().collect();
    let mut last: Option<(String, usize)> = None;
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\'' {
            i += 1;
            while i < chars.len() {
                if chars[i] == '\'' && chars.get(i + 1) == Some(&'\'') {
                    i += 2;
                    continue;
                }
                if chars[i] == '\'' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if ch == '-' && chars.get(i + 1) == Some(&'-') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if ch == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
            continue;
        }
        if is_ident_start(ch) {
            let start = i;
            i += 1;
            while i < chars.len() && is_word_char(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
            if TABLE_KWS.contains(&word.as_str()) || COLUMN_KWS.contains(&word.as_str()) {
                last = Some((word, i));
            }
            continue;
        }
        i += 1;
    }
    last
}

pub fn last_bare_sql_keyword(sql: &str) -> Option<String> {
    last_bare_sql_keyword_with_offset(sql).map(|(kw, _)| kw)
}

fn find_unclosed_paren_after(text: &str, from: usize) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut i = from;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\'' || ch == '"' || ch == '`' {
            let quote = ch;
            i += 1;
            while i < chars.len() {
                if chars[i] == quote && chars.get(i + 1) == Some(&quote) {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if ch == '(' {
            let mut j = i + 1;
            while j < chars.len() {
                let inner = chars[j];
                if inner == '\'' || inner == '"' || inner == '`' {
                    let quote = inner;
                    j += 1;
                    while j < chars.len() {
                        if chars[j] == quote && chars.get(j + 1) == Some(&quote) {
                            j += 2;
                            continue;
                        }
                        if chars[j] == quote {
                            j += 1;
                            break;
                        }
                        j += 1;
                    }
                    continue;
                }
                if inner == ')' {
                    break;
                }
                j += 1;
            }
            if j >= chars.len() {
                return true;
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    false
}

fn in_insert_column_list(stmt: &str) -> bool {
    let lower = stmt.to_lowercase();
    let bytes = lower.as_bytes();
    let mut search = 0usize;
    while let Some(offset) = lower[search..].find("into") {
        let word_start = search + offset;
        let before_ok = word_start == 0 || !is_word_char(bytes[word_start - 1] as char);
        let after = word_start + 4;
        let after_ok = after >= bytes.len() || !is_word_char(bytes[after] as char);
        if before_ok && after_ok {
            let char_from = lower[..after].chars().count();
            if find_unclosed_paren_after(stmt, char_from) {
                return true;
            }
        }
        search = after;
    }
    false
}

fn match_segment(chars: &[char], start: usize) -> Option<usize> {
    let ch = *chars.get(start)?;
    if ch == '"' || ch == '`' || ch == '[' {
        let close = if ch == '[' { ']' } else { ch };
        let mut i = start + 1;
        let mut found = false;
        while i < chars.len() {
            if chars[i] == close {
                found = true;
                break;
            }
            i += 1;
        }
        if found && i > start + 1 {
            return Some(i + 1);
        }
        return None;
    }
    if !is_word_char(ch) {
        return None;
    }
    let mut i = start;
    while i < chars.len() && is_word_char(chars[i]) {
        i += 1;
    }
    Some(i)
}

fn identifier_matches(segment: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = segment.chars().collect();
    let mut matches = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if let Some(end) = match_segment(&chars, i) {
            let mut full_end = end;
            let mut cursor = end;
            loop {
                let mut dot = cursor;
                while dot < chars.len() && (chars[dot] == ' ' || chars[dot] == '\t') {
                    dot += 1;
                }
                if dot >= chars.len() || chars[dot] != '.' {
                    break;
                }
                dot += 1;
                while dot < chars.len() && (chars[dot] == ' ' || chars[dot] == '\t') {
                    dot += 1;
                }
                match match_segment(&chars, dot) {
                    Some(next_end) => {
                        full_end = next_end;
                        cursor = next_end;
                    }
                    None => break,
                }
            }
            matches.push((i, full_end));
            i = full_end;
        } else {
            i += 1;
        }
    }
    matches
}

pub fn is_table_context(tail: &str) -> bool {
    let segment = match tail.rfind(',') {
        Some(index) => &tail[index + 1..],
        None => tail,
    };
    if segment.trim_start().is_empty() {
        return true;
    }
    let matches = identifier_matches(segment);
    if matches.is_empty() {
        return true;
    }
    if matches.len() == 1 {
        let after = segment.char_indices().nth(matches[0].1).map(|(byte, _)| byte).unwrap_or(segment.len());
        return !segment[after..].starts_with(char::is_whitespace);
    }
    false
}

pub fn extract_qualifier_parts(text_before_cursor: &str) -> Vec<String> {
    if text_before_cursor.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = text_before_cursor.chars().collect();
    let mut i = chars.len() - 1;
    if let Some(close) = chars.get(i).copied() {
        if close == '"' || close == '`' || close == ']' {
            let open = if close == ']' { '[' } else { close };
            i = i.saturating_sub(1);
            while i > 0 && chars[i] != open {
                i -= 1;
            }
            if chars.get(i) == Some(&open) {
                i = i.saturating_sub(1);
            }
        } else {
            while i > 0 && is_word_char(chars[i]) {
                i -= 1;
            }
        }
    }
    while i > 0 && (chars[i] == ' ' || chars[i] == '\t') {
        i -= 1;
    }
    if chars.get(i) != Some(&'.') {
        return Vec::new();
    }
    let mut segments = Vec::new();
    loop {
        if chars.get(i) != Some(&'.') {
            break;
        }
        i = i.saturating_sub(1);
        while i > 0 && (chars[i] == ' ' || chars[i] == '\t') {
            i -= 1;
        }
        if i == 0 && !is_word_char(chars[i]) && chars[i] != '"' && chars[i] != '`' && chars[i] != ']' {
            break;
        }
        if chars[i] == '"' || chars[i] == '`' || chars[i] == ']' {
            let close = chars[i];
            let open = if close == ']' { '[' } else { close };
            let end = i;
            i = i.saturating_sub(1);
            while i > 0 && chars[i] != open {
                i -= 1;
            }
            if chars.get(i) == Some(&open) {
                let name: String = chars[i + 1..end].iter().collect();
                let unquoted = if close == '"' {
                    name.replace("\"\"", "\"")
                } else if close == ']' {
                    name.replace("]]", "]")
                } else {
                    name
                };
                segments.insert(0, unquoted);
                i = i.saturating_sub(1);
            } else {
                break;
            }
        } else if is_word_char(chars[i]) {
            let end = i + 1;
            while i > 0 && is_word_char(chars[i]) {
                i -= 1;
            }
            let start = if is_word_char(chars[i]) { i } else { i + 1 };
            let name: String = chars[start..end].iter().collect();
            segments.insert(0, name);
        } else {
            break;
        }
        while i > 0 && (chars[i] == ' ' || chars[i] == '\t') {
            i -= 1;
        }
    }
    segments
}

pub fn infer_sql_completion_kind(text_before_cursor: &str) -> SqlCompletionKind {
    if !extract_qualifier_parts(text_before_cursor).is_empty() {
        return SqlCompletionKind::Any;
    }
    let stmt_start = text_before_cursor.rfind(';').map(|index| index + 1).unwrap_or(0);
    let stmt = &text_before_cursor[stmt_start..];
    if in_insert_column_list(stmt) {
        return SqlCompletionKind::Column;
    }
    let last = match last_bare_sql_keyword_with_offset(stmt) {
        Some(last) => last,
        None => return SqlCompletionKind::Any,
    };
    if TABLE_KWS.contains(&last.0.as_str()) {
        let tail: String = stmt.chars().skip(last.1).collect();
        return if is_table_context(&tail) {
            SqlCompletionKind::Table
        } else {
            SqlCompletionKind::Any
        };
    }
    if COLUMN_KWS.contains(&last.0.as_str()) {
        return SqlCompletionKind::Column;
    }
    SqlCompletionKind::Any
}

pub fn detect_alias_dot_context(text_before_cursor: &str) -> (&'static str, String) {
    let parts = extract_qualifier_parts(text_before_cursor);
    if parts.is_empty() {
        return ("none", String::new());
    }
    if parts.len() == 1 {
        let qualifier = parts.first().cloned().unwrap_or_default();
        return ("unique", qualifier);
    }
    ("none", parts.join("."))
}
