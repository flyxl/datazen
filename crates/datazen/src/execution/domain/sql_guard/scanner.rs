//! Lexical helpers shared by safety checks and parameter binding.

pub(crate) fn skip_quoted(chars: &[char], i: usize) -> Option<(usize, char)> {
    let quote = chars[i];
    if quote != '\'' && quote != '"' && quote != '`' {
        return None;
    }
    let mut j = i + 1;
    while j < chars.len() {
        if chars[j] == quote {
            if quote == '\'' && j + 1 < chars.len() && chars[j + 1] == '\'' {
                j += 2;
                continue;
            }
            return Some((j - i + 1, quote));
        }
        j += 1;
    }
    Some((chars.len() - i, quote))
}

/// Skip a PG dollar-quoted string (`$tag$…$tag$`) starting at `i`.
/// Returns `Some(end)` (exclusive) if a valid dollar-quote is found, else `None`.
pub(crate) fn skip_dollar_quoted(chars: &[char], i: usize) -> Option<usize> {
    if chars[i] != '$' {
        return None;
    }
    let mut j = i + 1;
    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
        j += 1;
    }
    if j >= chars.len() || chars[j] != '$' {
        return None;
    }
    let tag_len = j + 1 - i;
    let tag: String = chars[i..i + tag_len].iter().collect();
    let mut k = j + 1;
    while k + tag_len <= chars.len() {
        let candidate: String = chars[k..k + tag_len].iter().collect();
        if candidate == tag {
            return Some(k + tag_len);
        }
        k += 1;
    }
    Some(chars.len())
}

pub(crate) fn push_separator(out: &mut String) {
    if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') && !out.ends_with('\t') {
        out.push(' ');
    }
}

pub(crate) fn split_statements(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut stmts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        if let Some((consumed, _)) = skip_quoted(&chars, i) {
            i += consumed;
            continue;
        }
        if chars[i] == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        if chars[i] == ';' {
            let chunk: String = chars[start..i].iter().collect();
            if !chunk.trim().is_empty() {
                stmts.push(chunk);
            }
            start = i + 1;
        }
        i += 1;
    }
    let chunk: String = chars[start..].iter().collect();
    if !chunk.trim().is_empty() {
        stmts.push(chunk);
    }
    stmts
}

/// Mark byte ranges that must not receive parameter substitution.
pub(crate) fn non_replaceable_ranges(sql: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = sql.chars().collect();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some((consumed, _)) = skip_quoted(&chars, i) {
            ranges.push((i, i + consumed));
            i += consumed;
            continue;
        }
        if chars[i] == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            ranges.push((start, i));
            continue;
        }
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            ranges.push((start, i));
            continue;
        }
        if chars[i] == '$' {
            if let Some(end) = dollar_quote_end(&chars, i) {
                ranges.push((i, end));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    ranges
}

pub(crate) fn is_in_non_replaceable(pos: usize, end: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|&(from, to)| pos < to && end > from)
}

fn dollar_quote_end(chars: &[char], i: usize) -> Option<usize> {
    if chars[i] != '$' {
        return None;
    }
    let mut j = i + 1;
    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
        j += 1;
    }
    if j >= chars.len() || chars[j] != '$' {
        return None;
    }
    let tag_len = j + 1 - i;
    let tag: String = chars[i..i + tag_len].iter().collect();
    let mut k = j + 1;
    while k + tag_len <= chars.len() {
        let candidate: String = chars[k..k + tag_len].iter().collect();
        if candidate == tag {
            return Some(k + tag_len);
        }
        k += 1;
    }
    Some(chars.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_respects_semicolons_inside_strings() {
        let parts = split_statements("SELECT 'a; b'; SELECT 1");
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn marks_dollar_quote_as_non_replaceable() {
        let ranges = non_replaceable_ranges("SELECT $x$ :hidden $x$ , :visible");
        assert!(!ranges.is_empty());
    }
}
