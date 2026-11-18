//! Pure helpers ported 1:1 from the React lib layer for the SqlEditor port.
//!
//! These belong to `lib/sqlPathPrefix.ts` and `lib/sqlEditorContextMenu.ts`
//! (other segments/tasks per the ledger); the component port carries the
//! functions it needs until those files land and absorb them. Kept in a
//! sibling module so the component file stays under the 800-line limit.

/// `isInsideQuotes` from `lib/sqlPathPrefix.ts`.
pub(crate) fn is_inside_quotes(text: &str, pos: usize) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let end = pos.min(text.len());
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0usize;
    while i < chars.len() && chars[i].0 < end {
        let (byte, ch) = chars[i];
        let _ = byte;
        if in_single {
            if ch == '\'' {
                if i + 1 < chars.len() && chars[i + 1].0 < end && chars[i + 1].1 == '\'' {
                    i += 1;
                } else {
                    in_single = false;
                }
            }
        } else if in_double {
            if ch == '"' {
                if i + 1 < chars.len() && chars[i + 1].0 < end && chars[i + 1].1 == '"' {
                    i += 1;
                } else {
                    in_double = false;
                }
            }
        } else if ch == '\'' {
            in_single = true;
        } else if ch == '"' {
            in_double = true;
        }
        i += 1;
    }
    in_single || in_double
}

pub(crate) fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

/// `extractQualifiedToken` from `lib/sqlPathPrefix.ts` (byte offsets).
///
/// The shape follows the React source statement by statement: cursor clamp,
/// the non-ident look (with the end-of-text branch that requires the previous
/// character to be an identifier or dot), the identifier-only back-scan, the
/// `.segment` walk that extends `start` left, and the forward scan from
/// `start` that consumes `ident`/`ident.` pairs.
pub(crate) fn extract_qualified_token(text: &str, pos: usize) -> Option<(String, bool)> {
    if is_inside_quotes(text, pos) {
        return None;
    }
    let len = text.len();
    let mut cursor = pos.min(len);

    if cursor < len {
        let ch = text[cursor..].chars().next()?;
        if !is_ident_char(ch) && ch != '.' {
            if cursor > 0 {
                let prev = text[..cursor].chars().last()?;
                if is_ident_char(prev) || prev == '.' {
                    cursor -= prev.len_utf8();
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
    } else if cursor == len {
        // `else if (cursor === len)`: the character before the end must be an
        // identifier character or a dot, or there is no token here.
        let prev = text.chars().last()?;
        if !is_ident_char(prev) && prev != '.' {
            return None;
        }
    }

    // `while (start > 0 && isIdentChar(text[start - 1])) start--;`
    let mut start = cursor;
    while start > 0 {
        let prev = text[..start].chars().last()?;
        if is_ident_char(prev) {
            start -= prev.len_utf8();
        } else {
            break;
        }
    }

    // The `.segment` walk: extend `start` left across `.ident` groups.
    let mut i = start;
    while i > 0 {
        let prev = text[..i].chars().last()?;
        if prev == '.' {
            let mut k = i - prev.len_utf8();
            while k > 0 {
                let before_dot = text[..k].chars().last()?;
                if is_ident_char(before_dot) {
                    k -= before_dot.len_utf8();
                } else {
                    break;
                }
            }
            if k == i - prev.len_utf8() {
                break;
            }
            i = k;
            continue;
        }
        break;
    }
    start = i;

    // Forward scan from `start`: `ident` runs joined by single dots.
    let mut end = start;
    while end < len {
        let ident_start = end;
        while end < len {
            let ch = text[end..].chars().next()?;
            if is_ident_char(ch) {
                end += ch.len_utf8();
            } else {
                break;
            }
        }
        if end == ident_start {
            break;
        }
        // React checks `end < len` before reading `text[end]`; at end-of-text
        // the token is complete (break), not absent.
        if end < len && text[end..].starts_with('.') {
            let dot_len = text[end..].chars().next().map_or(1, char::len_utf8);
            end += dot_len;
            continue;
        }
        break;
    }

    let value = &text[start..end];
    let first = value.chars().next()?;
    if !is_ident_char(first) {
        return None;
    }
    Some((value.to_string(), value.ends_with('.')))
}

/// `parseQualifiedPathParents` from `lib/sqlPathPrefix.ts` (byte offsets).
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub(crate) fn parse_qualified_path_parents(text: &str, cursor: usize) -> Vec<String> {
    let pos = cursor.min(text.len());
    if is_inside_quotes(text, pos) {
        return Vec::new();
    }
    let Some((value, ends_with_dot)) = extract_qualified_token(text, pos) else {
        return Vec::new();
    };
    let segments: Vec<String> =
        value.split('.').filter(|s| !s.is_empty()).map(String::from).collect();
    if ends_with_dot {
        return segments;
    }
    if segments.len() <= 1 {
        return Vec::new();
    }
    segments[..segments.len() - 1].to_vec()
}

/// `toggleSqlLineComments` from `lib/sqlEditorContextMenu.ts`.
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub(crate) fn toggle_sql_line_comments(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let non_empty: Vec<&str> = lines.iter().copied().filter(|l| !l.trim().is_empty()).collect();
    let all_commented =
        !non_empty.is_empty() && non_empty.iter().all(|l| l.trim_start().starts_with("--"));
    if all_commented {
        return lines
            .iter()
            .map(|l| {
                let indent_end = l.len() - l.trim_start().len();
                let rest = l[indent_end..].strip_prefix("--").unwrap_or(&l[indent_end..]);
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                format!("{}{}", &l[..indent_end], rest)
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::from(*l)
            } else {
                format!("-- {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
