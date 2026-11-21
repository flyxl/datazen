use super::types::{SqlToken, SqlTokenKind};

fn is_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{000c}')
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn match_dollar_tag(chars: &[char], start: usize) -> Option<String> {
    if chars.get(start) != Some(&'$') {
        return None;
    }
    let mut end = start + 1;
    while end < chars.len() && is_ident_char(chars[end]) {
        end += 1;
    }
    if end >= chars.len() || chars[end] != '$' {
        return None;
    }
    Some(chars[start..=end].iter().collect())
}

fn push_token(
    tokens: &mut Vec<SqlToken>,
    kind: SqlTokenKind,
    from: usize,
    to: usize,
    chars: &[char],
    depth: usize,
) {
    if from >= to {
        return;
    }
    tokens.push(SqlToken {
        kind,
        from,
        to,
        text: chars[from..to].iter().collect(),
        paren_depth: depth,
    });
}

fn is_other_break(ch: char, next: Option<char>) -> bool {
    (ch == '-' && next == Some('-'))
        || ch == '#'
        || (ch == '/' && next == Some('*'))
        || matches!(ch, '\'' | '"' | '`' | '[' | ';' | '(' | ')' | ',')
        || is_whitespace(ch)
}

pub fn normalize_fullwidth(sql: &str) -> String {
    sql.chars()
        .map(|ch| {
            let point = ch as u32;
            if (0xff01..=0xff5e).contains(&point) {
                char::from_u32(point - 0xfee0).unwrap_or(ch)
            } else {
                ch
            }
        })
        .collect()
}

pub fn scan_sql(source: &str) -> Vec<SqlToken> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        let next = chars.get(i + 1).copied();

        if is_whitespace(ch) {
            while i < chars.len() && is_whitespace(chars[i]) {
                i += 1;
            }
            continue;
        }

        if ch == '-' && next == Some('-') {
            let start = i;
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            push_token(
                &mut tokens,
                SqlTokenKind::LineComment,
                start,
                i,
                &chars,
                depth,
            );
            continue;
        }

        if ch == '#' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            push_token(
                &mut tokens,
                SqlTokenKind::LineComment,
                start,
                i,
                &chars,
                depth,
            );
            continue;
        }

        if ch == '/' && next == Some('*') {
            let start = i;
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < chars.len() {
                i += 2;
            }
            push_token(
                &mut tokens,
                SqlTokenKind::BlockComment,
                start,
                i,
                &chars,
                depth,
            );
            continue;
        }

        if matches!(ch, '\'' | '"' | '`' | '[') {
            let start = i;
            i += 1;
            let close = if ch == '[' { ']' } else { ch };
            let doubled = matches!(ch, '\'' | '"' | '[');
            while i < chars.len() {
                if chars[i] == close {
                    if doubled && i + 1 < chars.len() && chars[i + 1] == close {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            let kind = match ch {
                '\'' => SqlTokenKind::SingleQuoted,
                '"' => SqlTokenKind::DoubleQuoted,
                '`' => SqlTokenKind::BacktickQuoted,
                _ => SqlTokenKind::BracketQuoted,
            };
            push_token(&mut tokens, kind, start, i, &chars, depth);
            continue;
        }

        if ch == '$' {
            if let Some(tag) = match_dollar_tag(&chars, i) {
                let start = i;
                let width = tag.chars().count();
                let mut j = i + width;
                let mut closed = false;
                while j < chars.len() {
                    if chars[j] == '$' {
                        if let Some(candidate) = match_dollar_tag(&chars, j) {
                            if candidate == tag {
                                j += candidate.chars().count();
                                closed = true;
                                break;
                            }
                        }
                    }
                    j += 1;
                }
                if closed {
                    push_token(
                        &mut tokens,
                        SqlTokenKind::DollarQuoted,
                        start,
                        j,
                        &chars,
                        depth,
                    );
                    i = j;
                    continue;
                }
            }
        }

        if ch == ';' {
            push_token(
                &mut tokens,
                SqlTokenKind::Semicolon,
                i,
                i + 1,
                &chars,
                depth,
            );
            i += 1;
            continue;
        }

        if ch == '(' {
            push_token(
                &mut tokens,
                SqlTokenKind::OpenParen,
                i,
                i + 1,
                &chars,
                depth,
            );
            depth += 1;
            i += 1;
            continue;
        }

        if ch == ')' {
            push_token(
                &mut tokens,
                SqlTokenKind::CloseParen,
                i,
                i + 1,
                &chars,
                depth,
            );
            depth = depth.saturating_sub(1);
            i += 1;
            continue;
        }

        if ch == ',' {
            push_token(&mut tokens, SqlTokenKind::Other, i, i + 1, &chars, depth);
            i += 1;
            continue;
        }

        let start = i;
        while i < chars.len() && !is_other_break(chars[i], chars.get(i + 1).copied()) {
            i += 1;
        }
        push_token(&mut tokens, SqlTokenKind::Other, start, i, &chars, depth);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_words_parens_and_semicolons_with_depth() {
        let tokens = scan_sql("SELECT * FROM t WHERE (a=1); DROP TABLE x");
        let kinds: Vec<_> = tokens.iter().map(|tok| tok.kind).collect();
        assert!(kinds.contains(&SqlTokenKind::Semicolon));
        assert!(kinds.contains(&SqlTokenKind::OpenParen));
        assert!(kinds.contains(&SqlTokenKind::CloseParen));
        let semi = tokens
            .iter()
            .find(|tok| tok.kind == SqlTokenKind::Semicolon)
            .unwrap();
        assert_eq!(semi.paren_depth, 0);
        let drop = tokens.iter().find(|tok| tok.text == "DROP").unwrap();
        assert_eq!(drop.text, "DROP");
    }

    #[test]
    fn semicolons_inside_literals_and_comments_are_not_tokens() {
        let tokens = scan_sql("SELECT 'a;b', \"c;d\"; /* e;f */ SELECT 1 -- g;h\n; SELECT 2");
        let semis: Vec<_> = tokens
            .iter()
            .filter(|tok| tok.kind == SqlTokenKind::Semicolon)
            .collect();
        assert_eq!(semis.len(), 2);
    }

    #[test]
    fn handles_bracket_backtick_and_dollar_quotes() {
        let tokens = scan_sql("SELECT [a;b], `c;d`, $tag$e;f$tag$; SELECT 2");
        assert_eq!(
            tokens
                .iter()
                .filter(|tok| tok.kind == SqlTokenKind::Semicolon)
                .count(),
            1
        );
        assert!(tokens
            .iter()
            .any(|tok| tok.kind == SqlTokenKind::BracketQuoted));
        assert!(tokens
            .iter()
            .any(|tok| tok.kind == SqlTokenKind::BacktickQuoted));
        assert!(tokens
            .iter()
            .any(|tok| tok.kind == SqlTokenKind::DollarQuoted));
    }

    #[test]
    fn fullwidth_latin_maps_to_ascii() {
        assert_eq!(normalize_fullwidth("ＤＲＯＰ"), "DROP");
        assert_eq!(normalize_fullwidth("SELECT 你好"), "SELECT 你好");
        assert_eq!(normalize_fullwidth(""), "");
    }
}
